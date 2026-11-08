//! SMS: list folders, read one message, send.

use crate::errors::{friendly, CmdResult};
use crate::state::AppState;
use phonesym_platform::{Message, MessageFolder};
use tauri::State;

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    phone: Option<String>,
    folder: MessageFolder,
    limit: Option<u16>,
) -> CmdResult<Vec<Message>> {
    let phone = state.resolve_phone(phone).await?;
    let limit = limit.unwrap_or(200);
    state
        .platform()
        .await?
        .messages
        .list(&phone, folder, limit)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn get_message(
    state: State<'_, AppState>,
    phone: Option<String>,
    id: String,
) -> CmdResult<Message> {
    let phone = state.resolve_phone(phone).await?;
    state
        .platform()
        .await?
        .messages
        .get(&phone, &id)
        .await
        .map_err(friendly)
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    phone: Option<String>,
    to: String,
    text: String,
) -> CmdResult<()> {
    let (to, text) = (to.trim(), text.trim());
    if to.is_empty() || text.is_empty() {
        return Err("Enter a number and a message.".into());
    }
    let phone = state.resolve_phone(phone).await?;
    state
        .platform()
        .await?
        .messages
        .send(&phone, to, text)
        .await
        .map_err(friendly)
}
