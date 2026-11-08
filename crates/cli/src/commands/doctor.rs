//! `phonesym doctor`: check that everything PhoneSymbiosis needs is present,
//! and say how to fix what isn't.
//!
//! Bluetooth and telephony are checked through the platform traits, so they
//! work for any backend. The remaining checks (OBEX service, PipeWire
//! tools) are specific to the Linux backend.
//! TODO: move OS-specific checks behind a `diagnostics()` method on the
//! platform once core has one, so this file has no OS knowledge.

use phonesym_platform::{Error, PlatformConfig};

/// One line of the report.
struct Check {
    ok: bool,
    what: String,
    /// How to fix it (shown only when the check fails).
    hint: &'static str,
}

impl Check {
    fn new(ok: bool, what: impl Into<String>, hint: &'static str) -> Self {
        Self {
            ok,
            what: what.into(),
            hint,
        }
    }

    fn print(&self) {
        if self.ok {
            println!("✓ {}", self.what);
        } else {
            println!("✗ {}\n    → {}", self.what, self.hint);
        }
    }
}

pub async fn run(config: PlatformConfig) -> anyhow::Result<()> {
    let mut checks = Vec::new();

    match phonesym_platform::init(config).await {
        Ok(platform) => {
            checks.push(bluetooth_check(&platform).await);
            checks.push(telephony_check(&platform).await);
        }
        Err(e) => checks.push(Check::new(
            false,
            format!(
                "backend {} failed to start: {e}",
                phonesym_platform::backend_name()
            ),
            "see the error above; run with RUST_LOG=debug for details",
        )),
    }
    checks.extend(os_checks().await);

    for c in &checks {
        c.print();
    }
    let failed = checks.iter().filter(|c| !c.ok).count();
    if failed > 0 {
        anyhow::bail!("{failed} check(s) failed");
    }
    println!("All good. Next: `phonesym device scan`.");
    Ok(())
}

async fn bluetooth_check(platform: &phonesym_platform::Platform) -> Check {
    match platform.bluetooth.adapter().await {
        Ok(a) if a.powered => Check::new(true, format!("Bluetooth adapter {} ({}) is on", a.name, a.address), ""),
        Ok(a) => Check::new(
            false,
            format!("Bluetooth adapter {} ({}) is off", a.name, a.address),
            "run `phonesym device power on` (or `rfkill unblock bluetooth` if it is blocked)",
        ),
        Err(Error::AdapterUnavailable(m)) => {
            Check::new(false, format!("no Bluetooth adapter: {m}"), "plug in / enable a Bluetooth adapter; check `rfkill list`")
        }
        Err(e) => Check::new(
            false,
            format!("BlueZ not reachable: {e}"),
            "install and start BlueZ: `sudo apt install bluez && sudo systemctl enable --now bluetooth`",
        ),
    }
}

async fn telephony_check(platform: &phonesym_platform::Platform) -> Check {
    match platform.calls.available_phones().await {
        Ok(phones) => Check::new(
            true,
            format!("hands-free telephony service running ({} phone(s) with a call link)", phones.len()),
            "",
        ),
        Err(e) => Check::new(
            false,
            format!("hands-free telephony service missing: {e}"),
            "use PipeWire >= 1.4 with its native Bluetooth HFP backend (Ubuntu default): \
             `sudo apt install pipewire wireplumber libspa-0.2-bluetooth`, then `systemctl --user restart pipewire wireplumber`",
        ),
    }
}

#[cfg(target_os = "linux")]
async fn os_checks() -> Vec<Check> {
    vec![
        Check::new(
            session_bus_has("org.bluez.obex").await,
            "BlueZ OBEX service (org.bluez.obex) available for contacts and SMS",
            "`sudo apt install bluez-obexd` (it starts automatically on the session bus)",
        ),
        Check::new(
            in_path("pw-dump"),
            "pw-dump found (to find call audio)",
            "`sudo apt install pipewire-bin`",
        ),
        Check::new(
            in_path("pw-loopback"),
            "pw-loopback found (to route call audio)",
            "`sudo apt install pipewire-bin`",
        ),
    ]
}

#[cfg(not(target_os = "linux"))]
async fn os_checks() -> Vec<Check> {
    Vec::new()
}

/// Is `name` running or D-Bus-activatable on the session bus? obexd is
/// normally started on demand, so "activatable" counts as present.
/// Uses `busctl` (part of systemd) to keep the CLI free of D-Bus code.
#[cfg(target_os = "linux")]
async fn session_bus_has(name: &str) -> bool {
    for method in ["ListNames", "ListActivatableNames"] {
        let out = tokio::process::Command::new("busctl")
            .args([
                "--user",
                "call",
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                method,
            ])
            .output()
            .await;
        if let Ok(out) = out {
            if String::from_utf8_lossy(&out.stdout).contains(&format!("\"{name}\"")) {
                return true;
            }
        }
    }
    false
}

/// Is an executable called `program` somewhere in `$PATH`?
fn in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}
