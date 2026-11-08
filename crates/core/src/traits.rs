//! The interfaces every platform backend implements.
//!
//! Each trait maps to one area of phone features, so a backend can be built
//! piece by piece, and a piece can return [`Error::NotSupported`](crate::Error::NotSupported)
//! until it is ready.

use crate::model::*;
use crate::Result;
use async_trait::async_trait;
use std::time::Duration;

/// Finding, pairing and connecting phones.
#[async_trait]
pub trait BluetoothManager: Send + Sync {
    async fn adapter(&self) -> Result<AdapterInfo>;
    async fn set_powered(&self, on: bool) -> Result<()>;
    /// Look for nearby devices for `duration`, then return everything known.
    async fn scan(&self, duration: Duration) -> Result<Vec<Device>>;
    /// Devices already known to the system (paired or recently seen).
    async fn devices(&self) -> Result<Vec<Device>>;
    async fn device(&self, address: &Address) -> Result<Device>;
    /// Pair and mark as trusted, so the phone can reconnect on its own later.
    async fn pair(&self, address: &Address) -> Result<()>;
    /// Answer a [`PhoneEvent::PairingConfirm`](crate::PhoneEvent::PairingConfirm) prompt.
    async fn confirm_pairing(&self, address: &Address, accept: bool) -> Result<()>;
    async fn connect(&self, address: &Address) -> Result<()>;
    async fn disconnect(&self, address: &Address) -> Result<()>;
    /// Remove the pairing.
    async fn forget(&self, address: &Address) -> Result<()>;
}

/// Hands-Free Profile call control.
#[async_trait]
pub trait CallControl: Send + Sync {
    /// Phones whose hands-free link is up right now.
    async fn available_phones(&self) -> Result<Vec<Address>>;
    async fn calls(&self) -> Result<Vec<Call>>;
    /// Start a call. `phone = None` uses the first available phone.
    async fn dial(&self, phone: Option<&Address>, number: &str) -> Result<()>;
    async fn answer(&self, call: &CallId) -> Result<()>;
    async fn hang_up(&self, call: &CallId) -> Result<()>;
    async fn hang_up_all(&self, phone: Option<&Address>) -> Result<()>;
    /// Send DTMF digits during a call (for menus: "press 1 for...").
    async fn send_tones(&self, phone: Option<&Address>, tones: &str) -> Result<()>;
    /// Put the active call on hold and switch to the held one.
    async fn swap_calls(&self, phone: Option<&Address>) -> Result<()>;
    async fn hold_and_answer(&self, phone: Option<&Address>) -> Result<()>;
    async fn release_and_answer(&self, phone: Option<&Address>) -> Result<()>;
    async fn merge_calls(&self, phone: Option<&Address>) -> Result<()>;
    async fn status(&self, phone: Option<&Address>) -> Result<PhoneStatus>;
    async fn volume(&self, phone: Option<&Address>) -> Result<CallVolume>;
    async fn set_volume(&self, phone: Option<&Address>, volume: CallVolume) -> Result<()>;
}

/// Moving call audio between the phone and the PC speakers / microphone.
#[async_trait]
pub trait AudioRouter: Send + Sync {
    /// Start sending phone audio to the PC speakers and PC mic to the phone.
    async fn start(&self, phone: &Address) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn is_active(&self) -> bool;
    /// Mute / unmute the PC microphone towards the phone.
    async fn set_mic_muted(&self, muted: bool) -> Result<()>;
}

/// Phonebook Access Profile: contacts and call history (read-only on iPhone).
#[async_trait]
pub trait ContactsProvider: Send + Sync {
    async fn contacts(&self, phone: &Address) -> Result<Vec<Contact>>;
    async fn call_history(&self, phone: &Address, kind: Option<CallHistoryKind>) -> Result<Vec<CallHistoryEntry>>;
}

/// Message Access Profile: SMS.
#[async_trait]
pub trait MessagesProvider: Send + Sync {
    async fn list(&self, phone: &Address, folder: MessageFolder, limit: u16) -> Result<Vec<Message>>;
    /// Full text of one message.
    async fn get(&self, phone: &Address, id: &str) -> Result<Message>;
    async fn send(&self, phone: &Address, to: &str, text: &str) -> Result<()>;
    /// Start pushing [`PhoneEvent::MessageReceived`](crate::PhoneEvent::MessageReceived) events.
    async fn watch(&self, phone: &Address) -> Result<()>;
}
