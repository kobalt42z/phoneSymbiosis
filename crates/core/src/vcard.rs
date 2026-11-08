//! Minimal vCard (2.1 / 3.0) parser for phonebook downloads.
//! TODO(contacts): implement parsing of contacts and call-history entries.

use crate::model::{CallHistoryEntry, CallHistoryKind, Contact};

/// Parse a phonebook file with one or more vCards.
pub fn parse_contacts(_data: &str) -> Vec<Contact> {
    Vec::new()
}

/// Parse a call-history phonebook (`ich`, `och`, `mch`, `cch`).
pub fn parse_call_history(_data: &str, _default_kind: Option<CallHistoryKind>) -> Vec<CallHistoryEntry> {
    Vec::new()
}
