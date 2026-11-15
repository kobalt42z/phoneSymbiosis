//! # Windows backend
//!
//! | Area                 | Status on Windows                                          |
//! |----------------------|------------------------------------------------------------|
//! | Bluetooth            | adapter info, on/off, scan, pair, unpair (WinRT)           |
//! | Connect/disconnect   | handled by Windows itself once a phone is paired           |
//! | Calls (HFP)          | not supported: Windows does not let apps act as a hands-free kit |
//! | Call audio           | not supported: SCO audio is owned by the Windows driver    |
//! | Contacts / history   | not supported yet (PBAP needs a custom OBEX client)        |
//! | SMS                  | not supported yet (MAP needs a custom OBEX client)         |
//!
//! Everything that is not supported returns [`Error::NotSupported`] with a
//! sentence the UI can show, so the app runs and explains what is missing.

#![cfg_attr(not(target_os = "windows"), allow(dead_code, unused_imports))]

#[cfg(target_os = "windows")]
mod bluetooth;
mod unsupported;

use phonesym_core::{EventBus, Platform, PlatformConfig, Result};
use std::sync::Arc;

pub const NAME: &str = "windows-winrt";

/// Build the [`Platform`] for Windows.
#[cfg(target_os = "windows")]
pub async fn init(config: PlatformConfig) -> Result<Platform> {
    let _ = config;
    let events = EventBus::new();
    let bluetooth = Arc::new(bluetooth::WinBluetooth::new(events.clone()).await?);
    let rest = Arc::new(unsupported::Unsupported);
    Ok(Platform {
        name: NAME,
        bluetooth,
        calls: rest.clone(),
        audio: rest.clone(),
        contacts: rest.clone(),
        messages: rest,
        events,
    })
}

#[cfg(not(target_os = "windows"))]
pub async fn init(_config: PlatformConfig) -> Result<Platform> {
    Err(phonesym_core::Error::NotSupported(
        "the Windows backend only runs on Windows".into(),
    ))
}
