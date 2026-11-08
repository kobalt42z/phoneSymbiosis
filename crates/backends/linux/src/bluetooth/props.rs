//! Turning BlueZ property dictionaries into our plain data types.
//!
//! Kept free of any D-Bus calls so it can be unit tested.

use crate::dbus_util::{get_bool, get_i16, get_str, get_str_list, PropMap};
use phonesym_core::{AdapterInfo, Address, Capabilities, Device};

/// Hands-Free Profile, Audio Gateway role (the phone side of a car kit).
pub const UUID_HFP_AG: &str = "0000111f-0000-1000-8000-00805f9b34fb";
/// Phone Book Access Profile, server side (contacts + call history).
pub const UUID_PBAP_PSE: &str = "0000112f-0000-1000-8000-00805f9b34fb";
/// Message Access Profile, server side (SMS).
pub const UUID_MAP_MAS: &str = "00001132-0000-1000-8000-00805f9b34fb";

/// What the phone can do for us, based on the service UUIDs it advertises.
pub fn capabilities_from_uuids(uuids: &[String]) -> Capabilities {
    let has = |u: &str| uuids.iter().any(|x| x.eq_ignore_ascii_case(u));
    Capabilities {
        calls: has(UUID_HFP_AG),
        contacts: has(UUID_PBAP_PSE),
        messages: has(UUID_MAP_MAS),
    }
}

/// Build a [`Device`] from the properties of an `org.bluez.Device1` object.
/// Returns `None` if the mandatory `Address` property is missing or malformed.
pub fn device_from_props(props: &PropMap) -> Option<Device> {
    let address = Address::parse(&get_str(props, "Address")?).ok()?;
    // "Alias" is what the user sees in desktop settings; it falls back to
    // "Name" inside BlueZ, and to the address if the device never told us.
    let name = get_str(props, "Alias")
        .or_else(|| get_str(props, "Name"))
        .unwrap_or_else(|| address.to_string());
    Some(Device {
        name,
        paired: get_bool(props, "Paired").unwrap_or(false),
        trusted: get_bool(props, "Trusted").unwrap_or(false),
        connected: get_bool(props, "Connected").unwrap_or(false),
        rssi: get_i16(props, "RSSI"),
        capabilities: capabilities_from_uuids(&get_str_list(props, "UUIDs")),
        address,
    })
}

/// Build [`AdapterInfo`] from the properties of an `org.bluez.Adapter1` object.
pub fn adapter_from_props(props: &PropMap) -> Option<AdapterInfo> {
    let address = Address::parse(&get_str(props, "Address")?).ok()?;
    Some(AdapterInfo {
        name: get_str(props, "Alias")
            .or_else(|| get_str(props, "Name"))
            .unwrap_or_default(),
        powered: get_bool(props, "Powered").unwrap_or(false),
        discoverable: get_bool(props, "Discoverable").unwrap_or(false),
        address,
    })
}

/// `/org/bluez/hci0` + `AA:BB:..` -> `/org/bluez/hci0/dev_AA_BB_..`.
pub fn device_path(adapter_path: &str, address: &Address) -> String {
    format!("{adapter_path}/dev_{}", address.underscored())
}

/// Extract the address from any object path containing a `dev_AA_BB_..`
/// segment. BlueZ device paths, oFono HFP modem paths and BlueZ transport
/// paths all follow this convention.
pub fn address_from_path(path: &str) -> Option<Address> {
    path.split('/')
        .find_map(|seg| seg.strip_prefix("dev_"))
        .and_then(|a| Address::parse(a).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{OwnedValue, Value};

    fn v<'a>(x: impl Into<Value<'a>>) -> OwnedValue {
        x.into().try_to_owned().expect("owned value")
    }

    fn iphone_props() -> PropMap {
        let mut p = PropMap::new();
        p.insert("Address".into(), v("a4:83:e7:12:34:56"));
        p.insert("Alias".into(), v("Ana's iPhone"));
        p.insert("Paired".into(), v(true));
        p.insert("Trusted".into(), v(false));
        p.insert("Connected".into(), v(true));
        p.insert("RSSI".into(), v(-60i16));
        p.insert(
            "UUIDs".into(),
            v(vec![
                "0000110a-0000-1000-8000-00805f9b34fb".to_string(),
                UUID_HFP_AG.to_uppercase(),
                UUID_PBAP_PSE.to_string(),
            ]),
        );
        p
    }

    #[test]
    fn parses_device() {
        let d = device_from_props(&iphone_props()).expect("device");
        assert_eq!(d.address.as_str(), "A4:83:E7:12:34:56");
        assert_eq!(d.name, "Ana's iPhone");
        assert!(d.paired && d.connected && !d.trusted);
        assert_eq!(d.rssi, Some(-60));
        assert_eq!(
            d.capabilities,
            Capabilities {
                calls: true,
                contacts: true,
                messages: false
            }
        );
    }

    #[test]
    fn device_without_name_uses_address() {
        let mut p = PropMap::new();
        p.insert("Address".into(), v("A4:83:E7:12:34:56"));
        let d = device_from_props(&p).expect("device");
        assert_eq!(d.name, "A4:83:E7:12:34:56");
        assert!(!d.paired);
        assert_eq!(d.capabilities, Capabilities::default());
    }

    #[test]
    fn device_without_address_is_skipped() {
        assert!(device_from_props(&PropMap::new()).is_none());
    }

    #[test]
    fn paths() {
        let a = Address::parse("A4:83:E7:12:34:56").unwrap();
        let p = device_path("/org/bluez/hci0", &a);
        assert_eq!(p, "/org/bluez/hci0/dev_A4_83_E7_12_34_56");
        assert_eq!(address_from_path(&p), Some(a.clone()));
        assert_eq!(
            address_from_path("/hfp/org/bluez/hci0/dev_A4_83_E7_12_34_56"),
            Some(a)
        );
        assert_eq!(address_from_path("/org/pipewire/Telephony/ag0"), None);
    }
}
