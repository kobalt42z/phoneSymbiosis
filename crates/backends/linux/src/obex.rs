//! Shared BlueZ OBEX helpers (sessions, transfers) used by [`contacts`](crate::contacts)
//! and [`messages`](crate::messages).
//!
//! ## How BlueZ OBEX works
//!
//! Contacts (PBAP) and SMS (MAP) don't go through the main `bluetoothd`
//! daemon but through **obexd**, a separate per-user daemon on the **session**
//! bus under the name `org.bluez.obex` (Ubuntu package `bluez-obexd`, usually
//! started on demand by D-Bus activation).
//!
//! 1. `org.bluez.obex.Client1.CreateSession(address, {"Target": "pbap"|"map"})`
//!    opens an OBEX connection to the phone and returns a *session* object.
//! 2. Profile-specific methods on that session (`PhonebookAccess1`,
//!    `MessageAccess1`) start *transfers*. A transfer is its own object with an
//!    `org.bluez.obex.Transfer1` interface whose `Status` property moves
//!    `queued -> active -> complete | error`.
//! 3. Downloads land in a file (obexd picks a temp file when we pass an empty
//!    name and tells us where in the `Filename` property). We read it, then
//!    delete it: nothing is cached on disk.
//!
//! obexd closes sessions by itself when the phone goes away and when our
//! D-Bus connection closes, so we keep one session per (phone, profile) and
//! simply recreate it when a call reports it is gone.
//!
//! ## iPhone permissions
//!
//! An iPhone only answers PBAP when **"Sync Contacts"** is switched on and MAP
//! when **"Show Notifications"** is switched on, both in
//! *Settings > Bluetooth > (i) next to this PC*. When they are off the phone
//! refuses the OBEX connection or the transfer; we turn that into
//! [`Error::Refused`] carrying that hint (see [`Target::hint`]).

use crate::dbus_util::{self, PropMap};
use futures_util::StreamExt;
use phonesym_core::{Address, Error, Result};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};
use zbus::message::Type as MsgType;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

/// Bus name of obexd on the session bus.
pub const SERVICE: &str = "org.bluez.obex";
/// Object implementing [`CLIENT_IFACE`].
pub const CLIENT_PATH: &str = "/org/bluez/obex";
pub const CLIENT_IFACE: &str = "org.bluez.obex.Client1";
pub const SESSION_IFACE: &str = "org.bluez.obex.Session1";
pub const TRANSFER_IFACE: &str = "org.bluez.obex.Transfer1";
pub const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
pub const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";

/// How long opening an OBEX session may take (the phone may need paging and
/// may show a permission prompt).
const SESSION_TIMEOUT: Duration = Duration::from_secs(45);

/// Which OBEX profile a session is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// Phone Book Access Profile: contacts and call history.
    Pbap,
    /// Message Access Profile: SMS.
    Map,
}

impl Target {
    /// Value of the `Target` key in `CreateSession`.
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Pbap => "pbap",
            Target::Map => "map",
        }
    }

    /// What the user most likely needs to change on the phone when this
    /// profile is refused.
    pub fn hint(self) -> &'static str {
        match self {
            Target::Pbap => {
                "on the iPhone open Settings > Bluetooth > (i) next to this PC and turn on \"Sync Contacts\" \
                 (on Android allow contact access when asked)"
            }
            Target::Map => {
                "on the iPhone open Settings > Bluetooth > (i) next to this PC and turn on \"Show Notifications\" \
                 (on Android allow message access when asked)"
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Target::Pbap => "contacts (PBAP)",
            Target::Map => "messages (MAP)",
        }
    }
}

/// A lazily connected obexd client that keeps one session per (phone, profile).
///
/// The session bus is only contacted on first use, so building the
/// [`Platform`](phonesym_core::Platform) never fails just because obexd or a
/// session bus is missing; the error shows up when contacts/SMS are used.
pub struct ObexClient {
    conn: OnceCell<Connection>,
    sessions: Mutex<HashMap<(Address, Target), OwnedObjectPath>>,
    /// Serialises operations. PBAP `Select` + `PullAll` and MAP `SetFolder` +
    /// `ListMessages` change per-session state ("current folder"), so two
    /// interleaved operations could read the wrong folder.
    op_lock: Mutex<()>,
}

