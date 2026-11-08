//! App-level commands: start-up status, settings, health checks.

use crate::errors::CmdResult;
use crate::settings::Settings;
use crate::state::{AppState, AppStatus};
use serde::Serialize;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> CmdResult<AppStatus> {
    Ok(state.status().await)
}

/// Try starting the backend again (from the setup screen).
#[tauri::command]
pub async fn retry_init(app: AppHandle, state: State<'_, AppState>) -> CmdResult<AppStatus> {
    crate::state::start_platform(app).await;
    Ok(state.status().await)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.lock().await.clone())
}

/// Save settings. `auto_audio` is passed to the backend at start-up, so a
/// change applies after the app restarts.
#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let mut s = state.settings.lock().await;
    *s = settings;
    s.save(&app)?;
    Ok(s.clone())
}

/// One line of the "Run checks" report.
#[derive(Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn check(name: &str, r: Result<String, String>) -> Check {
    match r {
        Ok(detail) => Check {
            name: name.into(),
            ok: true,
            detail,
        },
        Err(detail) => Check {
            name: name.into(),
            ok: false,
            detail,
        },
    }
}

/// A quick health report built from the platform APIs.
#[tauri::command]
pub async fn run_checks(state: State<'_, AppState>) -> CmdResult<Vec<Check>> {
    let mut out = Vec::new();
    let p = match state.platform().await {
        Ok(p) => {
            out.push(check("Phone services", Ok(format!("backend {}", p.name))));
            p
        }
        Err(e) => {
            out.push(check("Phone services", Err(e)));
            return Ok(out);
        }
    };
    let fr = crate::errors::friendly;

    out.push(check(
        "Bluetooth adapter",
        p.bluetooth.adapter().await.map_err(fr).and_then(|a| {
            if a.powered {
                Ok(format!("{} ({}) is on", a.name, a.address))
            } else {
                Err(format!("{} ({}) is turned off", a.name, a.address))
            }
        }),
    ));

    let devices = p.bluetooth.devices().await.map_err(fr);
    out.push(check(
        "Paired phones",
        devices.as_ref().map_err(Clone::clone).and_then(|d| {
            let paired: Vec<_> = d
                .iter()
                .filter(|d| d.paired)
                .map(|d| d.name.clone())
                .collect();
            if paired.is_empty() {
                Err("none yet, pair one in Devices".into())
            } else {
                Ok(paired.join(", "))
            }
        }),
    ));
    out.push(check(
        "Connected phone",
        devices.and_then(|d| {
            d.into_iter()
                .find(|d| d.connected)
                .map(|d| format!("{} ({})", d.name, d.address))
                .ok_or_else(|| "no phone connected".into())
        }),
    ));

    out.push(check(
        "Hands-free (calls)",
        p.calls.available_phones().await.map_err(fr).and_then(|v| {
            if v.is_empty() {
                Err("no phone ready for calls".into())
            } else {
                Ok(v.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", "))
            }
        }),
    ));

    out.push(check(
        "Call audio",
        Ok(if p.audio.is_active().await {
            "routed to this PC".into()
        } else {
            "idle".into()
        }),
    ));
    Ok(out)
}
