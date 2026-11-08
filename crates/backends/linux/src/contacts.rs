//! Contacts and call history over PBAP (Phone Book Access Profile).
//!
//! Read-only by design: PBAP has no way to change the phone's address book.
//! Every call downloads fresh data from the phone; nothing is cached on disk.
//!
//! Flow for one download (see [`crate::obex`] for sessions and transfers):
//! 1. `PhonebookAccess1.Select("int", <phonebook>)` picks the internal-memory
//!    phonebook: `pb` (contacts), `ich` / `och` / `mch` (incoming / outgoing /
//!    missed calls) or `cch` (all calls combined).
//! 2. `PhonebookAccess1.PullAll("", {"Format": "vcard30"})` starts a transfer
//!    into a temp file chosen by obexd.
//! 3. Wait for the transfer, read and delete the file, parse it with
//!    [`phonesym_core::vcard`].
//!
//! iPhone: requires **"Sync Contacts"** to be enabled for this PC in
//! *Settings > Bluetooth > (i)*; otherwise the phone refuses and we return
//! [`Error::Refused`] with that hint.

use crate::dbus_util::PropMap;
use crate::obex::{self, Args, ObexClient, Target};
use async_trait::async_trait;
use phonesym_core::vcard;
use phonesym_core::*;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, Value};

pub const PHONEBOOK_IFACE: &str = "org.bluez.obex.PhonebookAccess1";

/// Large address books with photos can take a while over Bluetooth.
const PULL_TIMEOUT: Duration = Duration::from_secs(180);

pub struct PbapContacts {
    #[allow(dead_code)]
    events: EventBus,
    obex: ObexClient,
}

impl PbapContacts {
    pub async fn new(events: EventBus) -> Result<Self> {
        Ok(Self {
            events,
            obex: ObexClient::new(),
        })
    }

    /// Download one phonebook (`pb`, `ich`, ...) as vCard text.
    async fn pull(&self, phone: &Address, phonebook: &'static str) -> Result<String> {
        self.obex
            .with_session(phone, Target::Pbap, |conn, session| async move {
                let session = session.as_str();
                obex::call_void(
                    &conn,
                    session,
                    PHONEBOOK_IFACE,
                    "Select",
                    &("int", phonebook),
                )
                .await?;

                // vCard 3.0 is UTF-8 and mostly free of quoted-printable; the
                // parser handles 2.1 too if a phone ignores the request.
                let mut filters: Args = Args::new();
                filters.insert("Format", Value::from("vcard30"));
                let (transfer, props): (OwnedObjectPath, PropMap) =
                    obex::call(&conn, session, PHONEBOOK_IFACE, "PullAll", &("", filters)).await?;

                let file = obex::transfer_filename(&props);
                let waited =
                    obex::wait_transfer(&conn, transfer.as_str(), PULL_TIMEOUT, Target::Pbap).await;
                let file = file?;
                if let Err(e) = waited {
                    let _ = tokio::fs::remove_file(&file).await;
                    return Err(e);
                }
                obex::take_file(&file).await
            })
            .await
    }
}

/// PBAP phonebook name for a call-history kind (`None` = all calls).
pub fn history_phonebook(kind: Option<CallHistoryKind>) -> &'static str {
    match kind {
        Some(CallHistoryKind::Incoming) => "ich",
        Some(CallHistoryKind::Outgoing) => "och",
        Some(CallHistoryKind::Missed) => "mch",
        None => "cch",
    }
}

#[async_trait]
impl ContactsProvider for PbapContacts {
    async fn contacts(&self, phone: &Address) -> Result<Vec<Contact>> {
        let data = self.pull(phone, "pb").await?;
        Ok(vcard::parse_contacts(&data))
    }

    async fn call_history(
        &self,
        phone: &Address,
        kind: Option<CallHistoryKind>,
    ) -> Result<Vec<CallHistoryEntry>> {
        let data = self.pull(phone, history_phonebook(kind)).await?;
        // For a single-kind folder every entry has that kind even if the phone
        // leaves out the IrMC parameter; for `cch` the parameter decides.
        let mut entries = vcard::parse_call_history(&data, kind);
        if let Some(kind) = kind {
            // Some phones tag entries in `mch` as RECEIVED; trust the folder.
            for e in &mut entries {
                e.kind = kind;
            }
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonebook_names() {
        assert_eq!(history_phonebook(Some(CallHistoryKind::Incoming)), "ich");
        assert_eq!(history_phonebook(Some(CallHistoryKind::Outgoing)), "och");
        assert_eq!(history_phonebook(Some(CallHistoryKind::Missed)), "mch");
        assert_eq!(history_phonebook(None), "cch");
    }
}
