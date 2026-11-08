//! Contacts and call history over PBAP. TODO(pim-agent): implement.

use async_trait::async_trait;
use phonesym_core::*;

pub struct PbapContacts {
    #[allow(dead_code)]
    events: EventBus,
}

impl PbapContacts {
    pub async fn new(events: EventBus) -> Result<Self> {
        Ok(Self { events })
    }
}

fn todo() -> Error {
    Error::NotSupported("contacts: not implemented yet".into())
}

#[async_trait]
impl ContactsProvider for PbapContacts {
    async fn contacts(&self, _p: &Address) -> Result<Vec<Contact>> { Err(todo()) }
    async fn call_history(&self, _p: &Address, _k: Option<CallHistoryKind>) -> Result<Vec<CallHistoryEntry>> { Err(todo()) }
}
