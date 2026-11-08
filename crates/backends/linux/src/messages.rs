//! SMS over MAP. TODO(pim-agent): implement.

use async_trait::async_trait;
use phonesym_core::*;

pub struct MapMessages {
    #[allow(dead_code)]
    events: EventBus,
}

impl MapMessages {
    pub async fn new(events: EventBus) -> Result<Self> {
        Ok(Self { events })
    }
}

fn todo() -> Error {
    Error::NotSupported("messages: not implemented yet".into())
}

#[async_trait]
impl MessagesProvider for MapMessages {
    async fn list(&self, _p: &Address, _f: MessageFolder, _l: u16) -> Result<Vec<Message>> { Err(todo()) }
    async fn get(&self, _p: &Address, _id: &str) -> Result<Message> { Err(todo()) }
    async fn send(&self, _p: &Address, _to: &str, _t: &str) -> Result<()> { Err(todo()) }
    async fn watch(&self, _p: &Address) -> Result<()> { Err(todo()) }
}
