//! Small helpers for reading D-Bus property dictionaries.

use phonesym_core::Error;
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;

pub type PropMap = HashMap<String, OwnedValue>;

pub fn get_str(props: &PropMap, key: &str) -> Option<String> {
    props.get(key).and_then(|v| v.downcast_ref::<&str>().ok()).map(str::to_owned)
}

pub fn get_bool(props: &PropMap, key: &str) -> Option<bool> {
    props.get(key).and_then(|v| v.downcast_ref::<bool>().ok())
}

pub fn get_u8(props: &PropMap, key: &str) -> Option<u8> {
    props.get(key).and_then(|v| v.downcast_ref::<u8>().ok())
}

pub fn get_i16(props: &PropMap, key: &str) -> Option<i16> {
    props.get(key).and_then(|v| v.downcast_ref::<i16>().ok())
}

pub fn get_u64(props: &PropMap, key: &str) -> Option<u64> {
    props.get(key).and_then(|v| v.downcast_ref::<u64>().ok())
}

pub fn get_str_list(props: &PropMap, key: &str) -> Vec<String> {
    props
        .get(key)
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_default()
}

/// Turn a zbus error into our error type, keeping D-Bus error names readable.
pub fn map_err(context: &str, e: zbus::Error) -> Error {
    match &e {
        zbus::Error::MethodError(name, msg, _) => {
            let name = name.as_str();
            let msg = msg.clone().unwrap_or_default();
            if name.ends_with("InvalidState") || name.ends_with("NotReady") || name.ends_with("InProgress") {
                Error::InvalidState(format!("{context}: {msg}"))
            } else if name.ends_with("InvalidArgs") || name.ends_with("InvalidArguments") || name.ends_with("InvalidFormat") {
                Error::InvalidArgument(format!("{context}: {msg}"))
            } else if name.ends_with("NotSupported") || name.ends_with("NotImplemented") {
                Error::NotSupported(format!("{context}: {msg}"))
            } else if name.ends_with("AuthenticationFailed")
                || name.ends_with("AuthenticationRejected")
                || name.ends_with("AuthenticationCanceled")
                || name.ends_with("Rejected")
                || name.ends_with("Forbidden")
            {
                Error::Refused(format!("{context}: {msg}"))
            } else if name.ends_with("ServiceUnknown") || name.ends_with("NameHasNoOwner") {
                Error::ServiceUnavailable(format!("{context}: {msg}"))
            } else if name.ends_with("UnknownObject") || name.ends_with("DoesNotExist") {
                Error::DeviceNotFound(format!("{context}: {msg}"))
            } else {
                Error::Backend(format!("{context}: {name}: {msg}"))
            }
        }
        _ => Error::Backend(format!("{context}: {e}")),
    }
}
