//! Hands-free call control (HFP, PC = hands-free unit, phone = audio gateway).
//!
//! On Ubuntu, PipeWire's native Bluetooth backend implements HFP and exposes
//! call control on D-Bus (see `spa/plugins/bluez5/README-Telephony.md` in
//! the PipeWire sources). oFono is supported as a fallback. [`service`]
//! explains the providers; this file implements [`CallControl`] on top.
//!
//! Object layout (PipeWire):
//! ```text
//! /org/pipewire/Telephony            manager: org.ofono.Manager.GetModems()
//! /org/pipewire/Telephony/ag0        one per connected phone ("audio gateway")
//! /org/pipewire/Telephony/ag0/call0  one per call
//! ```
//!
//! We use the `org.ofono.*` interfaces for discovery (every provider has
//! them) and PipeWire's own interfaces for actions when it is the provider.
//! A background task ([`watch`]) keeps a small cache of phones and calls and
//! publishes [`PhoneEvent`]s.

pub mod props;
pub mod service;
mod watch;

use crate::bluetooth::props::address_from_path;
use crate::dbus_util::{self, map_err, PropMap};
use async_trait::async_trait;
use phonesym_core::*;
use service::{
    Buses, TelephonyService, OFONO_CALL_VOLUME, OFONO_MANAGER, OFONO_VOICE_CALL_MANAGER,
    PW_AG_TRANSPORT, PW_AUDIO_GATEWAY,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::zvariant::{ObjectPath, Value};
use zbus::Connection;

/// A connected phone as seen by the telephony service.
#[derive(Debug, Clone)]
struct Gateway {
    /// Bus name of the provider, so we can forget its phones if it exits.
    service: &'static str,
    address: Address,
}

/// What we remember between requests and signals.
#[derive(Default)]
struct State {
    /// Gateway object path -> phone.
    gateways: HashMap<String, Gateway>,
    /// Call object path -> call. Needed because change signals only carry
    /// the changed property, and to keep the inferred call direction.
    calls: HashMap<String, Call>,
    /// Gateway object path -> last known volume.
    volumes: HashMap<String, CallVolume>,
}

/// Shared between the [`Telephony`] handle and its watcher tasks.
struct Inner {
    buses: Buses,
    state: Mutex<State>,
    events: EventBus,
}

pub struct Telephony {
    inner: Arc<Inner>,
}

impl Telephony {
    pub async fn new(_config: &PlatformConfig, events: EventBus) -> Result<Self> {
        let inner = Arc::new(Inner {
            buses: Buses::default(),
            state: Mutex::new(State::default()),
            events,
        });
        for svc in service::CANDIDATES {
            tokio::spawn(watch::run(inner.clone(), svc));
        }
        Ok(Self { inner })
    }
}


impl Inner {
    fn with_state<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        // A poisoned lock only means another task panicked mid-update; the
        // cache is still usable, so carry on with it.
        let mut guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut guard)
    }

    /// Phone address of a gateway object. PipeWire has an `Address`
    /// property; oFono HFP modems have the BlueZ device path in their path.
    async fn gateway_address(
        &self,
        conn: &Connection,
        svc: &TelephonyService,
        path: &str,
    ) -> Option<Address> {
        // PipeWire reuses `ag0` for the next phone, so ask it every time; the
        // cache is only a shortcut for oFono, whose paths contain the address.
        if !svc.native {
            if let Some(gw) = self.with_state(|s| s.gateways.get(path).cloned()) {
                return Some(gw.address);
            }
        }
        let from_prop = dbus_util::get_property(conn, svc.name, path, PW_AUDIO_GATEWAY, "Address")
            .await
            .ok()
            .and_then(|v| {
                v.downcast_ref::<&str>()
                    .ok()
                    .and_then(|s| Address::parse(s).ok())
            });
        let address = from_prop.or_else(|| address_from_path(path))?;
        self.with_state(|s| {
            s.gateways.insert(
                path.to_owned(),
                Gateway {
                    service: svc.name,
                    address: address.clone(),
                },
            )
        });
        Some(address)
    }

    /// All connected phones of one provider: (gateway path, address).
    async fn gateways_of(
        &self,
        svc: &TelephonyService,
        conn: &Connection,
    ) -> Result<Vec<(String, Address)>> {
        let modems = dbus_util::call_object_list(
            conn,
            svc.name,
            svc.manager_path,
            OFONO_MANAGER,
            "GetModems",
        )
        .await
        .map_err(|e| map_err("telephony GetModems", e))?;
        let mut out = Vec::new();
        for (path, props) in modems {
            if !props::modem_has_voice(&props) {
                continue;
            }
            match self.gateway_address(conn, svc, path.as_str()).await {
                Some(addr) => out.push((path.to_string(), addr)),
                None => tracing::debug!("skipping {path}: unknown phone address"),
            }
        }
        Ok(out)
    }

    /// The running provider and its phones.
    async fn gateways(&self) -> Result<(TelephonyService, Connection, Vec<(String, Address)>)> {
        let (svc, conn) = self.buses.resolve().await?;
        let gws = self.gateways_of(&svc, &conn).await?;
        Ok((svc, conn, gws))
    }

    /// The gateway for `phone`, or the first one.
    async fn pick_gateway(
        &self,
        phone: Option<&Address>,
    ) -> Result<(TelephonyService, Connection, String, Address)> {
        let (svc, conn, gws) = self.gateways().await?;
        let found = match phone {
            Some(want) => gws.into_iter().find(|(_, a)| a == want),
            None => gws.into_iter().next(),
        };
        let (path, address) = found.ok_or_else(|| {
            Error::NotConnected(match phone {
                Some(a) => format!(
                    "{a} has no hands-free link (connect it with `phonesym device connect {a}`)"
                ),
                None => "no phone with a hands-free link is connected".into(),
            })
        })?;
        Ok((svc, conn, path, address))
    }

    /// Current calls of one gateway, refreshing the cache.
    async fn fetch_calls(
        &self,
        svc: &TelephonyService,
        conn: &Connection,
        gw: &str,
        address: &Address,
    ) -> Result<Vec<Call>> {
        let raw = dbus_util::call_object_list(
            conn,
            svc.name,
            gw,
            OFONO_VOICE_CALL_MANAGER,
            "GetCalls",
        )
        .await
        .map_err(|e| map_err("telephony GetCalls", e))?;
        let calls = self.with_state(|s| {
            raw.iter()
                .filter_map(|(path, p)| {
                    let call = props::call_from_props(
                        path.as_str(),
                        p,
                        Some(address.clone()),
                        s.calls.get(path.as_str()),
                    )?;
                    s.calls.insert(path.to_string(), call.clone());
                    Some(call)
                })
                .collect()
        });
        Ok(calls)
    }

    /// Call a no-reply method on the gateway (Dial, SwapCalls, HangupAll...).
    async fn gateway_method<B>(&self, phone: Option<&Address>, method: &str, body: &B) -> Result<()>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let (svc, conn, path, _) = self.pick_gateway(phone).await?;
        dbus_util::call_void(&conn, svc.name, &path, svc.gateway_iface(), method, body)
            .await
            .map_err(|e| map_err(method, e))
    }

    /// Answer / Hangup on a call object.
    async fn call_method(&self, call: &CallId, method: &str) -> Result<()> {
        let path = ObjectPath::try_from(call.0.as_str())
            .map_err(|_| Error::InvalidArgument(format!("not a call id: {call}")))?;
        let (svc, conn) = self.buses.resolve().await?;
        dbus_util::call_void(
            &conn,
            svc.name,
            path.as_str(),
            svc.call_iface(),
            method,
            &(),
        )
        .await
        .map_err(|e| match map_err(method, e) {
            Error::DeviceNotFound(_) => {
                Error::InvalidState(format!("call {call} no longer exists"))
            }
            other => other,
        })
    }

    async fn read_volume(
        &self,
        svc: &TelephonyService,
        conn: &Connection,
        gw: &str,
    ) -> Result<CallVolume> {
        let volume = if svc.native {
            let p = dbus_util::get_all_properties(conn, svc.name, gw, PW_AUDIO_GATEWAY).await;
            props::volume_from_props(&p.map_err(|e| map_err("read volume", e))?, false)
        } else {
            let p: zbus::Result<PropMap> =
                dbus_util::call(conn, svc.name, gw, OFONO_CALL_VOLUME, "GetProperties", &()).await;
            props::volume_from_props(&p.map_err(|e| map_err("read volume", e))?, true)
        };
        self.with_state(|s| s.volumes.insert(gw.to_owned(), volume));
        Ok(volume)
    }

    async fn write_volume(
        &self,
        svc: &TelephonyService,
        conn: &Connection,
        gw: &str,
        name: &str,
        hfp: u8,
    ) -> Result<()> {
        let r = if svc.native {
            dbus_util::set_property(conn, svc.name, gw, PW_AUDIO_GATEWAY, name, Value::U8(hfp))
                .await
        } else {
            let pct = Value::U8(props::hfp_to_percent(hfp));
            dbus_util::call_void(
                conn,
                svc.name,
                gw,
                OFONO_CALL_VOLUME,
                "SetProperty",
                &(name, pct),
            )
            .await
        };
        r.map_err(|e| map_err(&format!("set {name}"), e))
    }
}

