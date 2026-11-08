//! Call audio routing through PipeWire. TODO(calls-agent): implement.

use async_trait::async_trait;
use phonesym_core::*;
use std::sync::Arc;

pub struct PipewireAudio {
    #[allow(dead_code)]
    events: EventBus,
}

impl PipewireAudio {
    pub fn new(_config: &PlatformConfig, events: EventBus) -> Self {
        Self { events }
    }
}

/// Start/stop audio routing automatically as calls become active / end.
pub fn spawn_auto_routing(_audio: Arc<PipewireAudio>, _events: EventBus) {}

fn todo() -> Error {
    Error::NotSupported("audio: not implemented yet".into())
}

#[async_trait]
impl AudioRouter for PipewireAudio {
    async fn start(&self, _p: &Address) -> Result<()> { Err(todo()) }
    async fn stop(&self) -> Result<()> { Err(todo()) }
    async fn is_active(&self) -> bool { false }
    async fn set_mic_muted(&self, _m: bool) -> Result<()> { Err(todo()) }
}
