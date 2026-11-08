//! Plain data types shared by every backend and front end.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A Bluetooth hardware address, always stored as upper-case `AA:BB:CC:DD:EE:FF`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct Address(String);

impl Address {
    pub fn parse(s: &str) -> crate::Result<Self> {
        let s = s.trim().replace(['_', '-'], ":").to_ascii_uppercase();
        let parts: Vec<&str> = s.split(':').collect();
        let ok = parts.len() == 6
            && parts
                .iter()
                .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
        if ok {
            Ok(Self(s))
        } else {
            Err(crate::Error::InvalidArgument(format!("not a Bluetooth address: {s}")))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `AA_BB_CC_DD_EE_FF`, the form used in BlueZ object paths and PipeWire node names.
    pub fn underscored(&self) -> String {
        self.0.replace(':', "_")
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Address {
    type Error = crate::Error;
    fn try_from(s: String) -> crate::Result<Self> {
        Address::parse(&s)
    }
}

impl From<Address> for String {
    fn from(a: Address) -> String {
        a.0
    }
}

/// Info about the local Bluetooth adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub address: Address,
    pub name: String,
    pub powered: bool,
    pub discoverable: bool,
}

/// What a remote device can do for us, based on the Bluetooth profiles it advertises.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Hands-Free Profile, phone side (calls).
    pub calls: bool,
    /// Phone Book Access Profile, server side (contacts and call history).
    pub contacts: bool,
    /// Message Access Profile, server side (SMS).
    pub messages: bool,
}

/// A remote Bluetooth device (normally a phone).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub address: Address,
    pub name: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    /// Signal strength while scanning, if known.
    pub rssi: Option<i16>,
    pub capabilities: Capabilities,
}

/// Opaque identifier of a call. Backends choose the format (e.g. a D-Bus object path).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallId(pub String);

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    /// Ringing on our side.
    Incoming,
    /// A second call arrived while one is active.
    Waiting,
    /// We started an outgoing call, network is setting it up.
    Dialing,
    /// The other side is ringing.
    Alerting,
    Active,
    Held,
    Disconnected,
}

impl CallState {
    /// Parse the state strings used by oFono / PipeWire telephony / most HFP stacks.
    pub fn from_hfp_str(s: &str) -> Option<Self> {
        Some(match s {
            "incoming" => Self::Incoming,
            "waiting" => Self::Waiting,
            "dialing" => Self::Dialing,
            "alerting" => Self::Alerting,
            "active" => Self::Active,
            "held" => Self::Held,
            "disconnected" => Self::Disconnected,
            _ => return None,
        })
    }

    pub fn is_ringing(self) -> bool {
        matches!(self, Self::Incoming | Self::Waiting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallDirection {
    Incoming,
    Outgoing,
}

/// A call currently known to the phone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: CallId,
    /// The phone this call belongs to.
    pub device: Option<Address>,
    /// Number of the other party (may be empty or "withheld").
    pub number: String,
    /// Caller name if the network or phone provided one.
    pub name: Option<String>,
    pub state: CallState,
    pub direction: CallDirection,
    pub multiparty: bool,
}

/// Phone status indicators reported over hands-free.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneStatus {
    pub device: Option<Address>,
    /// 0-100 percent, if reported.
    pub battery: Option<u8>,
    /// 0-5 bars, if reported.
    pub signal: Option<u8>,
    pub operator: Option<String>,
    pub roaming: Option<bool>,
}

/// Speaker / microphone volume for the call audio, 0-15 like HFP.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CallVolume {
    pub speaker: u8,
    pub microphone: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneNumber {
    pub number: String,
    /// "cell", "home", "work", ... as written in the vCard TYPE parameter.
    pub kind: Option<String>,
}

/// A contact from the phone's address book.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// Stable id within one download (vCard handle or index).
    pub id: String,
    pub name: String,
    pub numbers: Vec<PhoneNumber>,
    pub emails: Vec<String>,
    pub organization: Option<String>,
    /// Base64 photo if the phone sent one.
    pub photo_base64: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallHistoryKind {
    Incoming,
    Outgoing,
    Missed,
}

/// One entry of the phone's call log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallHistoryEntry {
    pub kind: CallHistoryKind,
    pub number: String,
    pub name: Option<String>,
    /// ISO-8601 local time as sent by the phone (e.g. `20261108T110455`), if any.
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageFolder {
    Inbox,
    Sent,
    Outbox,
    Draft,
    Deleted,
}

impl MessageFolder {
    /// Folder name used by the Message Access Profile.
    pub fn map_name(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Sent => "sent",
            Self::Outbox => "outbox",
            Self::Draft => "draft",
            Self::Deleted => "deleted",
        }
    }
}

/// An SMS message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Backend-specific handle, used to fetch the full text or mark read.
    pub id: String,
    pub folder: MessageFolder,
    /// Number (and name if known) of the other party.
    pub peer_number: String,
    pub peer_name: Option<String>,
    /// Short preview or full text, depending on what the phone sent.
    pub text: String,
    pub timestamp: Option<String>,
    pub read: bool,
}
