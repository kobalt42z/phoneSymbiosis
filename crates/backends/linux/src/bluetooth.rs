//! BlueZ (`org.bluez` on the system bus): adapter, scanning, pairing, connecting.
//!
//! - Objects are found with the standard ObjectManager on `/`.
//! - Adapters are `/org/bluez/hciN` (`org.bluez.Adapter1`), phones are
//!   `/org/bluez/hciN/dev_AA_BB_CC_DD_EE_FF` (`org.bluez.Device1`).
//! - A pairing agent ([`agent`]) answers BlueZ's "do the codes match?" question.
//! - A background task ([`watch_devices`]) turns BlueZ signals into
//!   [`PhoneEvent`]s (device connected / disconnected / found).
//!
//! The system bus connection is opened lazily, so the rest of the app keeps
//! working (with a clear error here) when BlueZ or D-Bus is missing.

mod agent;
pub mod props;

use crate::dbus_util::{self, map_err, ManagedObjects};
use async_trait::async_trait;
use futures_util::StreamExt;
use phonesym_core::*;
use props::{adapter_from_props, address_from_path, device_from_props, device_path};
use std::time::Duration;
use tokio::sync::OnceCell;
use zbus::zvariant::{ObjectPath, Value};
use zbus::Connection;

const BLUEZ: &str = "org.bluez";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";

pub struct BluezManager {
    events: EventBus,
    /// Adapter chosen by the user ("hci0"), or `None` for the first one.
    adapter_name: Option<String>,
    conn: OnceCell<Connection>,
    pending: agent::PendingConfirmations,
}

impl BluezManager {
    pub async fn new(config: &PlatformConfig, events: EventBus) -> Result<Self> {
        let me = Self {
            events,
            adapter_name: config.adapter.clone(),
            conn: OnceCell::new(),
            pending: agent::PendingConfirmations::default(),
        };
        // Try now so the agent and event watcher start early; if it fails we
        // retry on the first real call instead of failing the whole app.
        if let Err(e) = me.conn().await {
            tracing::debug!("Bluetooth not ready yet: {e}");
        }
        Ok(me)
    }

    /// The system bus connection, set up on first use with the pairing agent
    /// and the event watcher.
    async fn conn(&self) -> Result<&Connection> {
        self.conn
            .get_or_try_init(|| async {
                let conn = Connection::system()
                    .await
                    .map_err(|e| Error::ServiceUnavailable(format!("system D-Bus: {e}")))?;
                if let Err(e) =
                    agent::register(&conn, self.events.clone(), self.pending.clone()).await
                {
                    tracing::warn!(
                        "could not register pairing agent (is bluetoothd running?): {e}"
                    );
                }
                tokio::spawn(watch_devices(conn.clone(), self.events.clone()));
                Ok(conn)
            })
            .await
    }

    async fn objects(&self) -> Result<ManagedObjects> {
        let conn = self.conn().await?;
        dbus_util::managed_objects(conn, BLUEZ, "/")
            .await
            .map_err(|e| match map_err("BlueZ", e) {
                Error::ServiceUnavailable(m) => Error::ServiceUnavailable(format!(
                    "{m} (start it with: sudo systemctl start bluetooth)"
                )),
                other => other,
            })
    }

    /// Object path of the adapter to use.
    async fn adapter_path(&self) -> Result<String> {
        let objects = self.objects().await?;
        pick_adapter(&objects, self.adapter_name.as_deref())
    }

    /// Object path of a device, checking that BlueZ knows it.
    async fn device_path(&self, address: &Address) -> Result<String> {
        let adapter = self.adapter_path().await?;
        let path = device_path(&adapter, address);
        let objects = self.objects().await?;
        let known = objects
            .iter()
            .any(|(p, ifaces)| p.as_str() == path && ifaces.contains_key(DEVICE_IFACE));
        if known {
            Ok(path)
        } else {
            Err(Error::DeviceNotFound(format!(
                "{address} is unknown; run `phonesym device scan` with the phone's Bluetooth settings open"
            )))
        }
    }

    async fn device_call(&self, address: &Address, method: &str) -> Result<()> {
        let path = self.device_path(address).await?;
        let conn = self.conn().await?;
        dbus_util::call_void(conn, BLUEZ, &path, DEVICE_IFACE, method, &())
            .await
            .map_err(|e| map_err(&format!("{method} {address}"), e))
    }

    async fn set_device_prop(&self, address: &Address, name: &str, value: Value<'_>) -> Result<()> {
        let path = self.device_path(address).await?;
        let conn = self.conn().await?;
        dbus_util::set_property(conn, BLUEZ, &path, DEVICE_IFACE, name, value)
            .await
            .map_err(|e| map_err(&format!("set {name} on {address}"), e))
    }

    async fn adapter_call(&self, method: &str) -> Result<()> {
        let path = self.adapter_path().await?;
        let conn = self.conn().await?;
        dbus_util::call_void(conn, BLUEZ, &path, ADAPTER_IFACE, method, &())
            .await
            .map_err(|e| map_err(method, e))
    }
}

