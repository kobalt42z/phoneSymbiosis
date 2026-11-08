//! SMS over MAP (Message Access Profile).
//!
//! What MAP gives us through obexd (`org.bluez.obex.MessageAccess1` on a
//! session, see [`crate::obex`]):
//!
//! | Feature      | D-Bus calls                                                                 |
//! |--------------|------------------------------------------------------------------------------|
//! | list         | `SetFolder("/telecom/msg")`, `ListMessages("inbox", {MaxCount, SubjectLength})` |
//! | read one     | `Message1.Get("", false)` on `<session>/message<handle>` -> bMessage file   |
//! | send         | `SetFolder("/telecom/msg")`, `PushMessage(<bMessage file>, "outbox", {Charset})` |
//! | new messages | `ObjectManager.InterfacesAdded` for `Message1` + periodic re-listing        |
//!
//! Message ids are the MAP handles (hex strings such as `20000100001`). obexd
//! only creates a `Message1` object for a handle after it was listed in the
//! current session, so [`MapMessages::get`] re-lists folders when it has to.
//!
//! iPhone: requires **"Show Notifications"** to be enabled for this PC in
//! *Settings > Bluetooth > (i)*; otherwise MAP is refused and we return
//! [`Error::Refused`] with that hint. iMessages are not exposed as SMS; we
//! skip `email` and `im` entries anyway.

mod bmessage;

pub use bmessage::{build_sms, parse as parse_bmessage, BMessage, Party};

use crate::dbus_util::{self, PropMap};
use crate::obex::{self, Args, ObexClient, Target};
use async_trait::async_trait;
use futures_util::StreamExt;
use phonesym_core::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use zbus::message::Type as MsgType;
use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::{Connection, MatchRule, MessageStream};

pub const MESSAGE_ACCESS_IFACE: &str = "org.bluez.obex.MessageAccess1";
pub const MESSAGE_IFACE: &str = "org.bluez.obex.Message1";

/// Absolute MAP folder holding inbox, sent, outbox, ...
const MSG_ROOT: &str = "/telecom/msg";
/// Longest preview MAP allows (`SubjectLength` is a byte, 1..=255). Most SMS fit entirely.
const SUBJECT_LENGTH: u8 = 255;
/// How often `watch` re-lists the inbox in case no notification arrived.
const POLL_INTERVAL: Duration = Duration::from_secs(20);
/// How long `watch` waits before retrying after an error (phone away, ...).
const RETRY_INTERVAL: Duration = Duration::from_secs(60);
/// How many inbox messages `watch` compares on each check.
const WATCH_WINDOW: u16 = 50;
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);
/// Used when the caller passes `limit = 0` (MAP treats MaxCount 0 as "only count").
const DEFAULT_LIMIT: u16 = 1024;

pub struct MapMessages {
    inner: Arc<Inner>,
}

struct Inner {
    events: EventBus,
    obex: ObexClient,
    /// Phones with a running watch task, so `watch` is idempotent.
    watched: Mutex<HashSet<Address>>,
}

impl MapMessages {
    pub async fn new(events: EventBus) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                events,
                obex: ObexClient::new(),
                watched: Mutex::new(HashSet::new()),
            }),
        })
    }
}

/// One entry of a `ListMessages` reply.
struct Listed {
    path: OwnedObjectPath,
    props: PropMap,
}

impl Inner {
    /// `ListMessages` on one folder, newest first, SMS/MMS only.
    async fn list(
        &self,
        phone: &Address,
        folder: MessageFolder,
        limit: u16,
    ) -> Result<Vec<Listed>> {
        let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };
        self.obex
            .with_session(phone, Target::Map, |conn, session| async move {
                list_in_session(&conn, session.as_str(), folder, limit).await
            })
            .await
    }

    /// Compare the inbox with what we've seen and emit events for new unread messages.
    async fn check_inbox(
        &self,
        phone: &Address,
        seen: &mut HashSet<String>,
        emit: bool,
    ) -> Result<()> {
        let listed = self.list(phone, MessageFolder::Inbox, WATCH_WINDOW).await?;
        for item in listed {
            let message = to_message(&item, MessageFolder::Inbox);
            if seen.insert(message.id.clone()) && emit && !message.read {
                self.events.emit(PhoneEvent::MessageReceived { message });
            }
        }
        Ok(())
    }
}

