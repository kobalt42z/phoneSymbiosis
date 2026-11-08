//! # Linux backend
//!
//! Talks to standard Ubuntu system services over D-Bus. No root needed.
//!
//! | Module          | Feature                    | Service used                               |
//! |-----------------|----------------------------|--------------------------------------------|
//! | [`bluetooth`]   | scan, pair, connect        | BlueZ `org.bluez` (system bus)             |
//! | [`telephony`]   | calls, DTMF, volume        | PipeWire `org.pipewire.Telephony` (session bus), or oFono |
//! | [`audio`]       | call audio <-> speakers/mic| PipeWire (`pw-dump`, `pw-loopback`)        |
//! | [`contacts`]    | contacts, call history     | BlueZ OBEX `org.bluez.obex` PBAP (session) |
//! | [`messages`]    | SMS                        | BlueZ OBEX `org.bluez.obex` MAP (session)  |

pub mod audio;
pub mod bluetooth;
pub mod contacts;
pub mod dbus_util;
pub mod messages;
pub mod obex;
pub mod telephony;

use phonesym_core::{EventBus, Platform, PlatformConfig, Result};
use std::sync::Arc;

pub const NAME: &str = "linux-bluez-pipewire";

/// Connect to the system services and build the [`Platform`].
pub async fn init(config: PlatformConfig) -> Result<Platform> {
    let events = EventBus::new();
    let bluetooth = Arc::new(bluetooth::BluezManager::new(&config, events.clone()).await?);
    let calls = Arc::new(telephony::Telephony::new(&config, events.clone()).await?);
    let audio = Arc::new(audio::PipewireAudio::new(&config, events.clone()));
    let contacts = Arc::new(contacts::PbapContacts::new(events.clone()).await?);
    let messages = Arc::new(messages::MapMessages::new(events.clone()).await?);

    if config.auto_audio {
        audio::spawn_auto_routing(audio.clone(), events.clone());
    }

    Ok(Platform {
        name: NAME,
        bluetooth,
        calls,
        audio,
        contacts,
        messages,
        events,
    })
}
