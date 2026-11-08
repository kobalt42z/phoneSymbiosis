//! System tray icon: Show/Hide, Answer, Hang up, Quit.

use crate::state::AppState;
use phonesym_platform::CallState;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>)?;
    let answer = MenuItem::with_id(app, "answer", "Answer", true, None::<&str>)?;
    let hang_up = MenuItem::with_id(app, "hang_up", "Hang up", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &answer, &hang_up, &sep, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("PhoneSymbiosis")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_window(app),
            "answer" => spawn_call_action(app, CallAction::Answer),
            "hang_up" => spawn_call_action(app, CallAction::HangUp),
            "quit" => app.exit(0),
            _ => {}
        })
        // Left click toggles the window (not reported on every Linux desktop).
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            crate::show_main_window(app);
        }
    }
}

enum CallAction {
    Answer,
    HangUp,
}

fn spawn_call_action(app: &AppHandle, action: CallAction) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(p) = app.state::<AppState>().try_platform().await else {
            return;
        };
        let result = match action {
            CallAction::Answer => match p.calls.calls().await {
                Ok(calls) => match calls.into_iter().find(|c| c.state == CallState::Incoming) {
                    Some(c) => p.calls.answer(&c.id).await,
                    None => Ok(()),
                },
                Err(e) => Err(e),
            },
            CallAction::HangUp => p.calls.hang_up_all(None).await,
        };
        if let Err(e) = result {
            tracing::warn!("tray call action failed: {e}");
        }
    });
}