#[async_trait]
impl CallControl for Telephony {
    async fn available_phones(&self) -> Result<Vec<Address>> {
        let (_, _, gws) = self.inner.gateways().await?;
        Ok(gws.into_iter().map(|(_, a)| a).collect())
    }

    async fn calls(&self) -> Result<Vec<Call>> {
        let (svc, conn, gws) = self.inner.gateways().await?;
        let mut all = Vec::new();
        for (path, address) in gws {
            all.extend(self.inner.fetch_calls(&svc, &conn, &path, &address).await?);
        }
        Ok(all)
    }

    async fn dial(&self, phone: Option<&Address>, number: &str) -> Result<()> {
        let number = props::normalize_number(number)?;
        let (svc, conn, path, _) = self.inner.pick_gateway(phone).await?;
        let r = if svc.native {
            dbus_util::call_void(
                &conn,
                svc.name,
                &path,
                PW_AUDIO_GATEWAY,
                "Dial",
                &(number.as_str(),),
            )
            .await
        } else {
            // oFono's Dial takes a caller-id mode; "default" = network setting.
            dbus_util::call_void(
                &conn,
                svc.name,
                &path,
                OFONO_VOICE_CALL_MANAGER,
                "Dial",
                &(number.as_str(), "default"),
            )
            .await
        };
        r.map_err(|e| map_err("Dial", e))
    }

