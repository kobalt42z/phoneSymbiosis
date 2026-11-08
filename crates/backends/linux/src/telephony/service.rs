//! Which D-Bus service provides hands-free call control on this machine.
//!
//! There are three possible providers, all speaking (mostly) the oFono API:
//!
//! | Provider                          | Bus     | Name                     | Manager path              |
//! |-----------------------------------|---------|--------------------------|---------------------------|
//! | PipeWire native HFP (Ubuntu default) | session | `org.pipewire.Telephony` | `/org/pipewire/Telephony` |
//! | PipeWire with `bluez5.telephony.provide-ofono` | system | `org.ofono` | `/` |
//! | Real oFono                        | system  | `org.ofono`              | `/`                       |
//!
//! PipeWire's native service also has its own interfaces
//! (`org.pipewire.Telephony.*`) which we prefer when available, because they
//! give the phone address directly and use the HFP 0-15 volume scale.

use crate::dbus_util;
use phonesym_core::{Error, Result};
use zbus::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Session,
    System,
}

/// Where to find the telephony service and which dialect it speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelephonyService {
    pub bus: Bus,
    pub name: &'static str,
    pub manager_path: &'static str,
    /// `true` for PipeWire's own interfaces, `false` for plain `org.ofono.*`.
    pub native: bool,
}

pub const PIPEWIRE: TelephonyService = TelephonyService {
    bus: Bus::Session,
    name: "org.pipewire.Telephony",
    manager_path: "/org/pipewire/Telephony",
    native: true,
};

pub const OFONO: TelephonyService = TelephonyService {
    bus: Bus::System,
    name: "org.ofono",
    manager_path: "/",
    native: false,
};

/// Tried in this order.
pub const CANDIDATES: [TelephonyService; 2] = [PIPEWIRE, OFONO];

// Interface names. The oFono ones are implemented by every provider.
pub const OFONO_MANAGER: &str = "org.ofono.Manager";
pub const OFONO_VOICE_CALL_MANAGER: &str = "org.ofono.VoiceCallManager";
pub const OFONO_VOICE_CALL: &str = "org.ofono.VoiceCall";
pub const OFONO_CALL_VOLUME: &str = "org.ofono.CallVolume";
pub const PW_AUDIO_GATEWAY: &str = "org.pipewire.Telephony.AudioGateway1";
pub const PW_AG_TRANSPORT: &str = "org.pipewire.Telephony.AudioGatewayTransport1";
pub const PW_CALL: &str = "org.pipewire.Telephony.Call1";

impl TelephonyService {
    /// Interface on a phone ("audio gateway" / "modem") object for Dial, SwapCalls...
    pub fn gateway_iface(&self) -> &'static str {
        if self.native {
            PW_AUDIO_GATEWAY
        } else {
            OFONO_VOICE_CALL_MANAGER
        }
    }

    /// Interface on a call object for Answer / Hangup.
    pub fn call_iface(&self) -> &'static str {
        if self.native {
            PW_CALL
        } else {
            OFONO_VOICE_CALL
        }
    }

    pub async fn connect(&self) -> zbus::Result<Connection> {
        match self.bus {
            Bus::Session => Connection::session().await,
            Bus::System => Connection::system().await,
        }
    }
}

/// Bus connections, opened on first use and reused.
#[derive(Default)]
pub struct Buses {
    session: tokio::sync::OnceCell<Connection>,
    system: tokio::sync::OnceCell<Connection>,
}

impl Buses {
    pub async fn get(&self, svc: &TelephonyService) -> zbus::Result<Connection> {
        let cell = match svc.bus {
            Bus::Session => &self.session,
            Bus::System => &self.system,
        };
        cell.get_or_try_init(|| svc.connect()).await.cloned()
    }

    /// Find the first provider that is running right now.
    ///
    /// Re-checked on every request, so it keeps working when PipeWire is
    /// restarted or the user switches to oFono.
    pub async fn resolve(&self) -> Result<(TelephonyService, Connection)> {
        for svc in CANDIDATES {
            let Ok(conn) = self.get(&svc).await else {
                continue;
            };
            if dbus_util::name_has_owner(&conn, svc.name)
                .await
                .unwrap_or(false)
            {
                return Ok((svc, conn));
            }
        }
        Err(Error::ServiceUnavailable(
            "no hands-free telephony service: need PipeWire's native HFP backend \
             (org.pipewire.Telephony on the session bus) or oFono"
                .into(),
        ))
    }
}
