//! Shared app state and platform start-up.
//!
//! The backend may fail to start (no Bluetooth, missing services, ...). The app
//! keeps running anyway and the UI shows a setup screen with the error.

use crate::errors::{friendly, CmdResult};
use crate::settings::Settings;
use phonesym_platform::{Address, Platform, PlatformConfig};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    platform: RwLock<Option<Arc<Platform>>>,
    /// Why the platform failed to start, if it did.
    init_error: RwLock<Option<String>>,
    pub settings: Mutex<Settings>,
}

/// What the UI needs to know to pick between the main screens and the setup screen.
#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    /// `true` once the backend started successfully.
    pub ready: bool,
    /// `true` while the backend is still starting.
    pub starting: bool,
    pub error: Option<String>,
    pub backend: &'static str,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            platform: RwLock::new(None),
            init_error: RwLock::new(None),
            settings: Mutex::new(settings),
        }
    }

    /// The running platform, or a friendly "not ready" error.
    pub async fn platform(&self) -> CmdResult<Arc<Platform>> {
        if let Some(p) = self.platform.read().await.clone() {
            return Ok(p);
        }
        Err(match self.init_error.read().await.clone() {
            Some(e) => format!("Phone services failed to start: {e}"),
            None => "Phone services are still starting, try again in a moment.".into(),
        })
    }

    /// The platform if it is running, without an error message.
    pub async fn try_platform(&self) -> Option<Arc<Platform>> {
        self.platform.read().await.clone()
    }

    pub async fn status(&self) -> AppStatus {
        let ready = self.platform.read().await.is_some();
        let error = self.init_error.read().await.clone();
        AppStatus {
            ready,
            starting: !ready && error.is_none(),
            error,
            backend: phonesym_platform::backend_name(),
        }
    }

    /// Pick the phone a command should act on: the given address, else a phone
    /// with calls available, else a connected device, else the last used phone.
    pub async fn resolve_phone(&self, address: Option<String>) -> CmdResult<Address> {
        if let Some(a) = address.filter(|a| !a.is_empty()) {
            return Address::parse(&a).map_err(friendly);
        }
        let p = self.platform().await?;
        if let Ok(phones) = p.calls.available_phones().await {
            if let Some(a) = phones.into_iter().next() {
                return Ok(a);
            }
        }
        if let Ok(devs) = p.bluetooth.devices().await {
            if let Some(d) = devs.into_iter().find(|d| d.connected) {
                return Ok(d.address);
            }
        }
        if let Some(a) = self.settings.lock().await.last_phone.clone() {
            return Address::parse(&a).map_err(friendly);
        }
        Err("No phone connected. Pair and connect one in Devices.".into())
    }

    /// Remember `address` as the phone to reconnect to.
    pub async fn remember_phone(&self, app: &AppHandle, address: &Address) {
        let mut s = self.settings.lock().await;
        if s.last_phone.as_deref() != Some(address.as_str()) {
            s.last_phone = Some(address.to_string());
            if let Err(e) = s.save(app) {
                tracing::warn!("could not save settings: {e}");
            }
        }
    }
}

/// Start the backend. On success, start forwarding events; either way tell the UI.
/// Safe to call again (from "Retry" on the setup screen) after a failure.
pub async fn start_platform(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.try_platform().await.is_some() {
        return;
    }
    *state.init_error.write().await = None;
    let _ = app.emit("app-status", state.status().await);

    let auto_audio = state.settings.lock().await.auto_audio;
    let config = PlatformConfig {
        auto_audio,
        ..Default::default()
    };
    match phonesym_platform::init(config).await {
        Ok(platform) => {
            tracing::info!("backend {} started", platform.name);
            let platform = Arc::new(platform);
            *state.platform.write().await = Some(platform.clone());
            tauri::async_runtime::spawn(crate::events::forward(app.clone(), platform));
        }
        Err(e) => {
            tracing::warn!("backend failed to start: {e}");
            *state.init_error.write().await = Some(friendly(e));
        }
    }
    let _ = app.emit("app-status", state.status().await);
}
