//! Bluetooth: adapter, scanning, pairing, connecting.

use super::address;
use crate::errors::{friendly, CmdResult};
use crate::state::AppState;
use phonesym_platform::{AdapterInfo, Device};
use std::time::Duration;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn adapter(state: State<'_, AppState>) -> CmdResult<AdapterInfo> {
    state
        .platform()
        .await?
        .bluetooth
        .adapter()
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn set_powered(state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    state
        .platform()
        .await?
        .bluetooth
        .set_powered(on)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn devices(state: State<'_, AppState>) -> CmdResult<Vec<Device>> {
    state
        .platform()
        .await?
        .bluetooth
        .devices()
        .await
        .map_err(friendly)
}

/// Scan for `seconds` (default 8, max 60), then return every known device.
#[tauri::command]
pub async fn scan(state: State<'_, AppState>, seconds: Option<u64>) -> CmdResult<Vec<Device>> {
    let secs = seconds.unwrap_or(8).clamp(1, 60);
    state
        .platform()
        .await?
        .bluetooth
        .scan(Duration::from_secs(secs))
        .await
        .map_err(friendly)
}

/// Pair with a phone. A `PairingConfirm` event may arrive while this runs.
#[tauri::command]
pub async fn pair(state: State<'_, AppState>, address: String) -> CmdResult<()> {
    let a = self::address(&address)?;
    state
        .platform()
        .await?
        .bluetooth
        .pair(&a)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn confirm_pairing(
    state: State<'_, AppState>,
    address: String,
    accept: bool,
) -> CmdResult<()> {
    let a = self::address(&address)?;
    state
        .platform()
        .await?
        .bluetooth
        .confirm_pairing(&a, accept)
        .await
        .map_err(friendly)
}

/// Connect and remember the phone for auto-reconnect.
#[tauri::command]
pub async fn connect(app: AppHandle, state: State<'_, AppState>, address: String) -> CmdResult<()> {
    let a = self::address(&address)?;
    state
        .platform()
        .await?
        .bluetooth
        .connect(&a)
        .await
        .map_err(friendly)?;
    state.remember_phone(&app, &a).await;
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, address: String) -> CmdResult<()> {
    let a = self::address(&address)?;
    state
        .platform()
        .await?
        .bluetooth
        .disconnect(&a)
        .await
        .map_err(friendly)
}

/// Remove the pairing. Also stops auto-reconnect to that phone.
#[tauri::command]
pub async fn forget(app: AppHandle, state: State<'_, AppState>, address: String) -> CmdResult<()> {
    let a = self::address(&address)?;
    state
        .platform()
        .await?
        .bluetooth
        .forget(&a)
        .await
        .map_err(friendly)?;
    let mut s = state.settings.lock().await;
    if s.last_phone.as_deref() == Some(a.as_str()) {
        s.last_phone = None;
        s.save(&app)?;
    }
    Ok(())
}
