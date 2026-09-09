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
} from "@statasmon/protocol";

/** `store_stats` (Phase 5.1): where the sessions live and how much they take. */
export interface StoreStats {
  root: string;
  total_bytes: number;
  session_count: number;
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
  deviceAction: async (deviceId: string, action: DeviceAction) =>
    (await getStream()).invoke<unknown>("device_action", { deviceId, action }),
  fakeDetach: async (deviceId: string) =>
    (await getStream()).invoke<void>("fake_detach", { deviceId }),
  fakeReattach: async (deviceId: string) =>
    (await getStream()).invoke<void>("fake_reattach", { deviceId }),
};
