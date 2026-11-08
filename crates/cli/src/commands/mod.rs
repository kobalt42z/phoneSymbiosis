//! One file per feature area. Each exposes a clap `...Cmd` enum and `run`.

pub mod call;
pub mod contacts;
pub mod device;
pub mod doctor;
pub mod sms;
pub mod watch;

use phonesym_platform::{Address, Platform};

/// Use the given address, or the first connected phone.
pub async fn pick_phone(platform: &Platform, address: Option<&str>) -> anyhow::Result<Address> {
    if let Some(a) = address {
        return Ok(Address::parse(a)?);
    }
    let devices = platform.bluetooth.devices().await?;
    devices
        .into_iter()
        .find(|d| d.connected)
        .map(|d| d.address)
        .ok_or_else(|| anyhow::anyhow!("no connected phone; pass --phone AA:BB:CC:DD:EE:FF or run `phonesym device connect`"))
}
