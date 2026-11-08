//! Background task: turn telephony D-Bus signals into [`PhoneEvent`]s.
//!
//! One task per possible provider ([`TelephonyService`]). Each one:
//! 1. subscribes to the signals below on that provider's bus,
//! 2. loads the phones and calls that already exist,
//! 3. then updates the cache in [`Inner`] and publishes events as signals arrive.
//!
//! | Signal                                     | Event                         |
//! |--------------------------------------------|-------------------------------|
//! | `org.ofono.Manager.ModemAdded/Removed`     | `CallsAvailable/Unavailable`  |
//! | `org.ofono.VoiceCallManager.CallAdded/Removed` | `CallAdded/CallRemoved`   |
//! | `org.ofono.VoiceCall.PropertyChanged`, `Properties.PropertiesChanged` on `Call1` | `CallChanged` |
//! | `PropertiesChanged` on `AudioGateway1`, `org.ofono.CallVolume.PropertyChanged` | `VolumeChanged` |
//! | `NameOwnerChanged` of the provider         | everything gone / reloaded    |
//!
//! PipeWire sends both the oFono-style and the standard property signals for
//! the same change; [`props::apply_call_update`] ignores the duplicate.

use super::service::{
    TelephonyService, OFONO_CALL_VOLUME, OFONO_MANAGER, OFONO_VOICE_CALL, OFONO_VOICE_CALL_MANAGER,
    PW_AUDIO_GATEWAY, PW_CALL,
};
use super::{props, Inner};
use crate::dbus_util::{self, PropMap};
use futures_util::{stream::select_all, StreamExt};
use phonesym_core::{CallId, CallVolume, PhoneEvent};
use std::collections::HashMap;
use std::sync::Arc;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, Message, MessageStream};

pub(super) async fn run(inner: Arc<Inner>, svc: TelephonyService) {
    // No bus (e.g. no session bus under sudo): nothing to watch here.
    let Ok(conn) = inner.buses.get(&svc).await else {
        return;
    };
    let streams = match subscribe(&conn, &svc).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("cannot watch {}: {e}", svc.name);
            return;
        }
    };
    if dbus_util::name_has_owner(&conn, svc.name)
        .await
        .unwrap_or(false)
    {
        load_existing(&inner, &svc, &conn).await;
    }
    let mut all = select_all(streams);
    while let Some(msg) = all.next().await {
        if let Ok(msg) = msg {
            handle(&inner, &svc, &conn, &msg).await;
        }
    }
}

async fn subscribe(conn: &Connection, svc: &TelephonyService) -> zbus::Result<Vec<MessageStream>> {
    let mut streams = vec![
        dbus_util::signal_stream(conn, OFONO_MANAGER, None, None).await?,
        dbus_util::signal_stream(conn, OFONO_VOICE_CALL_MANAGER, None, None).await?,
        dbus_util::signal_stream(conn, OFONO_VOICE_CALL, Some("PropertyChanged"), None).await?,
        dbus_util::signal_stream(conn, OFONO_CALL_VOLUME, Some("PropertyChanged"), None).await?,
    ];
    // PipeWire's own objects always live under this path, even in oFono mode.
    streams.push(
        dbus_util::signal_stream(
            conn,
            "org.freedesktop.DBus.Properties",
            Some("PropertiesChanged"),
            Some("/org/pipewire/Telephony"),
        )
        .await?,
    );
    let owner_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg(0, svc.name)?
        .build();
    streams.push(MessageStream::for_match_rule(owner_rule, conn, Some(8)).await?);
    Ok(streams)
}

/// Announce phones and calls that existed before we started.
async fn load_existing(inner: &Inner, svc: &TelephonyService, conn: &Connection) {
    let gws = match inner.gateways_of(svc, conn).await {
        Ok(g) => g,
        Err(e) => {
            tracing::debug!("telephony: {e}");
            return;
        }
    };
    for (path, address) in gws {
        inner.events.emit(PhoneEvent::CallsAvailable {
            address: address.clone(),
        });
        if let Ok(calls) = inner.fetch_calls(svc, conn, &path, &address).await {
            for call in calls {
                inner.events.emit(PhoneEvent::CallAdded { call });
            }
        }
    }
}

