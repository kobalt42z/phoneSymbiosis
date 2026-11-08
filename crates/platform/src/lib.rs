//! # phonesym-platform
//!
//! The single place that decides which backend gets compiled in. Front ends
//! call [`init`] and receive a [`Platform`] made of trait objects, so they
//! never contain OS-specific code.
//!
//! Adding an OS:
//! 1. Create `crates/backends/<os>` implementing the traits in `phonesym-core`
//!    and exposing `pub async fn init(PlatformConfig) -> Result<Platform>`.
//! 2. Add it under a `[target.'cfg(target_os = "<os>")'.dependencies]` block in
//!    this crate's `Cargo.toml`.
//! 3. Add a matching `#[cfg]` branch below.

pub use phonesym_core::*;

/// Start the backend for the current OS.
pub async fn init(config: PlatformConfig) -> Result<Platform> {
    #[cfg(target_os = "linux")]
    {
        phonesym_backend_linux::init(config).await
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(Error::NotSupported(format!(
            "no backend for {} yet (add one in crates/backends/)",
            std::env::consts::OS
        )))
    }
}

/// Name of the backend compiled into this build.
pub fn backend_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        phonesym_backend_linux::NAME
    }
    #[cfg(not(target_os = "linux"))]
    {
        "none"
    }
}
