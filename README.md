# PhoneSymbiosis

Use your phone from your PC, the way a car does: make and answer calls with
your PC's speakers and microphone, browse contacts and call history, and read
and send SMS. It uses the standard Bluetooth car profiles (Hands-Free,
Phonebook Access, Message Access), so no app is needed on the phone.

- **Platforms:** Ubuntu (tested target: 26.04). The code is split so other OSes
  can be added as separate backends, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- **Phones:** iPhone first; any phone supporting HFP/PBAP/MAP should work.
- **Stack:** Rust backend, Tauri 2 + Svelte 5 desktop app.

## Project layout

```
crates/core              shared types, interfaces and events (no OS code)
crates/platform          picks the right backend at build time
crates/backends/linux    Linux implementation (BlueZ + PipeWire over D-Bus)
crates/cli               `phonesym` command-line tool
apps/desktop             Tauri + Svelte desktop app
docs/                    architecture and setup notes
```

## Requirements (Ubuntu)

```sh
sudo apt install bluez bluez-obexd pipewire wireplumber pipewire-bin
```

PipeWire 1.4 or newer is needed for call control (its built-in Bluetooth
telephony service). Ubuntu 26.04 ships a new enough version.

Build dependencies:

```sh
sudo apt install build-essential libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
# plus Rust (https://rustup.rs) and Node.js 20+
```

## Try it from the terminal

```sh
cargo run -p phonesym-cli -- doctor          # check your system
cargo run -p phonesym-cli -- device scan     # open Bluetooth settings on the iPhone first
cargo run -p phonesym-cli -- device pair AA:BB:CC:DD:EE:FF
cargo run -p phonesym-cli -- call dial +33612345678
cargo run -p phonesym-cli -- watch --audio   # live events, auto call audio
cargo run -p phonesym-cli -- contacts list
cargo run -p phonesym-cli -- sms list
```

## iPhone settings

After pairing, open **Settings → Bluetooth → (i)** next to your PC and turn on:

- **Sync Contacts** for contacts and call history
- **Show Notifications** for SMS

## Desktop app

See [apps/desktop/README.md](apps/desktop/README.md).

## Install

Download the latest `.deb` from the [Releases page](https://github.com/kobalt42z/phoneSymbiosis/releases) and run:

```sh
sudo apt install ./PhoneSymbiosis_*_amd64.deb
```

Maintainers: releases are built by `.github/workflows/release.yml` when a
`vX.Y.Z` tag matching the app version is pushed.
