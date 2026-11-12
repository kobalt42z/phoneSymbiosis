//! Bluetooth device management through the WinRT `Windows.Devices.*` APIs.
//!
//! WinRT calls block while they wait, so each one runs on Tokio's blocking pool.

use async_trait::async_trait;
use phonesym_core::*;
use std::time::Duration;
use windows::core::HSTRING;
use windows::Devices::Bluetooth::{
    BluetoothAdapter, BluetoothConnectionStatus, BluetoothDevice, BluetoothMajorClass,
};
use windows::Devices::Enumeration::{
    DeviceInformation, DevicePairingResultStatus, DeviceUnpairingResultStatus,
};
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioState};

pub struct WinBluetooth {
    events: EventBus,
}

fn backend(e: windows::core::Error) -> Error {
    Error::Backend(e.message().to_string())
}

/// Run a blocking WinRT call off the async runtime.
async fn blocking<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::Backend(e.to_string()))?
}

fn format_address(raw: u64) -> Address {
    let b = raw.to_be_bytes();
    let s = format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        b[2], b[3], b[4], b[5], b[6], b[7]
    );
    Address::parse(&s).expect("formatted address is valid")
}

fn parse_address(a: &Address) -> u64 {
    a.as_str().split(':').fold(0u64, |acc, p| {
        (acc << 8) | u64::from(u8::from_str_radix(p, 16).unwrap_or(0))
    })
}

fn default_adapter() -> Result<BluetoothAdapter> {
    BluetoothAdapter::GetDefaultAsync()
        .and_then(|op| op.join())
        .map_err(|e| Error::AdapterUnavailable(e.message().to_string()))
}

fn radio() -> Result<Radio> {
    default_adapter()?
        .GetRadioAsync()
        .and_then(|op| op.join())
        .map_err(|e| Error::AdapterUnavailable(e.message().to_string()))
}

fn to_device(dev: &BluetoothDevice, paired: bool) -> Result<Device> {
    let address = format_address(dev.BluetoothAddress().map_err(backend)?);
    let name = dev.Name().map(|n| n.to_string()).unwrap_or_default();
    let connected =
        dev.ConnectionStatus().map_err(backend)? == BluetoothConnectionStatus::Connected;
    let is_phone = dev
        .ClassOfDevice()
        .and_then(|c| c.MajorClass())
        .map(|m| m == BluetoothMajorClass::Phone)
        .unwrap_or(false);
    Ok(Device {
        address,
        name: if name.is_empty() {
            "Unknown device".into()
        } else {
            name
        },
        paired,
        trusted: paired,
        connected,
        rssi: None,
        capabilities: Capabilities {
            calls: is_phone,
            contacts: is_phone,
            messages: is_phone,
        },
    })
}

/// Devices matching a pairing state, as Windows knows them.
fn list(paired: bool) -> Result<Vec<Device>> {
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(paired).map_err(backend)?;
    let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)
        .and_then(|op| op.join())
        .map_err(backend)?;
    let mut out = Vec::new();
    for info in infos {
        let Ok(id) = info.Id() else { continue };
        match BluetoothDevice::FromIdAsync(&id).and_then(|op| op.join()) {
            Ok(dev) => {
                if let Ok(d) = to_device(&dev, paired) {
                    out.push(d);
                }
            }
            Err(e) => tracing::debug!("skipping {id}: {}", e.message()),
        }
    }
    Ok(out)
}

fn find(address: &Address) -> Result<BluetoothDevice> {
    BluetoothDevice::FromBluetoothAddressAsync(parse_address(address))
        .and_then(|op| op.join())
        .map_err(|_| Error::DeviceNotFound(address.to_string()))
}

fn info_of(dev: &BluetoothDevice) -> Result<DeviceInformation> {
    let id: HSTRING = dev.DeviceId().map_err(backend)?;
    DeviceInformation::CreateFromIdAsync(&id)
        .and_then(|op| op.join())
        .map_err(backend)
}

impl WinBluetooth {
    pub async fn new(events: EventBus) -> Result<Self> {
        // Fail early with a clear message when there is no Bluetooth adapter.
        blocking(|| default_adapter().map(|_| ())).await?;
        Ok(Self { events })
    }
}