async fn list_in_session(
    conn: &Connection,
    session: &str,
    folder: MessageFolder,
    limit: u16,
) -> Result<Vec<Listed>> {
    obex::call_void(
        conn,
        session,
        MESSAGE_ACCESS_IFACE,
        "SetFolder",
        &(MSG_ROOT,),
    )
    .await?;
    let mut filter: Args = Args::new();
    filter.insert("MaxCount", Value::from(limit));
    filter.insert("SubjectLength", Value::from(SUBJECT_LENGTH));
    let reply: HashMap<OwnedObjectPath, PropMap> = obex::call(
        conn,
        session,
        MESSAGE_ACCESS_IFACE,
        "ListMessages",
        &(folder.map_name(), filter),
    )
    .await?;

    let mut items: Vec<Listed> = reply
        .into_iter()
        .filter(|(_, props)| is_text_message(props))
        .map(|(path, props)| Listed { path, props })
        .collect();
    // The reply is a dictionary, so the phone's order is lost; MAP timestamps
    // (YYYYMMDDTHHMMSS) sort correctly as strings.
    items.sort_by(|a, b| {
        let ta = dbus_util::get_str(&a.props, "Timestamp").unwrap_or_default();
        let tb = dbus_util::get_str(&b.props, "Timestamp").unwrap_or_default();
        tb.cmp(&ta)
    });
    Ok(items)
}

/// SMS and MMS, not e-mail or instant messages (`Type` is e.g. `sms-gsm`).
fn is_text_message(props: &PropMap) -> bool {
    match dbus_util::get_str(props, "Type") {
        Some(t) => {
            let t = t.to_ascii_lowercase();
            t.starts_with("sms") || t == "mms"
        }
        None => true,
    }
}

/// The MAP handle from an object path like `/org/bluez/obex/client/session3/message20000100001`.
pub fn handle_from_path(path: &str) -> String {
    let last = path.rsplit('/').next().unwrap_or(path);
    last.strip_prefix("message").unwrap_or(last).to_owned()
}