impl Default for ObexClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ObexClient {
    pub fn new() -> Self {
        Self {
            conn: OnceCell::new(),
            sessions: Mutex::new(HashMap::new()),
            op_lock: Mutex::new(()),
        }
    }

    /// The session-bus connection, opened on first use.
    pub async fn connection(&self) -> Result<Connection> {
        self.conn
            .get_or_try_init(|| async {
                Connection::session().await.map_err(|e| {
                    Error::ServiceUnavailable(format!(
                        "cannot reach the D-Bus session bus needed for obexd (contacts/SMS): {e}"
                    ))
                })
            })
            .await
            .cloned()
    }

    /// The cached session for `(phone, target)`, or a new one.
    pub async fn session(&self, phone: &Address, target: Target) -> Result<OwnedObjectPath> {
        let key = (phone.clone(), target);
        if let Some(path) = self.sessions.lock().await.get(&key) {
            return Ok(path.clone());
        }
        let conn = self.connection().await?;
        let path = create_session(&conn, phone, target).await?;
        tracing::debug!(%phone, target = target.as_str(), session = %path.as_str(), "OBEX session created");
        self.sessions.lock().await.insert(key, path.clone());
        Ok(path)
    }

    /// Forget (and politely close) a session, e.g. after it stopped working.
    pub async fn drop_session(&self, phone: &Address, target: Target) {
        let removed = self.sessions.lock().await.remove(&(phone.clone(), target));
        if let (Some(path), Ok(conn)) = (removed, self.connection().await) {
            // Fails harmlessly when obexd already removed it.
            let _ = conn
                .call_method(
                    Some(SERVICE),
                    CLIENT_PATH,
                    Some(CLIENT_IFACE),
                    "RemoveSession",
                    &(path,),
                )
                .await;
        }
    }

    /// Run `op` with a session for `(phone, target)` while holding the operation lock.
    ///
    /// If `op` fails with [`Error::NotConnected`] (what [`call`] reports when
    /// the session object vanished, e.g. the phone walked out of range and
    /// came back), the cached session is dropped and `op` is retried once on a
    /// fresh one.
    pub async fn with_session<T, F, Fut>(&self, phone: &Address, target: Target, op: F) -> Result<T>
    where
        F: Fn(Connection, OwnedObjectPath) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let _guard = self.op_lock.lock().await;
        let conn = self.connection().await?;
        let session = self.session(phone, target).await?;
        match op(conn.clone(), session).await {
            Err(Error::NotConnected(why)) => {
                tracing::debug!(%phone, target = target.as_str(), %why, "OBEX session lost, reopening");
                self.drop_session(phone, target).await;
                let session = self.session(phone, target).await?;
                op(conn, session).await
            }
            other => other,
        }
    }
}

/// `Client1.CreateSession`, with errors turned into something a user can act on.
async fn create_session(
    conn: &Connection,
    phone: &Address,
    target: Target,
) -> Result<OwnedObjectPath> {
    let mut args: HashMap<&str, Value> = HashMap::new();
    args.insert("Target", Value::from(target.as_str()));
    let body = (phone.as_str(), args);
    let call = conn.call_method(
        Some(SERVICE),
        CLIENT_PATH,
        Some(CLIENT_IFACE),
        "CreateSession",
        &body,
    );
    let reply = match tokio::time::timeout(SESSION_TIMEOUT, call).await {
        Err(_) => {
            return Err(Error::NotConnected(format!(
                "{phone}: opening {} timed out; is the phone nearby and unlocked? ({})",
                target.label(),
                target.hint()
            )))
        }
        Ok(Err(e)) => return Err(session_error(phone, target, e)),
        Ok(Ok(reply)) => reply,
    };
    reply
        .body()
        .deserialize::<OwnedObjectPath>()
        .map_err(|e| Error::Backend(format!("CreateSession: unexpected reply: {e}")))
}

