// TypeScript mirror of the serde output of crates/core/src/model.rs and events.rs.
// Keep in sync with the Rust types: enums are snake_case, PhoneEvent is tagged by "type".

/** Bluetooth address, upper-case "AA:BB:CC:DD:EE:FF". */
export type Address = string;
/** Opaque call id chosen by the backend. */
export type CallId = string;

export interface AdapterInfo {
  address: Address;
  name: string;
  powered: boolean;
  discoverable: boolean;
}

export interface Capabilities {
  calls: boolean;
  contacts: boolean;
  messages: boolean;
}

export interface Device {
  address: Address;
  name: string;
  paired: boolean;
  trusted: boolean;
  connected: boolean;
  rssi: number | null;
  capabilities: Capabilities;
}

export type CallState =
  | "incoming"
  | "waiting"
  | "dialing"
  | "alerting"
  | "active"
  | "held"
  | "disconnected";

export type CallDirection = "incoming" | "outgoing";

export interface Call {
  id: CallId;
  device: Address | null;
  number: string;
  name: string | null;
  state: CallState;
  direction: CallDirection;
  multiparty: boolean;
}

export interface PhoneStatus {
  device: Address | null;
  battery: number | null; // 0-100
  signal: number | null; // 0-5
  operator: string | null;
  roaming: boolean | null;
}

/** 0-15, like HFP. */
export interface CallVolume {
  speaker: number;
  microphone: number;
}

export interface PhoneNumber {
  number: string;
  kind: string | null;
}

export interface Contact {
  id: string;
  name: string;
  numbers: PhoneNumber[];
  emails: string[];
  organization: string | null;
  photo_base64: string | null;
}

export type CallHistoryKind = "incoming" | "outgoing" | "missed";

export interface CallHistoryEntry {
  kind: CallHistoryKind;
  number: string;
  name: string | null;
  timestamp: string | null;
}

export type MessageFolder = "inbox" | "sent" | "outbox" | "draft" | "deleted";

export interface Message {
  id: string;
  folder: MessageFolder;
  peer_number: string;
  peer_name: string | null;
  text: string;
  timestamp: string | null;
  read: boolean;
}

export type PhoneEvent =
  | { type: "device_found"; device: Device }
  | { type: "device_connected"; address: Address }
  | { type: "device_disconnected"; address: Address }
  | { type: "calls_available"; address: Address }
  | { type: "calls_unavailable"; address: Address }
  | { type: "call_added"; call: Call }
  | { type: "call_changed"; call: Call }
  | { type: "call_removed"; id: CallId }
  | { type: "status_changed"; status: PhoneStatus }
  | { type: "volume_changed"; device: Address | null; volume: CallVolume }
  | { type: "audio_routed"; active: boolean }
  | { type: "message_received"; message: Message }
  | { type: "pairing_confirm"; address: Address; passkey: number }
  | { type: "warning"; message: string };

// ---- App-level types (apps/desktop/src-tauri) ----

/** state.rs AppStatus */
export interface AppStatus {
  ready: boolean;
  starting: boolean;
  error: string | null;
  backend: string;
}

/** settings.rs Settings */
export interface Settings {
  last_phone: string | null;
  auto_reconnect: boolean;
  auto_audio: boolean;
}

/** commands/app.rs Check */
export interface Check {
  name: string;
  ok: boolean;
  detail: string;
}
