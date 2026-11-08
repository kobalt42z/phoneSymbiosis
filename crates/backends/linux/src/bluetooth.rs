//! BlueZ: adapter, scanning, pairing, connecting. TODO(calls-agent): implement.

use async_trait::async_trait;
use phonesym_core::*;
use std::time::Duration;

pub struct BluezManager {
    #[allow(dead_code)]
    events: EventBus,
}

impl BluezManager {
    pub async fn new(_config: &PlatformConfig, events: EventBus) -> Result<Self> {
        Ok(Self { events })
    }
}

fn todo() -> Error {
    Error::NotSupported("bluetooth: not implemented yet".into())
}

#[async_trait]
impl BluetoothManager for BluezManager {
    async fn adapter(&self) -> Result<AdapterInfo> { Err(todo()) }
    async fn set_powered(&self, _on: bool) -> Result<()> { Err(todo()) }
    async fn scan(&self, _d: Duration) -> Result<Vec<Device>> { Err(todo()) }
    async fn devices(&self) -> Result<Vec<Device>> { Err(todo()) }
    async fn device(&self, _a: &Address) -> Result<Device> { Err(todo()) }
    async fn pair(&self, _a: &Address) -> Result<()> { Err(todo()) }
    async fn confirm_pairing(&self, _a: &Address, _accept: bool) -> Result<()> { Err(todo()) }
    async fn connect(&self, _a: &Address) -> Result<()> { Err(todo()) }
    async fn disconnect(&self, _a: &Address) -> Result<()> { Err(todo()) }
    async fn forget(&self, _a: &Address) -> Result<()> { Err(todo()) }
}