#[async_trait]
impl BluetoothManager for WinBluetooth {
    async fn adapter(&self) -> Result<AdapterInfo> {
        blocking(|| {
            let adapter = default_adapter()?;
            let address = format_address(adapter.BluetoothAddress().map_err(backend)?);
            let radio = adapter
                .GetRadioAsync()
                .and_then(|op| op.join())
                .map_err(backend)?;
            Ok(AdapterInfo {
                address,
                name: radio
                    .Name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| "Bluetooth".into()),
                powered: radio.State().map_err(backend)? == RadioState::On,
                // Windows makes the PC discoverable only while Settings > Bluetooth is open.
                discoverable: false,
            })
        })
        .await
    }

    async fn set_powered(&self, on: bool) -> Result<()> {
        blocking(move || {
            let state = if on { RadioState::On } else { RadioState::Off };
            let status = radio()?
                .SetStateAsync(state)
                .and_then(|op| op.join())
                .map_err(backend)?;
            if status == RadioAccessStatus::Allowed {
                Ok(())
            } else {
                Err(Error::Refused(
                    "Windows did not allow changing the Bluetooth radio".into(),
                ))
            }
        })
        .await
    }

    async fn scan(&self, duration: Duration) -> Result<Vec<Device>> {
        // An unpaired-device query makes Windows run an inquiry scan.
        let found = tokio::time::timeout(
            duration.max(Duration::from_secs(5)) + Duration::from_secs(10),
            blocking(|| list(false)),
        )
        .await
        .unwrap_or_else(|_| Ok(Vec::new()))?;
        for d in &found {
            self.events
                .emit(PhoneEvent::DeviceFound { device: d.clone() });
        }
        let mut all = blocking(|| list(true)).await?;
        all.extend(found);
        Ok(all)
    }

    async fn devices(&self) -> Result<Vec<Device>> {
        blocking(|| list(true)).await
    }

    async fn device(&self, address: &Address) -> Result<Device> {
        let address = address.clone();
        blocking(move || {
            let dev = find(&address)?;
            let paired = info_of(&dev)?
                .Pairing()
                .and_then(|p| p.IsPaired())
                .unwrap_or(false);
            to_device(&dev, paired)
        })
        .await
    }

    async fn pair(&self, address: &Address) -> Result<()> {
        let address = address.clone();
        blocking(move || {
            let info = info_of(&find(&address)?)?;
            // Windows shows its own "Pair device?" prompt with the passkey.
            let result = info
                .Pairing()
                .and_then(|p| p.PairAsync())
                .and_then(|op| op.join())
                .map_err(backend)?;
            match result.Status().map_err(backend)? {
                DevicePairingResultStatus::Paired | DevicePairingResultStatus::AlreadyPaired => {
                    Ok(())
                }
                s => Err(Error::Refused(format!("pairing failed (status {})", s.0))),
            }
        })
        .await
    }

    async fn confirm_pairing(&self, _address: &Address, _accept: bool) -> Result<()> {
        // Windows asks the user itself during `pair`, so there is nothing to answer here.
        Ok(())
    }

    async fn connect(&self, address: &Address) -> Result<()> {
        let dev = self.device(address).await?;
        if dev.connected {
            self.events.emit(PhoneEvent::DeviceConnected {
                address: dev.address,
            });
            Ok(())
        } else {
            Err(Error::NotSupported(
                "connecting from the app. Windows connects paired devices itself; use Settings > Bluetooth & devices".into(),
            ))
        }
    }

    async fn disconnect(&self, _address: &Address) -> Result<()> {
        Err(Error::NotSupported(
            "disconnecting from the app. Use Settings > Bluetooth & devices, or Forget the phone"
                .into(),
        ))
    }

    async fn forget(&self, address: &Address) -> Result<()> {
        let address = address.clone();
        blocking(move || {
            let info = info_of(&find(&address)?)?;
            let result = info
                .Pairing()
                .and_then(|p| p.UnpairAsync())
                .and_then(|op| op.join())
                .map_err(backend)?;
            match result.Status().map_err(backend)? {
                DeviceUnpairingResultStatus::Unpaired
                | DeviceUnpairingResultStatus::AlreadyUnpaired => Ok(()),
                s => Err(Error::Backend(format!("unpairing failed (status {})", s.0))),
            }
        })
        .await
    }
}
