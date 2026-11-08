//! `phonesym watch`: print events as they happen.

use crate::Ctx;
use phonesym_platform::PhoneEvent;

pub async fn run(ctx: &Ctx) -> anyhow::Result<()> {
    let mut rx = ctx.platform.events.subscribe();
    eprintln!("Watching for events, press Ctrl+C to stop.");
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            ev = rx.recv() => match ev {
                Ok(ev) => print_event(ctx, &ev),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => eprintln!("(missed {n} events)"),
                Err(_) => break,
            }
        }
    }
    Ok(())
}

fn print_event(ctx: &Ctx, ev: &PhoneEvent) {
    if ctx.json {
        println!("{}", serde_json::to_string(ev).unwrap_or_default());
        return;
    }
    match ev {
        PhoneEvent::CallAdded { call } if call.state.is_ringing() => {
            println!("📞 Incoming call from {} {}", call.name.clone().unwrap_or_default(), call.number)
        }
        PhoneEvent::CallAdded { call } => println!("📞 Call {} {:?}", call.number, call.state),
        PhoneEvent::CallChanged { call } => println!("   {} is now {:?}", call.number, call.state),
        PhoneEvent::CallRemoved { id } => println!("   call ended ({id})"),
        PhoneEvent::DeviceConnected { address } => println!("🔗 {address} connected"),
        PhoneEvent::DeviceDisconnected { address } => println!("⛓  {address} disconnected"),
        PhoneEvent::CallsAvailable { address } => println!("✅ calls available on {address}"),
        PhoneEvent::CallsUnavailable { address } => println!("❌ calls unavailable on {address}"),
        PhoneEvent::MessageReceived { message } => {
            println!("💬 SMS from {}: {}", message.peer_name.clone().unwrap_or(message.peer_number.clone()), message.text)
        }
        PhoneEvent::AudioRouted { active } => println!("🔊 call audio {}", if *active { "on" } else { "off" }),
        PhoneEvent::PairingConfirm { address, passkey } => {
            println!("🔑 Confirm code {passkey:06} on {address}")
        }
        PhoneEvent::Warning { message } => println!("⚠️  {message}"),
        other => println!("{other:?}"),
    }
}
