//! One file per feature area. Each exposes a clap `...Cmd` enum and `run`.

pub mod call;
pub mod contacts;
pub mod device;
pub mod doctor;
pub mod sms;
pub mod watch;

use phonesym_platform::{Address, Platform};

/// Use the given address, or the connected phone.
///
/// Without an address we take the first connected device that looks like a
/// phone (offers calls, contacts or SMS), falling back to any connected
/// device, so a connected headset or mouse doesn't get picked by accident.
pub async fn pick_phone(platform: &Platform, address: Option<&str>) -> anyhow::Result<Address> {
    if let Some(a) = address {
        return Ok(Address::parse(a)?);
    }
    let connected: Vec<_> = platform.bluetooth.devices().await?.into_iter().filter(|d| d.connected).collect();
    let is_phone = |d: &&phonesym_platform::Device| {
        let c = &d.capabilities;
        c.calls || c.contacts || c.messages
    };
    connected
        .iter()
        .find(is_phone)
        .or_else(|| connected.first())
        .map(|d| d.address.clone())
        .ok_or_else(|| anyhow::anyhow!("no connected phone; pass --phone AA:BB:CC:DD:EE:FF or run `phonesym device connect`"))
}

/// `on` / `off` argument shared by several commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    pub fn is_on(self) -> bool {
        self == OnOff::On
    }
}

/// Print `{"ok": true, ...}` in JSON mode or a human sentence otherwise,
/// for commands that just perform an action.
pub fn print_done(ctx: &crate::Ctx, message: &str) {
    ctx.print(&serde_json::json!({ "ok": true, "message": message }), |_| println!("{message}"));
}
