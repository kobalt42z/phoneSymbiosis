use crate::events::EventBus;
use crate::traits::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Everything a front end needs, bundled by a platform backend.
#[derive(Clone)]
pub struct Platform {
    /// Name of the backend, e.g. "linux-bluez".
    pub name: &'static str,
    pub bluetooth: Arc<dyn BluetoothManager>,
    pub calls: Arc<dyn CallControl>,
    pub audio: Arc<dyn AudioRouter>,
    pub contacts: Arc<dyn ContactsProvider>,
    pub messages: Arc<dyn MessagesProvider>,
    pub events: EventBus,
}

/// User settings a backend may honour.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// Use this Bluetooth adapter (e.g. "hci0") instead of the first one.
    pub adapter: Option<String>,
    /// Route call audio automatically when a call becomes active.
    pub auto_audio: bool,
}