async fn handle(inner: &Inner, svc: &TelephonyService, conn: &Connection, msg: &Message) {
    let header = msg.header();
    let (Some(iface), Some(member)) = (header.interface(), header.member()) else {
        return;
    };
    let path = header.path().map(|p| p.as_str()).unwrap_or_default();
    match (iface.as_str(), member.as_str()) {
        (OFONO_MANAGER, "ModemAdded") => {
            if let Ok((gw, props)) = msg.body().deserialize::<(OwnedObjectPath, PropMap)>() {
                on_gateway_added(inner, svc, conn, gw.as_str(), &props).await;
            }
        }
        (OFONO_MANAGER, "ModemRemoved") => {
            if let Ok((gw,)) = msg.body().deserialize::<(OwnedObjectPath,)>() {
                on_gateway_removed(inner, gw.as_str());
            }
        }
        (OFONO_VOICE_CALL_MANAGER, "CallAdded") => {
            if let Ok((call, props)) = msg.body().deserialize::<(OwnedObjectPath, PropMap)>() {
                on_call_added(inner, svc, conn, path, call.as_str(), &props).await;
            }
        }
        (OFONO_VOICE_CALL_MANAGER, "CallRemoved") => {
            if let Ok((call,)) = msg.body().deserialize::<(OwnedObjectPath,)>() {
                on_call_removed(inner, call.as_str());
            }
        }
        (OFONO_VOICE_CALL, "PropertyChanged") => {
            if let Some(changed) = single_property(msg) {
                on_call_changed(inner, path, &changed);
            }
        }
        (OFONO_CALL_VOLUME, "PropertyChanged") => {
            if let Some(changed) = single_property(msg) {
                on_volume_changed(inner, svc, conn, path, &changed, true).await;
            }
        }
        ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
            if let Ok((changed_iface, changed, _)) =
                msg.body().deserialize::<(String, PropMap, Vec<String>)>()
            {
                match changed_iface.as_str() {
                    PW_CALL => on_call_changed(inner, path, &changed),
                    PW_AUDIO_GATEWAY => {
                        on_volume_changed(inner, svc, conn, path, &changed, false).await
                    }
                    _ => {}
                }
            }
        }
        ("org.freedesktop.DBus", "NameOwnerChanged") => {
            if let Ok((_, _, new_owner)) = msg.body().deserialize::<(String, String, String)>() {
                // Gone or restarted: drop everything we knew from it.
                forget_service(inner, svc.name);
                if !new_owner.is_empty() {
                    load_existing(inner, svc, conn).await;
                }
            }
        }
        _ => {}
    }
}

/// oFono's `PropertyChanged(s name, v value)` as a one-entry map.
fn single_property(msg: &Message) -> Option<PropMap> {
    let (name, value) = msg.body().deserialize::<(String, OwnedValue)>().ok()?;
    Some(HashMap::from([(name, value)]))
}

async fn on_gateway_added(
    inner: &Inner,
    svc: &TelephonyService,
    conn: &Connection,
    gw: &str,
    props: &PropMap,
) {
    if !props::modem_has_voice(props) {
        return;
    }
    let Some(address) = inner.gateway_address(conn, svc, gw).await else {
        tracing::debug!("new gateway {gw} without a phone address");
        return;
    };
    inner.events.emit(PhoneEvent::CallsAvailable { address });
}

fn on_gateway_removed(inner: &Inner, gw: &str) {
    let (gateway, calls) = inner.with_state(|s| {
        s.volumes.remove(gw);
        let calls = take_calls(&mut s.calls, |call_path| {
            props::gateway_of_call(call_path) == Some(gw)
        });
        (s.gateways.remove(gw), calls)
    });
    for id in calls {
        inner.events.emit(PhoneEvent::CallRemoved { id });
    }
    if let Some(g) = gateway {
        inner
            .events
            .emit(PhoneEvent::CallsUnavailable { address: g.address });
    }
}

async fn on_call_added(
    inner: &Inner,
    svc: &TelephonyService,
    conn: &Connection,
    gw: &str,
    path: &str,
    props: &PropMap,
) {
    let device = inner.gateway_address(conn, svc, gw).await;
    let call = inner.with_state(|s| {
        let call = props::call_from_props(path, props, device, s.calls.get(path))?;
        s.calls.insert(path.to_owned(), call.clone());
        Some(call)
    });
    if let Some(call) = call {
        inner.events.emit(PhoneEvent::CallAdded { call });
    }
}

fn on_call_removed(inner: &Inner, path: &str) {
    if inner.with_state(|s| s.calls.remove(path)).is_some() {
        inner.events.emit(PhoneEvent::CallRemoved {
            id: CallId(path.to_owned()),
        });
    }
}

fn on_call_changed(inner: &Inner, path: &str, changed: &PropMap) {
    let call = inner.with_state(|s| {
        let call = s.calls.get_mut(path)?;
        props::apply_call_update(call, changed).then(|| call.clone())
    });
    if let Some(call) = call {
        inner.events.emit(PhoneEvent::CallChanged { call });
    }
}

async fn on_volume_changed(
    inner: &Inner,
    svc: &TelephonyService,
    conn: &Connection,
    gw: &str,
    changed: &PropMap,
    percent: bool,
) {
    // Signals carry only the changed half; start from the full current
    // volume so we never report a made-up 0 for the other one.
    let base = match inner.with_state(|s| s.volumes.get(gw).copied()) {
        Some(v) => v,
        None => match inner.read_volume(svc, conn, gw).await {
            Ok(v) => v,
            Err(_) => CallVolume {
                speaker: 0,
                microphone: 0,
            },
        },
    };
    let volume = props::volume_update(base, changed, percent);
    let device = inner.with_state(|s| {
        s.volumes.insert(gw.to_owned(), volume);
        s.gateways.get(gw).map(|g| g.address.clone())
    });
    inner
        .events
        .emit(PhoneEvent::VolumeChanged { device, volume });
}

/// The provider went away: report its calls and phones as gone.
fn forget_service(inner: &Inner, service: &str) {
    let gateways: Vec<String> = inner.with_state(|s| {
        s.gateways
            .iter()
            .filter(|(_, g)| g.service == service)
            .map(|(p, _)| p.clone())
            .collect()
    });
    for gw in gateways {
        on_gateway_removed(inner, &gw);
    }
}

fn take_calls(
    calls: &mut HashMap<String, phonesym_core::Call>,
    pred: impl Fn(&str) -> bool,
) -> Vec<CallId> {
    let ids: Vec<String> = calls.keys().filter(|k| pred(k)).cloned().collect();
    ids.into_iter()
        .filter_map(|k| calls.remove(&k).map(|c| c.id))
        .collect()
}
