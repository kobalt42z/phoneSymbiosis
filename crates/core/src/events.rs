//! Live events from the phone. Backends publish, front ends subscribe.

use crate::model::*;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Everything that can happen without the user asking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PhoneEvent {
    DeviceFound { device: Device },
    DeviceConnected { address: Address },
    DeviceDisconnected { address: Address },
    /// A phone's hands-free link is ready (calls can be made).
    CallsAvailable { address: Address },
    CallsUnavailable { address: Address },
    CallAdded { call: Call },
    CallChanged { call: Call },
    CallRemoved { id: CallId },
    StatusChanged { status: PhoneStatus },
    VolumeChanged { device: Option<Address>, volume: CallVolume },
    /// Call audio started or stopped flowing between PC and phone.
    AudioRouted { active: bool },
    MessageReceived { message: Message },
    /// Pairing needs the user to confirm this code matches the phone.
    PairingConfirm { address: Address, passkey: u32 },
    /// A non-fatal problem worth telling the user about.
    Warning { message: String },
}

/// Cheap, cloneable fan-out channel for [`PhoneEvent`]s.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<PhoneEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    /// Publish an event. It is fine if nobody is listening.
    pub fn emit(&self, event: PhoneEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PhoneEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
