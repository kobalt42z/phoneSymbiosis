//! Calls (hands-free) and call audio.
//!
//! `phone` arguments are optional: empty/None means "the first available phone".

use super::opt_address;
use crate::errors::{friendly, CmdResult};
use crate::state::AppState;
use phonesym_platform::{Call, CallId, CallVolume, PhoneStatus};
use tauri::State;

#[tauri::command]
pub async fn calls(state: State<'_, AppState>) -> CmdResult<Vec<Call>> {
    state
        .platform()
        .await?
        .calls
        .calls()
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn dial(
    state: State<'_, AppState>,
    phone: Option<String>,
    number: String,
) -> CmdResult<()> {
    let number = number.trim();
    if number.is_empty() {
        return Err("Enter a number to call.".into());
    }
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .dial(phone.as_ref(), number)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn answer(state: State<'_, AppState>, call_id: String) -> CmdResult<()> {
    state
        .platform()
        .await?
        .calls
        .answer(&CallId(call_id))
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn hang_up(state: State<'_, AppState>, call_id: String) -> CmdResult<()> {
    state
        .platform()
        .await?
        .calls
        .hang_up(&CallId(call_id))
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn hang_up_all(state: State<'_, AppState>, phone: Option<String>) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .hang_up_all(phone.as_ref())
        .await
        .map_err(friendly)
}

/// Send DTMF digits (0-9, *, #) during a call.
#[tauri::command]
pub async fn send_tones(
    state: State<'_, AppState>,
    phone: Option<String>,
    tones: String,
) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .send_tones(phone.as_ref(), &tones)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn swap_calls(state: State<'_, AppState>, phone: Option<String>) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .swap_calls(phone.as_ref())
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn hold_and_answer(state: State<'_, AppState>, phone: Option<String>) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .hold_and_answer(phone.as_ref())
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn release_and_answer(
    state: State<'_, AppState>,
    phone: Option<String>,
) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .release_and_answer(phone.as_ref())
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn merge_calls(state: State<'_, AppState>, phone: Option<String>) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .merge_calls(phone.as_ref())
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn phone_status(
    state: State<'_, AppState>,
    phone: Option<String>,
) -> CmdResult<PhoneStatus> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .status(phone.as_ref())
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn volume(state: State<'_, AppState>, phone: Option<String>) -> CmdResult<CallVolume> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .volume(phone.as_ref())
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn set_volume(
    state: State<'_, AppState>,
    phone: Option<String>,
    volume: CallVolume,
) -> CmdResult<()> {
    let phone = opt_address(phone)?;
    state
        .platform()
        .await?
        .calls
        .set_volume(phone.as_ref(), volume)
        .await
        .map_err(friendly)
}

/// Route call audio through the PC manually (normally automatic).
#[tauri::command]
pub async fn audio_start(state: State<'_, AppState>, phone: Option<String>) -> CmdResult<()> {
    let phone = state.resolve_phone(phone).await?;
    state
        .platform()
        .await?
        .audio
        .start(&phone)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn audio_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state.platform().await?.audio.stop().await.map_err(friendly)
}

#[tauri::command]
pub async fn audio_active(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.platform().await?.audio.is_active().await)
}

#[tauri::command]
pub async fn set_mic_muted(state: State<'_, AppState>, muted: bool) -> CmdResult<()> {
    state
        .platform()
        .await?
        .audio
        .set_mic_muted(muted)
        .await
        .map_err(friendly)
}
