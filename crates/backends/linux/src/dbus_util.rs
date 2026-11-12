//! Small helpers for reading D-Bus property dictionaries.

use phonesym_core::Error;
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;

pub type PropMap = HashMap<String, OwnedValue>;

pub fn get_str(props: &PropMap, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|v| v.downcast_ref::<&str>().ok())
        .map(str::to_owned)
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
            if name.ends_with("InvalidState")
                || name.ends_with("NotReady")
                || name.ends_with("InProgress")
            {
                Error::InvalidState(format!("{context}: {msg}"))
            } else if name.ends_with("InvalidArgs")
                || name.ends_with("InvalidArguments")
                || name.ends_with("InvalidFormat")
            {
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

// ---------------------------------------------------------------------------
// Generic call helpers.
//
// We talk to several services (BlueZ, PipeWire telephony, oFono) whose bus
// name and object paths are only known at runtime, so instead of generated
// proxies we use these few small helpers around `Connection::call_method`.
// ---------------------------------------------------------------------------

use serde::{de::DeserializeOwned, Serialize};
use zbus::zvariant::{DynamicType, OwnedObjectPath, Value};
use zbus::Connection;

/// `GetManagedObjects` result: object path -> interface name -> properties.
pub type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, PropMap>>;

const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

/// Call a method and decode its reply body.
pub async fn call<B, R>(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<R>
where
    B: Serialize + DynamicType,
    R: DeserializeOwned + zbus::zvariant::Type,
{
    let reply = conn
        .call_method(Some(dest), path, Some(iface), method, body)
        .await?;
    reply.body().deserialize::<R>()
}

/// Objects with their properties, as returned by oFono-style `GetModems` /
/// `GetCalls`.
pub type ObjectList = Vec<(zbus::zvariant::OwnedObjectPath, PropMap)>;

/// Call an oFono-style `GetModems` / `GetCalls`.
///
/// oFono documents the reply as an array of structs, `a(oa{sv})`, but some
/// PipeWire releases send a dictionary, `a{oa{sv}}`, with the same content.
/// Accept both so the app works on either.
pub async fn call_object_list(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
) -> zbus::Result<ObjectList> {
    let reply = conn
        .call_method(Some(dest), path, Some(iface), method, &())
        .await?;
    parse_object_list(&reply)
}

/// Decode either reply shape of [`call_object_list`].
pub fn parse_object_list(reply: &zbus::Message) -> zbus::Result<ObjectList> {
    let body = reply.body();
    if body.signature().to_string() == "a{oa{sv}}" {
        let map: HashMap<zbus::zvariant::OwnedObjectPath, PropMap> = body.deserialize()?;
        let mut list: ObjectList = map.into_iter().collect();
        // Keep a stable order (ag0 before ag1, call0 before call1).
        list.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        Ok(list)
    } else {
        body.deserialize()
    }
}

/// Call a method and ignore whatever it returns (some services return an
/// object path, others nothing, for the same method).
pub async fn call_void<B>(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<()>
where
    B: Serialize + DynamicType,
{
    conn.call_method(Some(dest), path, Some(iface), method, body)
        .await?;
    Ok(())
}

/// `org.freedesktop.DBus.Properties.Get`.
pub async fn get_property(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
) -> zbus::Result<OwnedValue> {
    call(conn, dest, path, PROPERTIES_IFACE, "Get", &(iface, name)).await
}

/// `org.freedesktop.DBus.Properties.GetAll`.
pub async fn get_all_properties(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
) -> zbus::Result<PropMap> {
    call(conn, dest, path, PROPERTIES_IFACE, "GetAll", &(iface,)).await
}

/// `org.freedesktop.DBus.Properties.Set`.
pub async fn set_property(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
    value: Value<'_>,
) -> zbus::Result<()> {
    call_void(
        conn,
        dest,
        path,
        PROPERTIES_IFACE,
        "Set",
        &(iface, name, value),
    )
    .await
}

/// `org.freedesktop.DBus.ObjectManager.GetManagedObjects`.
pub async fn managed_objects(
    conn: &Connection,
    dest: &str,
    path: &str,
) -> zbus::Result<ManagedObjects> {
    call(
        conn,
        dest,
        path,
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        &(),
    )
    .await
}

/// Is somebody currently owning `name` on this bus? Cheap way to tell whether
/// a service (BlueZ, PipeWire telephony, obexd...) is running.
pub async fn name_has_owner(conn: &Connection, name: &str) -> zbus::Result<bool> {
    call(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        &(name,),
    )
    .await
}

/// Build a match rule for signals of one interface (optionally one member,
/// optionally below a path) and return the message stream for it.
pub async fn signal_stream(
    conn: &Connection,
    iface: &str,
    member: Option<&str>,
    path_namespace: Option<&str>,
) -> zbus::Result<zbus::MessageStream> {
    let mut rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(iface)?;
    if let Some(m) = member {
        rule = rule.member(m)?;
    }
    if let Some(p) = path_namespace {
        rule = rule.path_namespace(p)?;
    }
    zbus::MessageStream::for_match_rule(rule.build(), conn, Some(64)).await
}

/// Like [`get_u8`], but also accepts the other integer types some services use.
pub fn get_u8_lenient(props: &PropMap, key: &str) -> Option<u8> {
    let v = props.get(key)?;
    if let Ok(x) = v.downcast_ref::<u8>() {
        return Some(x);
    }
    if let Ok(x) = v.downcast_ref::<u16>() {
        return u8::try_from(x).ok();
    }
    if let Ok(x) = v.downcast_ref::<u32>() {
        return u8::try_from(x).ok();
    }
    None
}

#[cfg(test)]
mod object_list_tests {
    use super::*;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    fn props() -> PropMap {
        PropMap::from([("State".to_string(), OwnedValue::from(zbus::zvariant::Str::from("active")))])
    }

    #[test]
    fn reads_array_of_structs() {
        let path = OwnedObjectPath::try_from("/ag0").unwrap();
        let body: ObjectList = vec![(path, props())];
        let msg = zbus::Message::method_call("/", "GetModems").unwrap().build(&(body,)).unwrap();
        let list = parse_object_list(&msg).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0.as_str(), "/ag0");
    }

    #[test]
    fn reads_dictionary_sent_by_some_pipewire_versions() {
        let mut body: HashMap<OwnedObjectPath, PropMap> = HashMap::new();
        body.insert(OwnedObjectPath::try_from("/ag1").unwrap(), props());
        body.insert(OwnedObjectPath::try_from("/ag0").unwrap(), props());
        let msg = zbus::Message::method_call("/", "GetModems").unwrap().build(&(body,)).unwrap();
        let list = parse_object_list(&msg).unwrap();
        let paths: Vec<_> = list.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["/ag0", "/ag1"]);
        assert_eq!(get_str(&list[0].1, "State").as_deref(), Some("active"));
    }
}
