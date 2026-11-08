# Architecture

PhoneSymbiosis makes your PC act like a car's hands-free kit for your phone.
It is split into small layers so that each one can be understood (and replaced)
on its own.

```
┌──────────────────────────────┐   ┌─────────────────────┐
│ apps/desktop  (Tauri+Svelte) │   │ crates/cli (phonesym)│   front ends
└──────────────┬───────────────┘   └──────────┬──────────┘
               │   depend only on             │
               ▼                              ▼
        ┌──────────────────────────────────────────┐
        │ crates/platform   picks the backend for  │   build-time switch
        │                   the OS being compiled  │
        └──────────────────────┬───────────────────┘
                               │ cfg(target_os = "linux")
                               ▼
        ┌──────────────────────────────────────────┐
        │ crates/backends/linux                    │   OS-specific code
        │   bluetooth.rs  → BlueZ                  │
        │   telephony.rs  → PipeWire telephony     │
        │   audio.rs      → PipeWire nodes         │
        │   contacts.rs   → BlueZ OBEX (PBAP)      │
        │   messages.rs   → BlueZ OBEX (MAP)       │
        └──────────────────────┬───────────────────┘
                               │ implements
                               ▼
        ┌──────────────────────────────────────────┐
        │ crates/core   types, traits, events,     │   no OS code
        │               vCard parser               │
        └──────────────────────────────────────────┘
```

## crates/core

The shared vocabulary. Front ends and backends both depend on it.

| File          | What it holds |
|---------------|---------------|
| `model.rs`    | `Device`, `Call`, `Contact`, `Message`, ... (serialisable, sent to the UI as JSON) |
| `traits.rs`   | `BluetoothManager`, `CallControl`, `AudioRouter`, `ContactsProvider`, `MessagesProvider` |
| `events.rs`   | `PhoneEvent` (incoming call, SMS, device connected...) and the `EventBus` |
| `platform.rs` | `Platform`: one struct holding one implementation of each trait |
| `vcard.rs`    | vCard parser shared by every backend that downloads phonebooks |

## crates/platform

The only crate front ends import. It re-exports `phonesym-core` and has one
`init()` function that starts the backend for the current OS. Which backend is
compiled in is decided by `[target.'cfg(...)'.dependencies]` in its
`Cargo.toml`, so a Linux build never contains Windows code and vice versa.

## crates/backends/linux

Talks to standard Ubuntu services over D-Bus (pure Rust via `zbus`, no C
libraries to install):

| Feature | Bluetooth profile | Linux service |
|---------|-------------------|---------------|
| Find, pair, connect | — | BlueZ `org.bluez` (system bus) |
| Calls | HFP (Hands-Free) | PipeWire `org.pipewire.Telephony` (session bus), falls back to oFono `org.ofono` |
| Call audio | HFP SCO audio | PipeWire nodes, linked with `pw-loopback` when needed |
| Contacts, call history | PBAP | BlueZ OBEX `org.bluez.obex` (session bus) |
| SMS | MAP | BlueZ OBEX `org.bluez.obex` (session bus) |

## Adding a new OS (example: Windows)

1. `cargo new --lib crates/backends/windows` and name it `phonesym-backend-windows`.
2. Implement the five traits from `phonesym-core` (start with stubs returning
   `Error::NotSupported`, like the Linux backend did at first).
3. Export `pub const NAME` and `pub async fn init(PlatformConfig) -> Result<Platform>`.
4. Add it to the workspace `members` in the root `Cargo.toml`.
5. In `crates/platform/Cargo.toml` add:
   ```toml
   [target.'cfg(target_os = "windows")'.dependencies]
   phonesym-backend-windows = { path = "../backends/windows" }
   ```
6. In `crates/platform/src/lib.rs` add a `#[cfg(target_os = "windows")]` branch.

Nothing in `apps/` or `crates/cli` changes. Building on Windows then produces a
Windows app with the Windows backend inside.

## Events

Backends run background tasks that listen to D-Bus signals and publish
`PhoneEvent`s on the `EventBus`. The CLI `watch` command prints them; the
desktop app forwards them to the web UI as `phone-event` messages.
