import type {
  DeviceAction,
  DeviceSummary,
  ExportRequest,
  ExportResult,
  HistoryPoint,
  HistoryQuery,
  LogFilter,
  LogPage,
  MetricMeta,
  SessionDetail,
  SessionState,
  SessionSummary,
  Settings,
} from "@statasmon/protocol";

/** `store_stats` (Phase 5.1): where the sessions live and how much they take. */
export interface StoreStats {
  root: string;
  total_bytes: number;
  session_count: number;
}

/** `check_for_update` (Phase 7.4): whether the release endpoint has a newer version. */
export interface UpdateStatus {
  available: boolean;
  version?: string;
  notes?: string;
}

/** `apple_support_status` (ADR-022): what iPhones and iPads need on this computer. */
export interface AppleSupportStatus {
  os: "macos" | "windows" | "linux";
  /** True when the app can install it (Linux with a known package manager and polkit). */
  installable: boolean;
  package_manager: string | null;
  /** The command that would run as root, shown before it runs. */
  command: string | null;
  reason: string | null;
}

import { getStream } from "./index";

/** Typed wrappers over the core commands (names are the contract with src-tauri). */
export const commands = {
  listDevices: async () => (await getStream()).invoke<DeviceSummary[]>("list_devices"),
  metricCatalog: async () => (await getStream()).invoke<MetricMeta[]>("metric_catalog"),
  startSession: async (deviceId: string) =>
    (await getStream()).invoke<SessionState>("start_session", { deviceId }),
  pauseSession: async (deviceId: string) =>
    (await getStream()).invoke<SessionState>("pause_session", { deviceId }),
  resumeSession: async (deviceId: string) =>
    (await getStream()).invoke<SessionState>("resume_session", { deviceId }),
  stopSession: async (deviceId: string) =>
    (await getStream()).invoke<SessionState>("stop_session", { deviceId }),
  mark: async (deviceId: string, label: string) =>
    (await getStream()).invoke<SessionState>("mark", { deviceId, label }),
  queryLogs: async (filter: LogFilter, offset = 0, limit = 500) =>
    (await getStream()).invoke<LogPage>("query_logs", { filter, offset, limit }),
  queryHistory: async (query: HistoryQuery) =>
    (await getStream()).invoke<HistoryPoint[]>("query_history", { query }),
  /** Runs on a worker; progress arrives as `progress` messages, the result when done. */
  exportData: async (request: ExportRequest) =>
    (await getStream()).invoke<ExportResult>("export_data", { request }),
  listSessions: async () => (await getStream()).invoke<SessionSummary[]>("list_sessions"),
  openSession: async (sessionId: string) =>
    (await getStream()).invoke<SessionDetail>("open_session", { sessionId }),
  deleteSession: async (sessionId: string) =>
    (await getStream()).invoke<void>("delete_session", { sessionId }),
  storeStats: async () => (await getStream()).invoke<StoreStats>("store_stats"),
  /** The settings in effect (Phase 6.6). */
  getSettings: async () => (await getStream()).invoke<Settings>("get_settings"),
  /** Validates, saves and returns the stored (normalised) settings; rejects with `{ code, message }`. */
  setSettings: async (settings: Settings) =>
    (await getStream()).invoke<Settings>("set_settings", { settings }),
  deviceAction: async (deviceId: string, action: DeviceAction) =>
    (await getStream()).invoke<unknown>("device_action", { deviceId, action }),
  fakeDetach: async (deviceId: string) =>
    (await getStream()).invoke<void>("fake_detach", { deviceId }),
  fakeReattach: async (deviceId: string) =>
    (await getStream()).invoke<void>("fake_reattach", { deviceId }),
  /**
   * Asks the release endpoint once (a user action, allowed whatever the launch
   * setting). Rejects with `{ code: "unsupported" }` when the build has no
   * updater endpoint/public key, `{ code: "io" }` when the request failed.
   */
  checkForUpdate: async () => (await getStream()).invoke<UpdateStatus>("check_for_update"),
  /**
   * Downloads, verifies the signature, installs and relaunches. Guarded by the
   * "Check for updates on launch" setting; progress arrives as `progress`
   * messages with `task_id: "update"`. Resolves only on failure (the app restarts).
   */
  installUpdate: async () => (await getStream()).invoke<void>("install_update"),
  /** What iPhones and iPads need here, and whether the app can install it. */
  appleSupportStatus: async () =>
    (await getStream()).invoke<AppleSupportStatus | null>("apple_support_status"),
  /**
   * Installs usbmuxd through the system's password dialog (Linux). Resolves
   * with the installer's last lines; rejects with `{ code, message, hint }`.
   */
  installAppleSupport: async () => (await getStream()).invoke<string>("install_apple_support"),
};
