# PhoneSymbiosis desktop app

Tauri 2 + Svelte 5 front end. It talks to the phone only through the
`phonesym-platform` crate, which picks the OS backend at compile time, so a
Windows (or macOS) build only needs a new backend crate, not app changes.

## Run in development

```sh
cd apps/desktop
npm install
npm run tauri dev
```

System libraries (Ubuntu): `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`.

## Build a package

```sh
npm run tauri build -- --bundles deb        # or: --bundles deb,appimage
# -> target/release/bundle/deb/PhoneSymbiosis_<version>_amd64.deb
```

The `.deb` depends on `bluez`, `bluez-obexd`, `pipewire-bin`, `wireplumber`
and `libspa-0.2-bluetooth`.

`cargo build -p phonesym-desktop` also works, but needs `dist/` to exist
first (`npm run build`).

## Layout

```
src-tauri/                Rust side (crate `phonesym-desktop`)
  src/main.rs             app setup, close-to-tray, command registration
  src/state.rs            AppState: Arc<Platform> + settings; backend start-up
  src/settings.rs         settings.json in the app config dir
  src/errors.rs           core Error -> friendly message for the UI
  src/events.rs           PhoneEvent -> "phone-event" + notifications
  src/tray.rs             tray icon: Show/Hide, Answer, Hang up, Quit
  src/reconnect.rs        reconnects the last phone every 10 s
  src/commands/*.rs       #[tauri::command] wrappers, one file per area
src/                      Svelte front end
  lib/types.ts            TS mirror of crates/core model.rs + events.rs
  lib/api.ts              typed invoke() wrappers, one per command
  lib/stores/phone.svelte.ts  live state fed by "phone-event"
  lib/stores/pim.svelte.ts    cached contacts / history / messages
  lib/stores/ui.svelte.ts     current view + "call/text this number"
  lib/components/         sidebar, keypad, overlays, toasts
  views/                  one file per sidebar entry
```

## How it fits together

- On start the backend is initialised in the background. If it fails, the app
  stays open and shows a setup screen with the error and a Retry button.
- Every backend event is forwarded to the UI as `phone-event` (same JSON as
  `phonesym watch --json`). Incoming calls and SMS also raise a desktop
  notification; an incoming call brings the window to the front.
- Closing the window hides it to the tray; use **Quit** in the tray menu to exit.
- Settings (`~/.config/io.github.kobalt42z.phonesymbiosis/settings.json`):
  `last_phone`, `auto_reconnect` (default on), `auto_audio` (default on,
  applied at start-up).
- Features a backend has not implemented yet return "Not available yet on this
  system"; views show that as an empty/error state instead of failing.

## Icons

`src-tauri/icons/icon.svg` is the placeholder source. To regenerate the PNG
sizes from a 1024 px PNG: `npm run icons`.