/// Classify a `CreateSession` failure. obexd mostly reports
/// `org.bluez.obex.Error.Failed` with a socket error text, so we look at the
/// message too: "Connection refused" is what a phone with the feature
/// switched off typically produces.
fn session_error(phone: &Address, target: Target, e: zbus::Error) -> Error {
    let zbus::Error::MethodError(name, msg, _) = &e else {
        return Error::Backend(format!("{phone}: opening {}: {e}", target.label()));
    };
    let name = name.as_str();
    let msg = msg.clone().unwrap_or_default();
    let lower = msg.to_ascii_lowercase();
    let what = format!("{phone}: opening {}", target.label());

    if name.ends_with("ServiceUnknown")
        || name.ends_with("NameHasNoOwner")
        || name.contains(".Spawn.")
    {
        Error::ServiceUnavailable(format!(
            "{what}: obexd (org.bluez.obex) is not available on the session bus; install the `bluez-obexd` package"
        ))
    } else if [
        "refused",
        "forbidden",
        "unauthorized",
        "not authorized",
        "rejected",
        "permission",
        "not allowed",
    ]
    .iter()
    .any(|w| lower.contains(w))
    {
        Error::Refused(format!("{what}: {msg}. Hint: {}", target.hint()))
    } else if [
        "host is down",
        "no route",
        "timed out",
        "timeout",
        "not connected",
        "unreachable",
        "reset by peer",
    ]
    .iter()
    .any(|w| lower.contains(w))
    {
        Error::NotConnected(format!(
            "{what}: {msg}. Is the phone in range, paired and connected?"
        ))
    } else {
        match dbus_util::map_err(&what, e) {
            Error::Backend(m) => {
                Error::Backend(format!("{m}. If this keeps happening: {}", target.hint()))
            }
            other => other,
        }
    }
}

