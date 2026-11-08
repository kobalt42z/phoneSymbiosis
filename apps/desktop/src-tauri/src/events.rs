//! Forwards every [`PhoneEvent`] to the UI as a `phone-event` and reacts to the
//! important ones: notifications, bringing the window up, remembering the phone.

use crate::state::AppState;
use phonesym_platform::{PhoneEvent, Platform};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast::error::RecvError;

/// Name of the event the frontend listens to.
pub const PHONE_EVENT: &str = "phone-event";

pub async fn forward(app: AppHandle, platform: Arc<Platform>) {
    let mut rx = platform.events.subscribe();
    loop {
        match rx.recv().await {
            Ok(ev) => {
                if let Err(e) = app.emit(PHONE_EVENT, &ev) {
                    tracing::warn!("could not send event to the UI: {e}");
                }
                react(&app, &platform, &ev).await;
            }
            Err(RecvError::Lagged(n)) => tracing::warn!("UI missed {n} phone events"),
            Err(RecvError::Closed) => break,
        }
    }
}

/// Side effects for a few events (everything else is handled by the UI).
async fn react(app: &AppHandle, platform: &Arc<Platform>, ev: &PhoneEvent) {
    match ev {
        PhoneEvent::CallAdded { call } if call.state.is_ringing() => {
            let who = call.name.clone().unwrap_or_else(|| call.number.clone());
            notify(app, "Incoming call", &who);
            crate::show_main_window(app);
        }
        PhoneEvent::MessageReceived { message } => {
            let who = message
                .peer_name
                .clone()
                .unwrap_or_else(|| message.peer_number.clone());
            notify(app, &format!("SMS from {who}"), &message.text);
        }
        PhoneEvent::DeviceConnected { address } | PhoneEvent::CallsAvailable { address } => {
            app.state::<AppState>().remember_phone(app, address).await;
            // Ask the phone to push new SMS. Failure is fine (not every phone allows it).
            let (p, a) = (platform.clone(), address.clone());
            tauri::async_runtime::spawn(async move {
                if let Err(e) = p.messages.watch(&a).await {
                    tracing::debug!("SMS notifications unavailable for {a}: {e}");
                }
            });
        }
        PhoneEvent::PairingConfirm { .. } => crate::show_main_window(app),
        _ => {}
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!("notification failed: {e}");
    }
}
