//! Calls, audio, contacts and SMS: not available on Windows yet.

use async_trait::async_trait;
use phonesym_core::*;

pub struct Unsupported;

const CALLS: &str = "calls on Windows. Windows does not let apps act as a Bluetooth hands-free kit, so calls stay on the phone";
const AUDIO: &str =
    "call audio on Windows. The Bluetooth call audio link is owned by the Windows driver";
const CONTACTS: &str = "contacts and call history on Windows yet";
const MESSAGES: &str = "SMS on Windows yet";

fn no<T>(what: &str) -> Result<T> {
    Err(Error::NotSupported(what.into()))
}

#[async_trait]
impl CallControl for Unsupported {
    async fn available_phones(&self) -> Result<Vec<Address>> {
        Ok(Vec::new())
    }
    async fn calls(&self) -> Result<Vec<Call>> {
        Ok(Vec::new())
    }
    async fn dial(&self, _: Option<&Address>, _: &str) -> Result<()> {
        no(CALLS)
    }
    async fn answer(&self, _: &CallId) -> Result<()> {
        no(CALLS)
    }
    async fn hang_up(&self, _: &CallId) -> Result<()> {
        no(CALLS)
    }
    async fn hang_up_all(&self, _: Option<&Address>) -> Result<()> {
        no(CALLS)
    }
    async fn send_tones(&self, _: Option<&Address>, _: &str) -> Result<()> {
        no(CALLS)
    }
    async fn swap_calls(&self, _: Option<&Address>) -> Result<()> {
        no(CALLS)
    }
    async fn hold_and_answer(&self, _: Option<&Address>) -> Result<()> {
        no(CALLS)
    }
    async fn release_and_answer(&self, _: Option<&Address>) -> Result<()> {
        no(CALLS)
    }
    async fn merge_calls(&self, _: Option<&Address>) -> Result<()> {
        no(CALLS)
    }
    async fn status(&self, _: Option<&Address>) -> Result<PhoneStatus> {
        no(CALLS)
    }
    async fn volume(&self, _: Option<&Address>) -> Result<CallVolume> {
        no(CALLS)
    }
    async fn set_volume(&self, _: Option<&Address>, _: CallVolume) -> Result<()> {
        no(CALLS)
    }
}

#[async_trait]
impl AudioRouter for Unsupported {
    async fn start(&self, _: &Address) -> Result<()> {
        no(AUDIO)
    }
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
    async fn is_active(&self) -> bool {
        false
    }
    async fn set_mic_muted(&self, _: bool) -> Result<()> {
        no(AUDIO)
    }
}

#[async_trait]
impl ContactsProvider for Unsupported {
    async fn contacts(&self, _: &Address) -> Result<Vec<Contact>> {
        no(CONTACTS)
    }
    async fn call_history(
        &self,
        _: &Address,
        _: Option<CallHistoryKind>,
    ) -> Result<Vec<CallHistoryEntry>> {
        no(CONTACTS)
    }
}

#[async_trait]
impl MessagesProvider for Unsupported {
    async fn list(&self, _: &Address, _: MessageFolder, _: u16) -> Result<Vec<Message>> {
        no(MESSAGES)
    }
    async fn get(&self, _: &Address, _: &str) -> Result<Message> {
        no(MESSAGES)
    }
    async fn send(&self, _: &Address, _: &str, _: &str) -> Result<()> {
        no(MESSAGES)
    }
    async fn watch(&self, _: &Address) -> Result<()> {
        Ok(())
    }
}
