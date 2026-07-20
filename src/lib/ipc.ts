// Типизированный слой поверх Tauri IPC: команды и события бэкенда.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// Типы (зеркало Rust-структур)
// ---------------------------------------------------------------------------

export type MicSelection =
  | { kind: "always_builtin" }
  | { kind: "system_default" }
  | { kind: "device"; id: string };

export type PostprocMode = "off" | "local" | "cloud";
export type HotkeyMode = "toggle" | "hold";
export type CloudProvider = "anthropic" | "openai" | "deepseek";
export type InjectionMode = "clipboard" | "typing";

export interface ReplacementRule {
  from: string;
  to: string;
  regex: boolean;
  ignore_case: boolean;
}

export interface Dictionary {
  terms: string[];
  replacements: ReplacementRule[];
}

export interface PostprocConfig {
  mode: PostprocMode;
  local_model: string | null;
  cloud_provider: CloudProvider;
  cloud_model: string;
  timeout_secs: number;
}

export interface AppConfig {
  config_version: number;
  wizard_completed: boolean;
  hotkey: string;
  hotkey_mode: HotkeyMode;
  microphone: MicSelection;
  language: string;
  silence_timeout_secs: number;
  max_recording_secs: number;
  asr_model: string | null;
  postproc: PostprocConfig;
  dictionary: Dictionary;
  history_enabled: boolean;
  history_limit: number;
  sounds_enabled: boolean;
  injection_mode: InjectionMode;
  paste_restore_delay_ms: number;
  model_idle_unload_mins: number;
}

export interface AudioDevice {
  id: string;
  name: string;
  is_builtin: boolean;
  is_default: boolean;
}

export type Permission = "microphone" | "accessibility" | "input_monitoring";
export type PermissionStatus =
  | "granted"
  | "denied"
  | "not_determined"
  | "not_applicable";

export interface PermissionInfo {
  permission: Permission;
  status: PermissionStatus;
}

export interface ModelStatus {
  id: string;
  kind: "asr" | "llm";
  file_name: string;
  url: string;
  sha256: string;
  size_bytes: number;
  title: string;
  description: string;
  recommended: boolean;
  downloaded: boolean;
  partial: boolean;
}

export interface ModelTestResult {
  ok: boolean;
  text: string;
  elapsed_ms: number;
  error: string | null;
}

export interface HistoryEntry {
  id: number;
  ts: number;
  raw_text: string;
  clean_text: string | null;
  app_name: string | null;
  duration_ms: number | null;
  model: string | null;
  status: string;
}

export type SessionState =
  | "idle"
  | "arming"
  | "recording"
  | "processing"
  | "error"
  | "notice";

export interface SessionStateEvent {
  state: SessionState;
  detail: string | null;
}

export interface DictationResult {
  raw: string;
  clean: string;
  postproc: "local" | "cloud" | "raw";
  asr_ms: number;
  postproc_ms: number;
  error: string | null;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number;
  done: boolean;
  cancelled: boolean;
  error: string | null;
}

// ---------------------------------------------------------------------------
// Команды
// ---------------------------------------------------------------------------

export const api = {
  configGet: () => invoke<AppConfig>("config_get"),
  configSet: (config: AppConfig) => invoke<void>("config_set", { config }),

  devicesList: () => invoke<AudioDevice[]>("devices_list"),
  builtinDevice: () => invoke<AudioDevice | null>("builtin_device"),

  permissionsList: () => invoke<PermissionInfo[]>("permissions_list"),
  permissionRequest: (permission: Permission) =>
    invoke<void>("permission_request", { permission }),
  permissionOpenSettings: (permission: Permission) =>
    invoke<void>("permission_open_settings", { permission }),

  modelsStatus: () => invoke<ModelStatus[]>("models_status"),
  modelDownload: (id: string) => invoke<void>("model_download", { id }),
  modelDownloadCancel: (id: string) =>
    invoke<void>("model_download_cancel", { id }),
  modelDelete: (id: string) => invoke<void>("model_delete", { id }),
  modelTest: (id: string) => invoke<ModelTestResult>("model_test", { id }),

  cloudKeySet: (provider: CloudProvider, key: string) =>
    invoke<void>("cloud_key_set", { provider, key }),
  cloudKeyPresent: (provider: CloudProvider) =>
    invoke<boolean>("cloud_key_present", { provider }),
  cloudValidate: (provider: CloudProvider, model: string) =>
    invoke<void>("cloud_validate", { provider, model }),

  hotkeyCheck: (combo: string) => invoke<void>("hotkey_check", { combo }),

  dictationToggle: () => invoke<void>("dictation_toggle"),
  dictationCancel: () => invoke<void>("dictation_cancel"),
  dictationTest: () => invoke<void>("dictation_test"),

  historyList: (limit: number) =>
    invoke<HistoryEntry[]>("history_list", { limit }),
  historyDelete: (id: number) => invoke<void>("history_delete", { id }),
  historyClear: () => invoke<void>("history_clear"),

  autostartStatus: () => invoke<boolean>("autostart_status"),
  autostartSet: (enabled: boolean) =>
    invoke<void>("autostart_set", { enabled }),

  pauseSet: (paused: boolean) => invoke<void>("pause_set", { paused }),
  wizardComplete: () => invoke<void>("wizard_complete"),
  mainWindowHide: () => invoke<void>("main_window_hide"),

  updateCheck: () => invoke<UpdateInfo | null>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
};

export interface UpdateInfo {
  version: string;
  notes: string | null;
}

// ---------------------------------------------------------------------------
// События
// ---------------------------------------------------------------------------

export const events = {
  onSessionState: (cb: (e: SessionStateEvent) => void): Promise<UnlistenFn> =>
    listen<SessionStateEvent>("session-state", (e) => cb(e.payload)),
  onAudioLevel: (cb: (level: number) => void): Promise<UnlistenFn> =>
    listen<{ level: number }>("audio-level", (e) => cb(e.payload.level)),
  onSilenceCountdown: (
    cb: (secondsLeft: number | null) => void,
  ): Promise<UnlistenFn> =>
    listen<{ seconds_left: number | null }>("silence-countdown", (e) =>
      cb(e.payload.seconds_left),
    ),
  onDictationResult: (cb: (r: DictationResult) => void): Promise<UnlistenFn> =>
    listen<DictationResult>("dictation-result", (e) => cb(e.payload)),
  onDownloadProgress: (
    cb: (p: DownloadProgress) => void,
  ): Promise<UnlistenFn> =>
    listen<DownloadProgress>("download-progress", (e) => cb(e.payload)),
  onNavigate: (cb: (route: string) => void): Promise<UnlistenFn> =>
    listen<{ route: string }>("navigate", (e) => cb(e.payload.route)),
};

// ---------------------------------------------------------------------------
// Утилиты
// ---------------------------------------------------------------------------

export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} ГБ`;
  if (bytes >= 1_000_000) return `${Math.round(bytes / 1_000_000)} МБ`;
  return `${Math.round(bytes / 1_000)} КБ`;
}

/// Человеческое имя хоткея для подсказок: Ctrl+Alt+Space → ⌃⌥Space (на Mac).
export function prettyHotkey(combo: string): string {
  const isMac = navigator.platform.toLowerCase().includes("mac");
  if (!isMac) return combo;
  return combo
    .replace(/CommandOrControl|Command|Cmd|Super/gi, "⌘")
    .replace(/Control|Ctrl/gi, "⌃")
    .replace(/Alt|Option/gi, "⌥")
    .replace(/Shift/gi, "⇧")
    .replace(/\+/g, "");
}