/// Choose the adapter: the one the user named, or the first one (sorted, so
/// `hci0` wins over `hci1`).
fn pick_adapter(objects: &ManagedObjects, wanted: Option<&str>) -> Result<String> {
    let mut adapters: Vec<&str> = objects
        .iter()
        .filter(|(_, ifaces)| ifaces.contains_key(ADAPTER_IFACE))
        .map(|(path, _)| path.as_str())
        .collect();
    adapters.sort_unstable();
    let found = match wanted {
        Some(name) => adapters
            .into_iter()
            .find(|p| p.rsplit('/').next() == Some(name)),
        None => adapters.into_iter().next(),
    };
    found.map(str::to_owned).ok_or_else(|| {
        Error::AdapterUnavailable(match wanted {
            Some(name) => format!("no adapter named {name}"),
            None => "no Bluetooth adapter found (is it plugged in / unblocked? try `rfkill unblock bluetooth`)".into(),
        })
    })
}

/// All devices below one adapter.
fn devices_of(objects: &ManagedObjects, adapter_path: &str) -> Vec<Device> {
    let prefix = format!("{adapter_path}/");
    let mut devices: Vec<Device> = objects
        .iter()
        .filter(|(path, _)| path.as_str().starts_with(&prefix))
        .filter_map(|(_, ifaces)| ifaces.get(DEVICE_IFACE))
        .filter_map(device_from_props)
        .collect();
    // Paired phones first, then by name: what a user expects to see.
    devices.sort_by(|a, b| b.paired.cmp(&a.paired).then_with(|| a.name.cmp(&b.name)));
    devices
}

#[async_trait]
impl BluetoothManager for BluezManager {
    async fn adapter(&self) -> Result<AdapterInfo> {
        let objects = self.objects().await?;
        let path = pick_adapter(&objects, self.adapter_name.as_deref())?;
        objects
            .iter()
            .find(|(p, _)| p.as_str() == path)
            .and_then(|(_, ifaces)| ifaces.get(ADAPTER_IFACE))
            .and_then(adapter_from_props)
            .ok_or_else(|| Error::AdapterUnavailable(format!("{path} has no readable properties")))
    }

    async fn set_powered(&self, on: bool) -> Result<()> {
        let path = self.adapter_path().await?;
        let conn = self.conn().await?;
        dbus_util::set_property(
            conn,
            BLUEZ,
            &path,
            ADAPTER_IFACE,
            "Powered",
            Value::from(on),
        )
        .await
        .map_err(|e| map_err("set Powered", e))
    }

    async fn scan(&self, duration: Duration) -> Result<Vec<Device>> {
        let adapter = self.adapter_path().await?;
        let conn = self.conn().await?;
        // Phones do HFP over classic Bluetooth (BR/EDR), so don't waste the
        // scan on BLE-only gadgets. Not fatal if the filter is refused.
        let filter = std::collections::HashMap::from([("Transport", Value::from("bredr"))]);
        if let Err(e) = dbus_util::call_void(
            conn,
            BLUEZ,
            &adapter,
            ADAPTER_IFACE,
            "SetDiscoveryFilter",
            &(filter,),
        )
        .await
        {
            tracing::debug!("SetDiscoveryFilter failed: {e}");
        }
        self.adapter_call("StartDiscovery").await?;
        tokio::time::sleep(duration).await;
        // Stopping can fail if bluetoothd ended the scan itself; harmless.
        if let Err(e) = self.adapter_call("StopDiscovery").await {
            tracing::debug!("StopDiscovery: {e}");
        }
        self.devices().await
    }

    async fn devices(&self) -> Result<Vec<Device>> {
        let objects = self.objects().await?;
        let adapter = pick_adapter(&objects, self.adapter_name.as_deref())?;
        Ok(devices_of(&objects, &adapter))
    }

    async fn device(&self, address: &Address) -> Result<Device> {
        self.devices()
            .await?
            .into_iter()
            .find(|d| &d.address == address)
            .ok_or_else(|| Error::DeviceNotFound(address.to_string()))
    }

    async fn pair(&self, address: &Address) -> Result<()> {
        let already = self.device(address).await?.paired;
        if !already {
            match self.device_call(address, "Pair").await {
                Ok(()) => {}
                // "AlreadyExists" means it got paired meanwhile: fine.
                Err(Error::Backend(m)) if m.contains("AlreadyExists") => {}
                Err(e) => return Err(e),
            }
        }
        // Trusted lets the phone reconnect by itself later without prompts.
        self.set_device_prop(address, "Trusted", Value::from(true))
            .await?;
        self.connect(address).await
    }

    async fn confirm_pairing(&self, address: &Address, accept: bool) -> Result<()> {
        if self.pending.answer(address, accept) {
            Ok(())
        } else {
            Err(Error::InvalidState(format!(
                "no pairing request waiting for {address}"
            )))
        }
    }

    async fn connect(&self, address: &Address) -> Result<()> {
        self.device_call(address, "Connect").await
    }