    async fn answer(&self, call: &CallId) -> Result<()> {
        self.inner.call_method(call, "Answer").await
    }

    async fn hang_up(&self, call: &CallId) -> Result<()> {
        self.inner.call_method(call, "Hangup").await
    }

    async fn hang_up_all(&self, phone: Option<&Address>) -> Result<()> {
        self.inner.gateway_method(phone, "HangupAll", &()).await
    }

    async fn send_tones(&self, phone: Option<&Address>, tones: &str) -> Result<()> {
        let tones = props::normalize_tones(tones)?;
        self.inner
            .gateway_method(phone, "SendTones", &(tones.as_str(),))
            .await
    }

    async fn swap_calls(&self, phone: Option<&Address>) -> Result<()> {
        self.inner.gateway_method(phone, "SwapCalls", &()).await
    }

    async fn hold_and_answer(&self, phone: Option<&Address>) -> Result<()> {
        self.inner.gateway_method(phone, "HoldAndAnswer", &()).await
    }

    async fn release_and_answer(&self, phone: Option<&Address>) -> Result<()> {
        self.inner
            .gateway_method(phone, "ReleaseAndAnswer", &())
            .await
    }

    async fn merge_calls(&self, phone: Option<&Address>) -> Result<()> {
        self.inner
            .gateway_method(phone, "CreateMultiparty", &())
            .await
    }

    /// PipeWire's telephony service does not expose battery, signal or
    /// operator (as of PipeWire 1.4/1.5), so only the phone is filled in.
    async fn status(&self, phone: Option<&Address>) -> Result<PhoneStatus> {
        let (_, _, _, address) = self.inner.pick_gateway(phone).await?;
        Ok(PhoneStatus {
            device: Some(address),
            ..PhoneStatus::default()
        })
    }

    async fn volume(&self, phone: Option<&Address>) -> Result<CallVolume> {
        let (svc, conn, path, _) = self.inner.pick_gateway(phone).await?;
        self.inner.read_volume(&svc, &conn, &path).await
    }

    async fn set_volume(&self, phone: Option<&Address>, volume: CallVolume) -> Result<()> {
        if volume.speaker > 15 || volume.microphone > 15 {
            return Err(Error::InvalidArgument("volume must be 0-15".into()));
        }
        let (svc, conn, path, _) = self.inner.pick_gateway(phone).await?;
        self.inner
            .write_volume(&svc, &conn, &path, "SpeakerVolume", volume.speaker)
            .await?;
        self.inner
            .write_volume(&svc, &conn, &path, "MicrophoneVolume", volume.microphone)
            .await
    }
}

/// Make sure the phone's audio (SCO) link is up, so PipeWire creates the
/// audio nodes we route. Only PipeWire's native service can do this
/// (`AudioGatewayTransport1.Activate`). Returns `Ok(true)` if we asked for it.
///
/// Normally the phone opens the link itself when a call starts; this helps
/// when the user moved the audio to the phone and back, or for some iPhones
/// that wait for the hands-free unit to ask.
pub async fn activate_audio_link(phone: &Address) -> Result<bool> {
    let svc = service::PIPEWIRE;
    let conn = svc
        .connect()
        .await
        .map_err(|e| map_err("session D-Bus", e))?;
    if !dbus_util::name_has_owner(&conn, svc.name)
        .await
        .unwrap_or(false)
    {
        return Ok(false);
    }
    let inner = Inner {
        buses: Buses::default(),
        state: Mutex::new(State::default()),
        events: EventBus::new(),
    };
    let gws = inner.gateways_of(&svc, &conn).await?;
    let Some((path, _)) = gws.into_iter().find(|(_, a)| a == phone) else {
        return Ok(false);
    };
    let state = dbus_util::get_property(&conn, svc.name, &path, PW_AG_TRANSPORT, "State")
        .await
        .ok()
        .and_then(|v| v.downcast_ref::<&str>().ok().map(str::to_owned));
    if state.as_deref() == Some("active") {
        return Ok(false);
    }
    dbus_util::call_void(&conn, svc.name, &path, PW_AG_TRANSPORT, "Activate", &())
        .await
        .map_err(|e| map_err("activate audio link", e))?;
    Ok(true)
}
