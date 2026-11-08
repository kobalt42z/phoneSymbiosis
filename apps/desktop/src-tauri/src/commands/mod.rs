//! `#[tauri::command]` wrappers, one file per feature area.
//!
//! Every command returns `Result<T, String>`; the string is a friendly message
//! (see `crate::errors::friendly`). Argument names arrive from JavaScript in
//! camelCase (Tauri converts them), e.g. `call_id` is `callId` in `api.ts`.

pub mod app;
pub mod calls;
pub mod contacts;
pub mod devices;
pub mod messages;

use crate::errors::{friendly, CmdResult};
use phonesym_platform::Address;

/// Parse an optional address coming from the UI ("" means none).
pub(crate) fn opt_address(a: Option<String>) -> CmdResult<Option<Address>> {
    match a.filter(|s| !s.is_empty()) {
        Some(s) => Address::parse(&s).map(Some).map_err(friendly),
        None => Ok(None),
    }
}

pub(crate) fn address(a: &str) -> CmdResult<Address> {
    Address::parse(a).map_err(friendly)
}