    async fn disconnect(&self, address: &Address) -> Result<()> {
        self.device_call(address, "Disconnect").await
    }

    async fn forget(&self, address: &Address) -> Result<()> {
        let device = self.device_path(address).await?;
        let adapter = self.adapter_path().await?;
        let conn = self.conn().await?;
        let device =
            ObjectPath::try_from(device.as_str()).map_err(|e| Error::Backend(e.to_string()))?;
        dbus_util::call_void(
            conn,
            BLUEZ,
            &adapter,
            ADAPTER_IFACE,
            "RemoveDevice",
            &(&device,),
        )
        .await
        .map_err(|e| map_err(&format!("forget {address}"), e))
    }
}

/// Background task: publish device events from BlueZ signals.
///
/// - `PropertiesChanged` on `Device1` with `Connected` -> connected/disconnected.
/// - `InterfacesAdded` with `Device1` -> a new device was found (scanning).
async fn watch_devices(conn: Connection, events: EventBus) {
    let props = dbus_util::signal_stream(
        &conn,
        "org.freedesktop.DBus.Properties",
        Some("PropertiesChanged"),
        Some("/org/bluez"),
    )
    .await;
    let added = dbus_util::signal_stream(
        &conn,
        "org.freedesktop.DBus.ObjectManager",
        Some("InterfacesAdded"),
        None,
    )
    .await;
    let (props, added) = match (props, added) {
        (Ok(p), Ok(a)) => (p, a),
        (Err(e), _) | (_, Err(e)) => {
            tracing::warn!("cannot watch Bluetooth devices: {e}");
            return;
        }
    };
    let mut stream = futures_util::stream::select(props, added);
    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let member = header
            .member()
            .map(|m| m.as_str().to_owned())
            .unwrap_or_default();
        let path = header
            .path()
            .map(|p| p.as_str().to_owned())
            .unwrap_or_default();
        match member.as_str() {
            "PropertiesChanged" => on_properties_changed(&msg, &path, &events),
            "InterfacesAdded" => on_interfaces_added(&msg, &events),
            _ => {}
        }
    }
}

fn on_properties_changed(msg: &zbus::Message, path: &str, events: &EventBus) {
    let Ok((iface, changed, _)) = msg
        .body()
        .deserialize::<(String, dbus_util::PropMap, Vec<String>)>()
    else {
        return;
    };
    if iface != DEVICE_IFACE {
        return;
    }
    let (Some(connected), Some(address)) = (
        dbus_util::get_bool(&changed, "Connected"),
        address_from_path(path),
    ) else {
        return;
    };
    events.emit(if connected {
        PhoneEvent::DeviceConnected { address }
    } else {
        PhoneEvent::DeviceDisconnected { address }
    });
}

fn on_interfaces_added(msg: &zbus::Message, events: &EventBus) {
    type Added = (
        zbus::zvariant::OwnedObjectPath,
        std::collections::HashMap<String, dbus_util::PropMap>,
    );
    let Ok((_, ifaces)) = msg.body().deserialize::<Added>() else {
        return;
    };
    if let Some(device) = ifaces.get(DEVICE_IFACE).and_then(device_from_props) {
        events.emit(PhoneEvent::DeviceFound { device });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    fn ov<'a>(x: impl Into<Value<'a>>) -> OwnedValue {
        x.into().try_to_owned().expect("owned")
    }

    fn sample() -> ManagedObjects {
        let mut objects = ManagedObjects::new();
        for hci in ["/org/bluez/hci1", "/org/bluez/hci0"] {
            let mut props = dbus_util::PropMap::new();
            props.insert("Address".into(), ov("00:11:22:33:44:55"));
            objects.insert(
                OwnedObjectPath::try_from(hci).unwrap(),
                HashMap::from([(ADAPTER_IFACE.to_string(), props)]),
            );
        }
        let mut dev = dbus_util::PropMap::new();
        dev.insert("Address".into(), ov("A4:83:E7:12:34:56"));
        dev.insert("Paired".into(), ov(true));
        objects.insert(
            OwnedObjectPath::try_from("/org/bluez/hci0/dev_A4_83_E7_12_34_56").unwrap(),
            HashMap::from([(DEVICE_IFACE.to_string(), dev)]),
        );
        objects
    }

    #[test]
    fn picks_first_or_named_adapter() {
        let o = sample();
        assert_eq!(pick_adapter(&o, None).unwrap(), "/org/bluez/hci0");
        assert_eq!(pick_adapter(&o, Some("hci1")).unwrap(), "/org/bluez/hci1");
        assert!(matches!(
            pick_adapter(&o, Some("hci7")),
            Err(Error::AdapterUnavailable(_))
        ));
    }

    #[test]
    fn lists_devices_of_adapter() {
        let o = sample();
        assert_eq!(devices_of(&o, "/org/bluez/hci0").len(), 1);
        assert!(devices_of(&o, "/org/bluez/hci1").is_empty());
    }
}
