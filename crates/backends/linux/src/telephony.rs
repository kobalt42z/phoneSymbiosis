//! Hands-free call control through PipeWire's telephony D-Bus service. TODO(calls-agent): implement.

use async_trait::async_trait;
use phonesym_core::*;

pub struct Telephony {
    #[allow(dead_code)]
    events: EventBus,
}

impl Telephony {
    pub async fn new(_config: &PlatformConfig, events: EventBus) -> Result<Self> {
        Ok(Self { events })
    }
}

fn todo() -> Error {
    Error::NotSupported("telephony: not implemented yet".into())
}

#[async_trait]
impl CallControl for Telephony {
    async fn available_phones(&self) -> Result<Vec<Address>> { Err(todo()) }
    async fn calls(&self) -> Result<Vec<Call>> { Err(todo()) }
    async fn dial(&self, _p: Option<&Address>, _n: &str) -> Result<()> { Err(todo()) }
    async fn answer(&self, _c: &CallId) -> Result<()> { Err(todo()) }
    async fn hang_up(&self, _c: &CallId) -> Result<()> { Err(todo()) }
    async fn hang_up_all(&self, _p: Option<&Address>) -> Result<()> { Err(todo()) }
    async fn send_tones(&self, _p: Option<&Address>, _t: &str) -> Result<()> { Err(todo()) }
    async fn swap_calls(&self, _p: Option<&Address>) -> Result<()> { Err(todo()) }
    async fn hold_and_answer(&self, _p: Option<&Address>) -> Result<()> { Err(todo()) }
    async fn release_and_answer(&self, _p: Option<&Address>) -> Result<()> { Err(todo()) }
    async fn merge_calls(&self, _p: Option<&Address>) -> Result<()> { Err(todo()) }
    async fn status(&self, _p: Option<&Address>) -> Result<PhoneStatus> { Err(todo()) }
    async fn volume(&self, _p: Option<&Address>) -> Result<CallVolume> { Err(todo()) }
    async fn set_volume(&self, _p: Option<&Address>, _v: CallVolume) -> Result<()> { Err(todo()) }
}
