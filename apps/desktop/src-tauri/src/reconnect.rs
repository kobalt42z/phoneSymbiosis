//! Auto-reconnect: every few seconds, if the last phone is not connected, try
//! to connect to it quietly. Phones often drop the link when they go out of
//! range; this brings it back without the user doing anything.

use crate::state::AppState;
use phonesym_platform::Address;
use std::time::Duration;
use tauri::{AppHandle, Manager};

const INTERVAL: Duration = Duration::from_secs(10);

pub async fn run(app: AppHandle) {
    loop {
        tokio::time::sleep(INTERVAL).await;
        try_once(&app).await;
    }
}

async fn try_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (enabled, last) = {
        let s = state.settings.lock().await;
        (s.auto_reconnect, s.last_phone.clone())
    };
    let (true, Some(last)) = (enabled, last) else {
        return;
    };
    let Some(platform) = state.try_platform().await else {
        return;
    };
    let Ok(address) = Address::parse(&last) else {
        return;
    };

    // Already connected, or the backend can't tell us: nothing to do.
    match platform.bluetooth.device(&address).await {
        Ok(d) if d.connected => return,
        Ok(_) => {}
        Err(_) => return,
    }
    match platform.bluetooth.connect(&address).await {
        Ok(()) => tracing::info!("reconnected to {address}"),
        Err(e) => tracing::debug!("reconnect to {address} failed: {e}"),
    }
}
