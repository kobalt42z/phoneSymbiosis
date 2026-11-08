//! `phonesym device ...`: find, pair and connect phones over Bluetooth.
//!
//! Typical first run with an iPhone:
//! 1. On the iPhone open Settings > Bluetooth (so it is visible).
//! 2. `phonesym device scan` and note its address.
//! 3. `phonesym device pair AA:BB:CC:DD:EE:FF`, check the code matches, type `y`.
//! 4. On the iPhone, allow "Sync Contacts" / "Show Notifications" if you want
//!    contacts and SMS.

use crate::commands::{print_done, OnOff};
use crate::Ctx;
use clap::Subcommand;
use phonesym_platform::{Address, BluetoothManager, Device, PhoneEvent};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

#[derive(Subcommand)]
pub enum DeviceCmd {
    /// Look for nearby devices, then list everything known.
    Scan {
        /// How long to scan.
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// List known (paired or recently seen) devices.
    List,
    /// Show one device.
    Info { address: String },
    /// Pair with a phone, trust it and connect.
    Pair { address: String },
    /// Connect to a paired phone.
    Connect { address: String },
    /// Disconnect a phone.
    Disconnect { address: String },
    /// Remove the pairing.
    Forget { address: String },
    /// Turn the Bluetooth adapter on or off.
    Power {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Show the local Bluetooth adapter.
    Adapter,
}

pub async fn run(ctx: &Ctx, cmd: DeviceCmd) -> anyhow::Result<()> {
    let bt = &ctx.platform.bluetooth;
    match cmd {
        DeviceCmd::Scan { seconds } => scan(ctx, seconds).await?,
        DeviceCmd::List => {
            let devices = bt.devices().await?;
            ctx.print(&devices, |d| print_devices(d));
        }
        DeviceCmd::Info { address } => {
            let device = bt.device(&Address::parse(&address)?).await?;
            ctx.print(&device, print_device_details);
        }
        DeviceCmd::Pair { address } => pair(ctx, Address::parse(&address)?).await?,
        DeviceCmd::Connect { address } => {
            let a = Address::parse(&address)?;
            bt.connect(&a).await?;
            print_done(ctx, &format!("Connected to {a}."));
        }
        DeviceCmd::Disconnect { address } => {
            let a = Address::parse(&address)?;
            bt.disconnect(&a).await?;
            print_done(ctx, &format!("Disconnected {a}."));
        }
        DeviceCmd::Forget { address } => {
            let a = Address::parse(&address)?;
            bt.forget(&a).await?;
            print_done(ctx, &format!("Removed pairing with {a}."));
        }
        DeviceCmd::Power { state } => {
            bt.set_powered(state.is_on()).await?;
            print_done(
                ctx,
                &format!(
                    "Bluetooth turned {}.",
                    if state.is_on() { "on" } else { "off" }
                ),
            );
        }
        DeviceCmd::Adapter => {
            let info = bt.adapter().await?;
            ctx.print(&info, |a| {
                println!("{} ({})", a.name, a.address);
                println!(
                    "  powered: {}  discoverable: {}",
                    yes_no(a.powered),
                    yes_no(a.discoverable)
                );
            });
        }
    }
    Ok(())
}

/// Scan, printing devices as they appear (human mode), then the full list.
async fn scan(ctx: &Ctx, seconds: u64) -> anyhow::Result<()> {
    let mut rx = ctx.platform.events.subscribe();
    if !ctx.json {
        eprintln!("Scanning for {seconds}s... (keep the phone's Bluetooth settings screen open)");
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                if let PhoneEvent::DeviceFound { device } = ev {
                    eprintln!("  found {}  {}", device.address, device.name);
                }
            }
        });
    }
    let devices = ctx
        .platform
        .bluetooth
        .scan(Duration::from_secs(seconds))
        .await?;
    ctx.print(&devices, |d| print_devices(d));
    Ok(())
}

/// Pair, answering the "do the codes match?" question on the terminal.
async fn pair(ctx: &Ctx, address: Address) -> anyhow::Result<()> {
    let bt = ctx.platform.bluetooth.clone();
    let mut rx = ctx.platform.events.subscribe();
    let wanted = address.clone();
    // Listen before starting so we can't miss the prompt.
    let prompter = tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let PhoneEvent::PairingConfirm { address, passkey } = ev {
                if address == wanted {
                    confirm_on_terminal(&bt, &address, passkey).await;
                }
            }
        }
    });
    eprintln!("Pairing with {address}... confirm on the phone when asked.");
    let result = ctx.platform.bluetooth.pair(&address).await;
    prompter.abort();
    result?;
    print_done(ctx, &format!("Paired, trusted and connected to {address}."));
    Ok(())
}

async fn confirm_on_terminal(bt: &Arc<dyn BluetoothManager>, address: &Address, passkey: u32) {
    eprint!("Does the phone show the code {passkey:06}? [y/N] ");
    let _ = std::io::stderr().flush();
    let answer = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).map(|_| line)
    })
    .await;
    let accept = matches!(answer, Ok(Ok(line)) if line.trim().eq_ignore_ascii_case("y") || line.trim().eq_ignore_ascii_case("yes"));
    if let Err(e) = bt.confirm_pairing(address, accept).await {
        eprintln!("Could not answer the pairing request: {e}");
    }
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Short "calls contacts sms" summary of what the phone offers.
fn capability_list(d: &Device) -> String {
    let c = &d.capabilities;
    let parts: Vec<&str> = [
        (c.calls, "calls"),
        (c.contacts, "contacts"),
        (c.messages, "sms"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(",")
    }
}

fn print_devices(devices: &[Device]) {
    if devices.is_empty() {
        println!(
            "No devices. Run `phonesym device scan` with the phone's Bluetooth settings open."
        );
        return;
    }
    println!("{:<17}  {:<9}  {:<20}  NAME", "ADDRESS", "STATE", "OFFERS");
    for d in devices {
        let state = if d.connected {
            "connected"
        } else if d.paired {
            "paired"
        } else {
            "seen"
        };
        println!(
            "{:<17}  {:<9}  {:<20}  {}",
            d.address,
            state,
            capability_list(d),
            d.name
        );
    }
}

fn print_device_details(d: &Device) {
    println!("{} ({})", d.name, d.address);
    println!(
        "  paired: {}  trusted: {}  connected: {}",
        yes_no(d.paired),
        yes_no(d.trusted),
        yes_no(d.connected)
    );
    if let Some(rssi) = d.rssi {
        println!("  signal: {rssi} dBm");
    }
    println!("  offers: {}", capability_list(d));
}