/// Handles end up in an object path, so only allow characters valid there.
fn valid_handle(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `Folder` property (`/telecom/msg/inbox`, `telecom/msg/SENT`, ...) to our folder type.
pub fn folder_from_map(path: &str) -> Option<MessageFolder> {
    let last = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()?
        .to_ascii_lowercase();
    Some(match last.as_str() {
        "inbox" => MessageFolder::Inbox,
        "sent" => MessageFolder::Sent,
        "outbox" => MessageFolder::Outbox,
        "draft" | "drafts" => MessageFolder::Draft,
        "deleted" => MessageFolder::Deleted,
        _ => return None,
    })
}

/// Build a [`Message`] from listing properties. For received messages the
/// peer is the sender, for our own messages it's the recipient.
fn to_message(item: &Listed, fallback_folder: MessageFolder) -> Message {
    message_from_props(
        handle_from_path(item.path.as_str()),
        &item.props,
        fallback_folder,
    )
}

fn message_from_props(id: String, props: &PropMap, fallback_folder: MessageFolder) -> Message {
    let get = |k: &str| {
        dbus_util::get_str(props, k)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let folder = get("Folder")
        .and_then(|f| folder_from_map(&f))
        .unwrap_or(fallback_folder);
    let outgoing = matches!(
        folder,
        MessageFolder::Sent | MessageFolder::Outbox | MessageFolder::Draft
    );
    let (number_key, name_key, other_number) = if outgoing {
        ("RecipientAddress", "Recipient", "SenderAddress")
    } else {
        ("SenderAddress", "Sender", "RecipientAddress")
    };
    let peer_number = get(number_key)
        .or_else(|| get(other_number))
        .unwrap_or_default();
    let peer_name = get(name_key).filter(|n| *n != peer_number);
    Message {
        id,
        folder,
        peer_number,
        peer_name,
        text: dbus_util::get_str(props, "Subject").unwrap_or_default(),
        timestamp: get("Timestamp"),
        read: dbus_util::get_bool(props, "Read").unwrap_or(outgoing),
    }
}

/// Keep only characters a phone accepts in a number; reject anything else.
pub fn normalise_number(raw: &str) -> Result<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.'))
        .collect();
    let valid = !cleaned.is_empty()
        && cleaned
            .chars()
            .enumerate()
            .all(|(i, c)| c.is_ascii_digit() || c == '*' || c == '#' || (c == '+' && i == 0))
        && cleaned.chars().any(|c| c.is_ascii_digit());
    if valid {
        Ok(cleaned)
    } else {
        Err(Error::InvalidArgument(format!(
            "not a phone number: {raw:?}"
        )))
    }
}

/// Wait for one `InterfacesAdded` that adds a `Message1` object (the MNS
/// "new message" notification) or for the poll interval, whichever is first.
async fn wait_for_trigger(stream: Option<&mut MessageStream>) {
    let Some(stream) = stream else {
        tokio::time::sleep(POLL_INTERVAL).await;
        return;
    };
    let notified = async {
        while let Some(Ok(msg)) = stream.next().await {
            if let Ok((_path, ifaces)) = msg
                .body()
                .deserialize::<(OwnedObjectPath, HashMap<String, PropMap>)>()
            {
                if ifaces.contains_key(MESSAGE_IFACE) {
                    return;
                }
            }
        }
        // Stream ended: fall back to plain polling for this round.
        std::future::pending::<()>().await
    };
    tokio::select! {
        _ = notified => {
            // The phone often sends several events per SMS; let them settle.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        _ = tokio::time::sleep(POLL_INTERVAL) => {}
    }
}

async fn interfaces_added_stream(conn: &Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(MsgType::Signal)
        .sender(obex::SERVICE)
        .and_then(|b| b.interface(obex::OBJECT_MANAGER_IFACE))
        .and_then(|b| b.member("InterfacesAdded"))
        .ok()?
        .build();
    MessageStream::for_match_rule(rule, conn, Some(64))
        .await
        .ok()
}

/// Background loop behind [`MessagesProvider::watch`].
///
/// New-message notifications: with a MAP session open, obexd registers for
/// MAP notifications (MNS) itself and, on a "new message" event, adds a
/// `Message1` object, which shows up as `InterfacesAdded`. Not every
/// phone / BlueZ build delivers these, so we also re-list the inbox every
/// [`POLL_INTERVAL`] and compare handles. Either way the event carries the
/// listing's preview text, and reading it does not mark it read on the phone.
async fn watch_loop(inner: Arc<Inner>, phone: Address, mut seen: HashSet<String>) {
    let mut stream = match inner.obex.connection().await {
        Ok(conn) => interfaces_added_stream(&conn).await,
        Err(_) => None,
    };
    let mut failing = false;
    loop {
        wait_for_trigger(stream.as_mut()).await;
        match inner.check_inbox(&phone, &mut seen, true).await {
            Ok(()) => {
                if failing {
                    tracing::info!(%phone, "SMS watch recovered");
                }
                failing = false;
            }
            Err(e) => {
                // Warn once per outage, not every minute.
                if !failing {
                    inner.events.emit(PhoneEvent::Warning {
                        message: format!("SMS watch for {phone}: {e}"),
                    });
                }
                failing = true;
                tokio::time::sleep(RETRY_INTERVAL).await;
            }
        }
    }
}

#[async_trait]
impl MessagesProvider for MapMessages {
    async fn list(
        &self,
        phone: &Address,
        folder: MessageFolder,
        limit: u16,
    ) -> Result<Vec<Message>> {
        let listed = self.inner.list(phone, folder, limit).await?;
        let mut messages: Vec<Message> =
            listed.iter().map(|item| to_message(item, folder)).collect();
        if limit > 0 {
            messages.truncate(limit as usize);
        }
        Ok(messages)
    }

    async fn get(&self, phone: &Address, id: &str) -> Result<Message> {
        let id = id.trim();
        if !valid_handle(id) {
            return Err(Error::InvalidArgument(format!("not a message id: {id:?}")));
        }
        self.inner
            .obex
            .with_session(phone, Target::Map, |conn, session| async move {
                let path = format!("{}/message{id}", session.as_str());

                // obexd only knows handles it has listed in this session. Ask
                // for the object's properties; if it doesn't exist yet, list
                // the folders until it shows up.
                let props: PropMap =
                    match obex::call_raw(&conn, &path, obex::PROPERTIES_IFACE, "GetAll", &(MESSAGE_IFACE,)).await {
                        Ok(reply) => reply
                            .body()
                            .deserialize::<PropMap>()
                            .map_err(|e| Error::Backend(format!("Message1 properties: {e}")))?,
                        Err(e) if obex::is_gone_error(&e) => {
                            let mut found = None;
                            for folder in [
                                MessageFolder::Inbox,
                                MessageFolder::Sent,
                                MessageFolder::Outbox,
                                MessageFolder::Draft,
                                MessageFolder::Deleted,
                            ] {
                                // A phone may not have every folder; skip those errors.
                                let listed = match list_in_session(&conn, session.as_str(), folder, DEFAULT_LIMIT).await {
                                    Ok(l) => l,
                                    Err(Error::NotConnected(m)) => return Err(Error::NotConnected(m)),
                                    Err(_) => continue,
                                };
                                if let Some(item) = listed.into_iter().find(|l| l.path.as_str() == path) {
                                    let mut props = item.props;
                                    if !props.contains_key("Folder") {
                                        if let Ok(v) = Value::from(format!("{MSG_ROOT}/{}", folder.map_name())).try_into() {
                                            props.insert("Folder".into(), v);
                                        }
                                    }
                                    found = Some(props);
                                    break;
                                }
                            }
                            found.ok_or_else(|| Error::InvalidArgument(format!("no message with id {id} on the phone")))?
                        }
                        Err(e) => return Err(obex::map_call_error("Message1.GetAll", e)),
                    };

                let (transfer, tprops): (OwnedObjectPath, PropMap) =
                    obex::call(&conn, &path, MESSAGE_IFACE, "Get", &("", false)).await?;
                let file = obex::transfer_filename(&tprops);
                let waited = obex::wait_transfer(&conn, transfer.as_str(), TRANSFER_TIMEOUT, Target::Map).await;
                let file = file?;
                if let Err(e) = waited {
                    let _ = tokio::fs::remove_file(&file).await;
                    return Err(e);
                }
                let raw = obex::take_file(&file).await?;

                let mut message = message_from_props(id.to_owned(), &props, MessageFolder::Inbox);
                if let Some(b) = bmessage::parse(&raw) {
                    // Without a Folder property, the bMessage FOLDER line tells us.
                    if !props.contains_key("Folder") {
                        if let Some(f) = b.folder.as_deref().and_then(folder_from_map) {
                            message.folder = f;
                        }
                    }
                    let peer = if matches!(message.folder, MessageFolder::Inbox | MessageFolder::Deleted) {
                        b.originator.clone()
                    } else {
                        b.recipients.first().cloned()
                    };
                    if let Some(peer) = peer {
                        if message.peer_number.is_empty() {
                            message.peer_number = peer.address.unwrap_or_default();
                        }
                        if message.peer_name.is_none() {
                            message.peer_name = peer.name;
                        }
                    }
                    if b.charset.as_deref().is_some_and(|c| c.eq_ignore_ascii_case("native")) {
                        tracing::warn!("message {id} came in the phone's native PDU encoding; showing the preview");
                    } else {
                        message.text = b.text;
                    }
                }
                Ok(message)
            })
            .await
    }

    async fn send(&self, phone: &Address, to: &str, text: &str) -> Result<()> {
        let number = normalise_number(to)?;
        if text.trim().is_empty() {
            return Err(Error::InvalidArgument("message text is empty".into()));
        }
        if !bmessage::text_is_sendable(text) {
            return Err(Error::InvalidArgument(
                "message text may not contain a line reading END:MSG".into(),
            ));
        }
        let file = obex::write_private_file(
            "phonesym-sms",
            "bmsg",
            bmessage::build_sms(&number, text).as_bytes(),
        )
        .await?;
        let source = file.path().to_string_lossy().into_owned();
        let source = source.as_str();

        self.inner
            .obex
            .with_session(phone, Target::Map, |conn, session| async move {
                let session = session.as_str();
                obex::call_void(
                    &conn,
                    session,
                    MESSAGE_ACCESS_IFACE,
                    "SetFolder",
                    &(MSG_ROOT,),
                )
                .await?;
                let mut args: Args = Args::new();
                args.insert("Charset", Value::from("utf8"));
                let (transfer, _props): (OwnedObjectPath, PropMap) = match obex::call(
                    &conn,
                    session,
                    MESSAGE_ACCESS_IFACE,
                    "PushMessage",
                    &(source, "outbox", args),
                )
                .await
                {
                    Err(Error::NotSupported(m)) => {
                        return Err(Error::NotSupported(format!(
                            "{m}: this phone does not accept messages to send over Bluetooth"
                        )))
                    }
                    other => other?,
                };
                obex::wait_transfer(&conn, transfer.as_str(), TRANSFER_TIMEOUT, Target::Map).await
            })
            .await
        // `file` is dropped (deleted) here, after the transfer has finished.
    }

    async fn watch(&self, phone: &Address) -> Result<()> {
        if self.inner.watched.lock().await.contains(phone) {
            return Ok(());
        }
        // Take a first snapshot now, so permission problems reach the caller
        // and messages already on the phone are not announced as new.
        let mut seen = HashSet::new();
        self.inner.check_inbox(phone, &mut seen, false).await?;
        if !self.inner.watched.lock().await.insert(phone.clone()) {
            return Ok(()); // another caller started it meanwhile
        }
        tokio::spawn(watch_loop(self.inner.clone(), phone.clone(), seen));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::OwnedValue;

    fn props(pairs: &[(&str, Value<'static>)]) -> PropMap {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    OwnedValue::try_from(v.clone()).expect("owned value"),
                )
            })
            .collect()
    }

    #[test]
    fn handles_and_folders() {
        assert_eq!(
            handle_from_path("/org/bluez/obex/client/session3/message20000100001"),
            "20000100001"
        );
        assert!(valid_handle("20000100001"));
        assert!(!valid_handle("../x"));
        assert!(!valid_handle(""));
        assert_eq!(
            folder_from_map("/telecom/msg/inbox"),
            Some(MessageFolder::Inbox)
        );
        assert_eq!(
            folder_from_map("TELECOM/MSG/SENT"),
            Some(MessageFolder::Sent)
        );
        assert_eq!(
            folder_from_map("telecom/msg/outbox/"),
            Some(MessageFolder::Outbox)
        );
        assert_eq!(folder_from_map("telecom/msg"), None);
    }

    #[test]
    fn incoming_message_uses_sender() {
        let p = props(&[
            ("Subject", Value::from("Hello there")),
            ("Timestamp", Value::from("20261108T110455")),
            ("Sender", Value::from("Anna")),
            ("SenderAddress", Value::from("+15550100")),
            ("RecipientAddress", Value::from("+15559999")),
            ("Type", Value::from("sms-gsm")),
            ("Read", Value::from(false)),
        ]);
        let m = message_from_props("1".into(), &p, MessageFolder::Inbox);
        assert_eq!(m.peer_number, "+15550100");
        assert_eq!(m.peer_name.as_deref(), Some("Anna"));
        assert_eq!(m.text, "Hello there");
        assert_eq!(m.timestamp.as_deref(), Some("20261108T110455"));
        assert!(!m.read);
        assert_eq!(m.folder, MessageFolder::Inbox);
    }

    #[test]
    fn sent_message_uses_recipient_and_folder_property() {
        let p = props(&[
            ("Folder", Value::from("/telecom/msg/sent")),
            ("SenderAddress", Value::from("+15559999")),
            ("Recipient", Value::from("+15550100")),
            ("RecipientAddress", Value::from("+15550100")),
        ]);
        let m = message_from_props("2".into(), &p, MessageFolder::Inbox);
        assert_eq!(m.folder, MessageFolder::Sent);
        assert_eq!(m.peer_number, "+15550100");
        assert_eq!(m.peer_name, None, "a name equal to the number is dropped");
        assert!(m.read, "own messages default to read");
    }

    #[test]
    fn type_filter() {
        assert!(is_text_message(&props(&[("Type", Value::from("sms-gsm"))])));
        assert!(is_text_message(&props(&[(
            "Type",
            Value::from("SMS_CDMA")
        )])));
        assert!(is_text_message(&props(&[("Type", Value::from("mms"))])));
        assert!(!is_text_message(&props(&[("Type", Value::from("email"))])));
        assert!(!is_text_message(&props(&[("Type", Value::from("im"))])));
        assert!(is_text_message(&props(&[])));
    }

    #[test]
    fn number_validation() {
        assert_eq!(
            normalise_number("+1 (555) 010-0").ok().as_deref(),
            Some("+15550100")
        );
        assert_eq!(normalise_number("*100#").ok().as_deref(), Some("*100#"));
        assert!(normalise_number("").is_err());
        assert!(normalise_number("abc").is_err());
        assert!(normalise_number("1+2").is_err());
        assert!(normalise_number("+").is_err());
    }
}
