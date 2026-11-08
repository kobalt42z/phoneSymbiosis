//! Contacts and call history (read-only, downloaded from the phone).

use crate::errors::{friendly, CmdResult};
use crate::state::AppState;
use phonesym_platform::{CallHistoryEntry, CallHistoryKind, Contact};
use tauri::State;

#[tauri::command]
pub async fn contacts(
    state: State<'_, AppState>,
    phone: Option<String>,
) -> CmdResult<Vec<Contact>> {
    let phone = state.resolve_phone(phone).await?;
    state
        .platform()
        .await?
        .contacts
        .contacts(&phone)
        .await
        .map_err(friendly)
}

/// Call log; `kind = None` returns all entries.
#[tauri::command]
pub async fn call_history(
    state: State<'_, AppState>,
    phone: Option<String>,
    kind: Option<CallHistoryKind>,
) -> CmdResult<Vec<CallHistoryEntry>> {
    let phone = state.resolve_phone(phone).await?;
    state
        .platform()
        .await?
        .contacts
        .call_history(&phone, kind)
        .await
        .map_err(friendly)
}
