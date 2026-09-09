import type {
  DeviceAction,
  DeviceSummary,
  ExportRequest,
  HistoryPoint,
  HistoryQuery,
  LogFilter,
  LogPage,
  MetricMeta,
  SessionState,
} from "@statasmon/protocol";
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
  exportData: async (request: ExportRequest) =>
    (await getStream()).invoke<string>("export_data", { request }),
  deviceAction: async (deviceId: string, action: DeviceAction) =>
    (await getStream()).invoke<unknown>("device_action", { deviceId, action }),
  fakeDetach: async (deviceId: string) =>
    (await getStream()).invoke<void>("fake_detach", { deviceId }),
  fakeReattach: async (deviceId: string) =>
    (await getStream()).invoke<void>("fake_reattach", { deviceId }),
};
