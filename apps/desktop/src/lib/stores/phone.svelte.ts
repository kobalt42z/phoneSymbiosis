// The live phone state, fed by "phone-event" from src-tauri/src/events.rs.
// Components read `phone.*` directly; Svelte 5 runes make it reactive.

import { listen } from "@tauri-apps/api/event";
import * as api from "../api";
import type { AppStatus, Call, CallVolume, Device, Message, PhoneEvent, PhoneStatus } from "../types";

export interface Toast {
  id: number;
  kind: "info" | "error";
  text: string;
}

class PhoneStore {
  /** Backend start-up state (setup screen when not ready). */
  app = $state<AppStatus>({ ready: false, starting: true, error: null, backend: "" });
  devices = $state<Device[]>([]);
  calls = $state<Call[]>([]);
  status = $state<PhoneStatus | null>(null);
  volume = $state<CallVolume | null>(null);
  audioActive = $state(false);
  micMuted = $state(false);
  /** Phones whose hands-free link is up. */
  callsAvailable = $state<string[]>([]);
  /** Set while the backend waits for us to confirm a pairing code. */
  pairing = $state<{ address: string; passkey: number } | null>(null);
  /** SMS that arrived while the app was running (Messages view merges these). */
  incomingMessages = $state<Message[]>([]);
  toasts = $state<Toast[]>([]);

  /** The phone shown in the header: first connected device. */
  connected = $derived(this.devices.find((d) => d.connected) ?? null);
  /** A ringing call, for the incoming-call overlay. */
  ringing = $derived(this.calls.find((c) => c.state === "incoming") ?? null);
  /** Any call that is not ringing (for the calls panel / DTMF). */
  inCall = $derived(this.calls.some((c) => c.state !== "incoming" && c.state !== "disconnected"));

  private nextToast = 1;
  private started = false;

  /** Subscribe to backend events once, then load the initial state. */
  async start() {
    if (this.started) return;
    this.started = true;
    await listen<PhoneEvent>("phone-event", (e) => this.apply(e.payload));
    await listen<AppStatus>("app-status", (e) => {
      const wasReady = this.app.ready;
      this.app = e.payload;
      if (!wasReady && e.payload.ready) this.refresh();
    });
    this.app = await api.appStatus();
    if (this.app.ready) await this.refresh();
  }

  /** Re-read everything from the backend. Errors are fine (feature may not exist yet). */
  async refresh() {
    const [devices, calls, status, audio, volume] = await Promise.allSettled([
      api.devices(),
      api.calls(),
      api.phoneStatus(),
      api.audioActive(),
      api.volume(),
    ]);
    if (devices.status === "fulfilled") this.devices = devices.value;
    if (calls.status === "fulfilled") this.calls = calls.value;
    if (status.status === "fulfilled") this.status = status.value;
    if (audio.status === "fulfilled") this.audioActive = audio.value;
    if (volume.status === "fulfilled") this.volume = volume.value;
  }

  /** Apply one event from the backend. */
  apply(ev: PhoneEvent) {
    switch (ev.type) {
      case "device_found":
        this.upsertDevice(ev.device);
        break;
      case "device_connected":
        this.setConnected(ev.address, true);
        this.refresh();
        break;
      case "device_disconnected":
        this.setConnected(ev.address, false);
        this.callsAvailable = this.callsAvailable.filter((a) => a !== ev.address);
        break;
      case "calls_available":
        if (!this.callsAvailable.includes(ev.address)) this.callsAvailable = [...this.callsAvailable, ev.address];
        break;
      case "calls_unavailable":
        this.callsAvailable = this.callsAvailable.filter((a) => a !== ev.address);
        break;
      case "call_added":
      case "call_changed":
        this.upsertCall(ev.call);
        break;
      case "call_removed":
        this.calls = this.calls.filter((c) => c.id !== ev.id);
        break;
      case "status_changed":
        this.status = ev.status;
        break;
      case "volume_changed":
        this.volume = ev.volume;
        break;
      case "audio_routed":
        this.audioActive = ev.active;
        break;
      case "message_received":
        this.incomingMessages = [...this.incomingMessages, ev.message];
        break;
      case "pairing_confirm":
        this.pairing = { address: ev.address, passkey: ev.passkey };
        break;
      case "warning":
        this.toast(ev.message, "error");
        break;
    }
  }

  toast(text: string, kind: Toast["kind"] = "info") {
    const id = this.nextToast++;
    this.toasts = [...this.toasts, { id, kind, text }];
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), 6000);
  }

  /** Run an API call and show its error as a toast. Returns undefined on failure. */
  async run<T>(p: Promise<T>): Promise<T | undefined> {
    try {
      return await p;
    } catch (e) {
      this.toast(api.errorText(e), "error");
      return undefined;
    }
  }

  private upsertDevice(d: Device) {
    const i = this.devices.findIndex((x) => x.address === d.address);
    if (i >= 0) this.devices[i] = d;
    else this.devices = [...this.devices, d];
  }

  private setConnected(address: string, connected: boolean) {
    const d = this.devices.find((x) => x.address === address);
    if (d) d.connected = connected;
    else api.devices().then((list) => (this.devices = list)).catch(() => {});
  }

  private upsertCall(c: Call) {
    if (c.state === "disconnected") {
      this.calls = this.calls.filter((x) => x.id !== c.id);
      return;
    }
    const i = this.calls.findIndex((x) => x.id === c.id);
    if (i >= 0) this.calls[i] = c;
    else this.calls = [...this.calls, c];
  }
}

export const phone = new PhoneStore();
