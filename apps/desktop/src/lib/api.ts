// Typed wrappers around the Tauri commands in src-tauri/src/commands/.
// Each rejects with a friendly string on error (see src-tauri/src/errors.rs).
// Argument names are camelCase here; Tauri maps them to the Rust snake_case names.

import { invoke } from "@tauri-apps/api/core";
import type {
  AdapterInfo,
  AppStatus,
  Call,
  CallHistoryEntry,
  CallHistoryKind,
  CallVolume,
  Check,
  Contact,
  Device,
  Message,
  MessageFolder,
  PhoneStatus,
  Settings,
} from "./types";

/** `phone` is optional everywhere: omitted means "the connected phone". */
type Phone = { phone?: string };

// App / settings
export const appStatus = () => invoke<AppStatus>("app_status");
export const retryInit = () => invoke<AppStatus>("retry_init");
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });
export const runChecks = () => invoke<Check[]>("run_checks");

// Devices
export const adapter = () => invoke<AdapterInfo>("adapter");
export const setPowered = (on: boolean) => invoke<void>("set_powered", { on });
export const devices = () => invoke<Device[]>("devices");
export const scan = (seconds = 8) => invoke<Device[]>("scan", { seconds });
export const pair = (address: string) => invoke<void>("pair", { address });
export const confirmPairing = (address: string, accept: boolean) =>
  invoke<void>("confirm_pairing", { address, accept });
export const connect = (address: string) => invoke<void>("connect", { address });
export const disconnect = (address: string) => invoke<void>("disconnect", { address });
export const forget = (address: string) => invoke<void>("forget", { address });

// Calls
export const calls = () => invoke<Call[]>("calls");
export const dial = (number: string, o: Phone = {}) => invoke<void>("dial", { number, ...o });
export const answer = (callId: string) => invoke<void>("answer", { callId });
export const hangUp = (callId: string) => invoke<void>("hang_up", { callId });
export const hangUpAll = (o: Phone = {}) => invoke<void>("hang_up_all", { ...o });
export const sendTones = (tones: string, o: Phone = {}) => invoke<void>("send_tones", { tones, ...o });
export const swapCalls = (o: Phone = {}) => invoke<void>("swap_calls", { ...o });
export const holdAndAnswer = (o: Phone = {}) => invoke<void>("hold_and_answer", { ...o });
export const releaseAndAnswer = (o: Phone = {}) => invoke<void>("release_and_answer", { ...o });
export const mergeCalls = (o: Phone = {}) => invoke<void>("merge_calls", { ...o });
export const phoneStatus = (o: Phone = {}) => invoke<PhoneStatus>("phone_status", { ...o });
export const volume = (o: Phone = {}) => invoke<CallVolume>("volume", { ...o });
export const setVolume = (v: CallVolume, o: Phone = {}) => invoke<void>("set_volume", { volume: v, ...o });
export const audioStart = (o: Phone = {}) => invoke<void>("audio_start", { ...o });
export const audioStop = () => invoke<void>("audio_stop");
export const audioActive = () => invoke<boolean>("audio_active");
export const setMicMuted = (muted: boolean) => invoke<void>("set_mic_muted", { muted });

// Contacts & history
export const contacts = (o: Phone = {}) => invoke<Contact[]>("contacts", { ...o });
export const callHistory = (kind?: CallHistoryKind, o: Phone = {}) =>
  invoke<CallHistoryEntry[]>("call_history", { kind: kind ?? null, ...o });

// Messages
export const listMessages = (folder: MessageFolder, limit = 200, o: Phone = {}) =>
  invoke<Message[]>("list_messages", { folder, limit, ...o });
export const getMessage = (id: string, o: Phone = {}) => invoke<Message>("get_message", { id, ...o });
export const sendMessage = (to: string, text: string, o: Phone = {}) =>
  invoke<void>("send_message", { to, text, ...o });

/** Turn a rejected invoke() into a display string. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
