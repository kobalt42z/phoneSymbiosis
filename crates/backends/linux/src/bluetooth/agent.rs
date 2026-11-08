//! BlueZ pairing agent (`org.bluez.Agent1`).
//!
//! When an iPhone pairs, both sides show a 6-digit code and the user must
//! confirm that they match ("numeric comparison"). BlueZ asks the registered
//! agent to do that confirmation. Our agent turns BlueZ's question into a
//! [`PhoneEvent::PairingConfirm`] and waits for the front end to answer via
//! [`BluetoothManager::confirm_pairing`](phonesym_core::BluetoothManager::confirm_pairing).
//!
//! If another agent (e.g. GNOME Settings) is already the default, BlueZ still
//! uses *ours* for pairings we start ourselves, because it prefers the agent
//! of the process that called `Pair()`.

use super::props::address_from_path;
use crate::dbus_util::{self, get_bool};
use phonesym_core::{Address, EventBus, PhoneEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use zbus::zvariant::ObjectPath;
use zbus::Connection;

/// Where our agent object lives on the system bus.
pub const AGENT_PATH: &str = "/org/phonesym/agent";
/// We can show a code and the user can say yes/no: BlueZ picks numeric
/// comparison for phones with this capability.
const CAPABILITY: &str = "DisplayYesNo";
/// How long we wait for the user to confirm a pairing code.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);

/// Pairing confirmations waiting for an answer from the user.
#[derive(Clone, Default)]
pub struct PendingConfirmations(Arc<Mutex<HashMap<Address, oneshot::Sender<bool>>>>);

impl PendingConfirmations {
    fn insert(&self, address: Address) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut map) = self.0.lock() {
            map.insert(address, tx);
        }
        rx
    }

    fn remove(&self, address: &Address) -> Option<oneshot::Sender<bool>> {
        self.0.lock().ok()?.remove(address)
    }

    /// Deliver the user's answer. Returns `false` if nothing was waiting.
    pub fn answer(&self, address: &Address, accept: bool) -> bool {
        match self.remove(address) {
            Some(tx) => tx.send(accept).is_ok(),
            None => false,
        }
    }
}

/// Errors BlueZ understands from an agent.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}

pub struct Agent {
    events: EventBus,
    pending: PendingConfirmations,
    conn: Connection,
}

impl Agent {
    fn address(device: &ObjectPath<'_>) -> Result<Address, AgentError> {
        address_from_path(device.as_str())
            .ok_or_else(|| AgentError::Rejected(format!("unknown device {device}")))
    }

    /// Only devices that are already paired may use services without asking.
    async fn is_paired(&self, device: &ObjectPath<'_>) -> bool {
        let props = dbus_util::get_all_properties(
            &self.conn,
            "org.bluez",
            device.as_str(),
            "org.bluez.Device1",
        )
        .await;
        props
            .ok()
            .and_then(|p| get_bool(&p, "Paired"))
            .unwrap_or(false)
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    /// BlueZ unregistered us (e.g. bluetoothd is shutting down).
    fn release(&self) {
        tracing::info!("pairing agent released by BlueZ");
    }

    /// Legacy PIN pairing. iPhones always use Secure Simple Pairing, so we
    /// don't support this rather than guess a PIN.
    fn request_pin_code(&self, _device: ObjectPath<'_>) -> Result<String, AgentError> {
        Err(AgentError::Rejected("PIN pairing not supported".into()))
    }

    /// Passkey entry is for keyboards; not used by phones.
    fn request_passkey(&self, _device: ObjectPath<'_>) -> Result<u32, AgentError> {
        Err(AgentError::Rejected("passkey entry not supported".into()))
    }

    fn display_pin_code(&self, device: ObjectPath<'_>, pincode: String) {
        tracing::info!("PIN for {device}: {pincode}");
    }

    fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, _entered: u16) {
        if let Ok(address) = Self::address(&device) {
            self.events
                .emit(PhoneEvent::PairingConfirm { address, passkey });
        }
    }

    /// Numeric comparison: ask the user whether `passkey` matches the phone.
    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> Result<(), AgentError> {
        let address = Self::address(&device)?;
        let answer = self.pending.insert(address.clone());
        self.events.emit(PhoneEvent::PairingConfirm {
            address: address.clone(),
            passkey,
        });

        let result = tokio::time::timeout(CONFIRM_TIMEOUT, answer).await;
        self.pending.remove(&address);
        match result {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(AgentError::Rejected(
                "user rejected the pairing code".into(),
            )),
            Ok(Err(_)) | Err(_) => Err(AgentError::Canceled("no answer to pairing request".into())),
        }
    }

    /// "Just works" pairing started by the phone: only allow it if the user
    /// confirms, like numeric comparison (passkey 0 means "no code").
    async fn request_authorization(&self, device: ObjectPath<'_>) -> Result<(), AgentError> {
        self.request_confirmation(device, 0).await
    }

    /// A device wants to use a profile (HFP, PBAP...). Allow paired phones,
    /// reject strangers.
    async fn authorize_service(
        &self,
        device: ObjectPath<'_>,
        _uuid: String,
    ) -> Result<(), AgentError> {
        if self.is_paired(&device).await {
            Ok(())
        } else {
            Err(AgentError::Rejected("device is not paired".into()))
        }
    }

    /// BlueZ gave up on the request (timeout on the phone, etc.).
    fn cancel(&self) {
        tracing::info!("pairing request cancelled by BlueZ");
    }
}

/// Serve the agent on `conn` and register it with BlueZ.
///
/// Becoming the *default* agent is best effort: a desktop environment
/// usually already has one, and that's fine.
pub async fn register(
    conn: &Connection,
    events: EventBus,
    pending: PendingConfirmations,
) -> zbus::Result<()> {
    let agent = Agent {
        events,
        pending,
        conn: conn.clone(),
    };
    conn.object_server().at(AGENT_PATH, agent).await?;

    let path = ObjectPath::try_from(AGENT_PATH)?;
    dbus_util::call_void(
        conn,
        "org.bluez",
        "/org/bluez",
        "org.bluez.AgentManager1",
        "RegisterAgent",
        &(&path, CAPABILITY),
    )
    .await?;
    if let Err(e) = dbus_util::call_void(
        conn,
        "org.bluez",
        "/org/bluez",
        "org.bluez.AgentManager1",
        "RequestDefaultAgent",
        &(&path,),
    )
    .await
    {
        tracing::debug!("not the default pairing agent (another one is): {e}");
    }
    Ok(())
}
