//! # phonesym-core
//!
//! The shared vocabulary of PhoneSymbiosis. Everything here is plain Rust with
//! no operating-system code, so every platform backend and every front end
//! (CLI, desktop app) speaks the same language.
//!
//! - [`model`]: data types (devices, calls, contacts, messages).
//! - [`traits`]: the interfaces a platform backend must implement.
//! - [`events`]: the live event stream (incoming call, device connected, ...).
//! - [`platform`]: the [`Platform`] bundle a backend hands to the app.
//! - [`vcard`]: a small vCard parser used for phonebook data.
//!
//! To add a new OS, implement the traits in a new crate under
//! `crates/backends/<os>` and register it in `crates/platform`.

pub mod error;
pub mod events;
pub mod model;
pub mod platform;
pub mod traits;
pub mod vcard;

pub use error::{Error, Result};
pub use events::{EventBus, PhoneEvent};
pub use model::*;
pub use platform::{Platform, PlatformConfig};
pub use traits::*;
