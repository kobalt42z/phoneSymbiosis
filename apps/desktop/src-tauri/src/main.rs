//! PhoneSymbiosis desktop app.
//!
//! Layout of this crate:
//! - `state`     : the shared [`state::AppState`] (platform handle + settings).
//! - `settings`  : the small JSON settings file in the app config dir.
//! - `errors`    : turns core errors into friendly strings for the UI.
//! - `commands/` : `#[tauri::command]` wrappers, one file per feature area.
//! - `events`    : forwards phone events to the UI, shows notifications.
//! - `tray`      : system tray icon and menu.
//! - `reconnect` : background loop that reconnects the last phone.
//!
//! The app only talks to `phonesym_platform`, which picks the OS backend.

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod errors;
mod events;
mod reconnect;
mod settings;
mod state;
mod tray;

use tauri::{Manager, WindowEvent};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,phonesym=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = settings::Settings::load(&handle);
            app.manage(state::AppState::new(settings));
            tray::create(&handle)?;
            // Start the backend in the background so the window shows immediately.
            tauri::async_runtime::spawn(state::start_platform(handle.clone()));
            tauri::async_runtime::spawn(reconnect::run(handle));
            Ok(())
        })
        // Close-to-tray: closing the window only hides it. "Quit" in the tray exits.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            // app / settings
            commands::app::app_status,
            commands::app::retry_init,
            commands::app::get_settings,
            commands::app::set_settings,
            commands::app::run_checks,
            // devices
            commands::devices::adapter,
            commands::devices::set_powered,
            commands::devices::devices,
            commands::devices::scan,
            commands::devices::pair,
            commands::devices::confirm_pairing,
            commands::devices::connect,
            commands::devices::disconnect,
            commands::devices::forget,
            // calls
            commands::calls::calls,
            commands::calls::dial,
            commands::calls::answer,
            commands::calls::hang_up,
            commands::calls::hang_up_all,
            commands::calls::send_tones,
            commands::calls::swap_calls,
            commands::calls::hold_and_answer,
            commands::calls::release_and_answer,
            commands::calls::merge_calls,
            commands::calls::phone_status,
            commands::calls::volume,
            commands::calls::set_volume,
            commands::calls::audio_start,
            commands::calls::audio_stop,
            commands::calls::audio_active,
            commands::calls::set_mic_muted,
            // contacts
            commands::contacts::contacts,
            commands::contacts::call_history,
            // messages
            commands::messages::list_messages,
            commands::messages::get_message,
            commands::messages::send_message,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the PhoneSymbiosis app")
        .run(|_app, _event| {});
}

/// Show the main window and bring it to the front.
pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