/// Call a method on obexd and return the raw zbus result (for callers that
/// need to look at the D-Bus error themselves).
pub async fn call_raw<B>(
    conn: &Connection,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + DynamicType,
{
    conn.call_method(Some(SERVICE), path, Some(iface), method, body)
        .await
}

/// Call a method on obexd and deserialize its reply.
///
/// Errors meaning "that object is gone" (the session closed under us) become
/// [`Error::NotConnected`] so [`ObexClient::with_session`] can reopen the
/// session; everything else goes through [`dbus_util::map_err`].
pub async fn call<B, R>(
    conn: &Connection,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> Result<R>
where
    B: serde::Serialize + DynamicType,
    R: for<'de> serde::Deserialize<'de> + zbus::zvariant::Type,
{
    let context = format!("{iface}.{method}");
    let reply = call_raw(conn, path, iface, method, body)
        .await
        .map_err(|e| map_call_error(&context, e))?;
    reply
        .body()
        .deserialize::<R>()
        .map_err(|e| Error::Backend(format!("{context}: unexpected reply: {e}")))
}

/// Like [`call`] for methods that return nothing.
pub async fn call_void<B>(
    conn: &Connection,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> Result<()>
where
    B: serde::Serialize + DynamicType,
{
    call_raw(conn, path, iface, method, body)
        .await
        .map(|_| ())
        .map_err(|e| map_call_error(&format!("{iface}.{method}"), e))
}

/// See [`call`].
pub fn map_call_error(context: &str, e: zbus::Error) -> Error {
    if is_gone_error(&e) {
        return Error::NotConnected(format!("{context}: OBEX session is gone ({e})"));
    }
    dbus_util::map_err(context, e)
}

/// True for D-Bus errors that mean the target object no longer exists.
pub fn is_gone_error(e: &zbus::Error) -> bool {
    match e {
        zbus::Error::MethodError(name, msg, _) => {
            let name = name.as_str();
            let msg = msg.as_deref().unwrap_or("").to_ascii_lowercase();
            name.ends_with("UnknownObject")
                || name.ends_with("UnknownMethod")
                || name.ends_with("UnknownInterface")
                || msg.contains("transport got disconnected")
                || msg.contains("not connected")
        }
        _ => false,
    }
}

/// Wait until a transfer finishes.
///
/// We subscribe to `PropertiesChanged` *before* reading the current status, so
/// a transfer that completes in between is not missed. obexd deletes the
/// transfer object right after it completes; if the object is already gone
/// when we look, we assume it completed and let the caller check the file.
pub async fn wait_transfer(
    conn: &Connection,
    transfer: &str,
    timeout: Duration,
    target: Target,
) -> Result<()> {
    let rule = MatchRule::builder()
        .msg_type(MsgType::Signal)
        .sender(SERVICE)
        .and_then(|b| b.path(transfer.to_owned()))
        .and_then(|b| b.interface(PROPERTIES_IFACE))
        .and_then(|b| b.member("PropertiesChanged"))
        .map_err(|e| Error::Backend(format!("transfer watch: {e}")))?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, conn, Some(64))
        .await
        .map_err(|e| Error::Backend(format!("transfer watch: {e}")))?;

    let current: zbus::Result<zbus::Message> = call_raw(
        conn,
        transfer,
        PROPERTIES_IFACE,
        "Get",
        &(TRANSFER_IFACE, "Status"),
    )
    .await;
    match current {
        Ok(reply) => {
            if let Ok(value) = reply.body().deserialize::<OwnedValue>() {
                if let Ok(status) = value.downcast_ref::<&str>() {
                    if let Some(done) = finished(status, target) {
                        return done;
                    }
                }
            }
        }
        Err(e) if is_gone_error(&e) => return Ok(()),
        Err(e) => return Err(dbus_util::map_err("Transfer1.Status", e)),
    }

    let wait = async {
        while let Some(msg) = stream.next().await {
            let Ok(msg) = msg else { continue };
            let Ok((iface, changed, _invalidated)) =
                msg.body().deserialize::<(String, PropMap, Vec<String>)>()
            else {
                continue;
            };
            if iface != TRANSFER_IFACE {
                continue;
            }
            if let Some(status) = dbus_util::get_str(&changed, "Status") {
                if let Some(done) = finished(&status, target) {
                    return done;
                }
            }
        }
        Err(Error::Backend(
            "transfer watch ended unexpectedly (obexd restarted?)".into(),
        ))
    };
    tokio::time::timeout(timeout, wait)
        .await
        .unwrap_or_else(|_| {
            Err(Error::Backend(format!(
                "transfer did not finish within {}s",
                timeout.as_secs()
            )))
        })
}

/// `Some(result)` once a transfer status is final.
fn finished(status: &str, target: Target) -> Option<Result<()>> {
    match status {
        "complete" => Some(Ok(())),
        // Transfer1 doesn't say why it failed; on iPhones the usual reason is
        // the permission switch, so say so.
        "error" => Some(Err(Error::Refused(format!(
            "the phone refused or aborted the {} transfer. Hint: {}",
            target.label(),
            target.hint()
        )))),
        _ => None,
    }
}

/// Where a transfer's file is: the `Filename` entry of the properties a
/// `PullAll` / `Get` call returned.
pub fn transfer_filename(props: &PropMap) -> Result<PathBuf> {
    dbus_util::get_str(props, "Filename")
        .filter(|f| !f.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| Error::Backend("obexd did not report where it stored the download".into()))
}

/// Read a downloaded file as text and delete it (we never keep phone data on disk).
/// Invalid UTF-8 is replaced rather than rejected: one odd byte shouldn't lose a phonebook.
pub async fn take_file(path: &Path) -> Result<String> {
    let bytes = tokio::fs::read(path).await;
    let _ = tokio::fs::remove_file(path).await;
    let bytes = bytes.map_err(|e| Error::Backend(format!("reading {}: {e}", path.display())))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// A file only we can read, for data we hand to obexd (e.g. an outgoing
/// bMessage). Prefers `$XDG_RUNTIME_DIR` (per-user, mode 0700) over `/tmp`.
/// Returns a guard that deletes the file when dropped.
pub async fn write_private_file(prefix: &str, ext: &str, contents: &[u8]) -> Result<TempFile> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{prefix}-{}-{nanos}.{ext}", std::process::id()));

    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&path)
        .await
        .map_err(|e| Error::Backend(format!("creating {}: {e}", path.display())))?;
    use tokio::io::AsyncWriteExt;
    let guard = TempFile(path);
    file.write_all(contents)
        .await
        .map_err(|e| Error::Backend(format!("writing {}: {e}", guard.0.display())))?;
    file.flush()
        .await
        .map_err(|e| Error::Backend(format!("writing {}: {e}", guard.0.display())))?;
    Ok(guard)
}

/// Deletes its file on drop.
pub struct TempFile(PathBuf);

impl TempFile {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A `a{sv}` dictionary for method arguments.
pub type Args<'a> = HashMap<&'a str, Value<'a>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn method_error(name: &str, msg: &str) -> zbus::Error {
        let msg_hdr = zbus::Message::method_call("/", "Ping")
            .and_then(|b| b.build(&()))
            .expect("test message");
        zbus::Error::MethodError(
            zbus::names::OwnedErrorName::try_from(name.to_owned()).expect("valid error name"),
            Some(msg.to_owned()),
            msg_hdr,
        )
    }

    fn addr() -> Address {
        Address::parse("AA:BB:CC:DD:EE:FF").expect("valid address")
    }

    #[test]
    fn refused_connection_maps_to_refused_with_hint() {
        let e = session_error(
            &addr(),
            Target::Pbap,
            method_error("org.bluez.obex.Error.Failed", "Connection refused (111)"),
        );
        match e {
            Error::Refused(m) => assert!(m.contains("Sync Contacts"), "{m}"),
            other => panic!("unexpected {other:?}"),
        }
        let e = session_error(
            &addr(),
            Target::Map,
            method_error("org.bluez.obex.Error.Failed", "Forbidden"),
        );
        assert!(matches!(e, Error::Refused(m) if m.contains("Show Notifications")));
    }

    #[test]
    fn missing_obexd_and_out_of_range() {
        let e = session_error(
            &addr(),
            Target::Map,
            method_error("org.freedesktop.DBus.Error.ServiceUnknown", "x"),
        );
        assert!(matches!(e, Error::ServiceUnavailable(m) if m.contains("bluez-obexd")));
        let e = session_error(
            &addr(),
            Target::Map,
            method_error("org.bluez.obex.Error.Failed", "Host is down (112)"),
        );
        assert!(matches!(e, Error::NotConnected(_)));
        let e = session_error(
            &addr(),
            Target::Map,
            method_error("org.bluez.obex.Error.Failed", "weird"),
        );
        assert!(matches!(e, Error::Backend(m) if m.contains("Show Notifications")));
    }

    #[test]
    fn gone_errors_become_not_connected() {
        let e = map_call_error(
            "x",
            method_error("org.freedesktop.DBus.Error.UnknownObject", "no such object"),
        );
        assert!(matches!(e, Error::NotConnected(_)));
        let e = map_call_error(
            "x",
            method_error("org.bluez.obex.Error.InvalidArguments", "bad"),
        );
        assert!(matches!(e, Error::InvalidArgument(_)));
    }

    #[test]
    fn transfer_status_final_states() {
        assert!(finished("queued", Target::Pbap).is_none());
        assert!(finished("active", Target::Pbap).is_none());
        assert!(matches!(finished("complete", Target::Pbap), Some(Ok(()))));
        assert!(matches!(
            finished("error", Target::Map),
            Some(Err(Error::Refused(_)))
        ));
    }

    #[tokio::test]
    async fn private_file_is_written_and_removed() {
        let f = write_private_file("phonesym-test", "txt", b"hello")
            .await
            .expect("write");
        let path = f.path().to_owned();
        assert_eq!(std::fs::read(&path).expect("read"), b"hello");
        drop(f);
        assert!(!path.exists());
    }
}
