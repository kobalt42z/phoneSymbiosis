//! The app's settings file: `<config dir>/settings.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Address of the phone we were last connected to.
    pub last_phone: Option<String>,
    /// Try to reconnect to `last_phone` every few seconds when disconnected.
    pub auto_reconnect: bool,
    /// Route call audio to the PC automatically when a call is active.
    pub auto_audio: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_phone: None,
            auto_reconnect: true,
            auto_audio: true,
        }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

impl Settings {
    /// Read the settings file; a missing or broken file gives the defaults.
    pub fn load(app: &AppHandle) -> Self {
        path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let p = path(app).ok_or("no config directory")?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(p, json).map_err(|e| e.to_string())
    }
}
