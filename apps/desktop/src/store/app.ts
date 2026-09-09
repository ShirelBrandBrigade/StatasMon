import type {
  CapabilitySet,
  ConnectionState,
  DeviceInfo,
  DeviceSummary,
  ErrorCode,
  Headline,
  MetricId,
  SessionState,
} from "@statasmon/protocol";
import { create } from "zustand";

export type Range = "live" | "5m" | "15m" | "1h" | "session";
export const RANGE_MS: Record<Range, number | undefined> = {
  live: 60_000,
  "5m": 300_000,
  "15m": 900_000,
  "1h": 3_600_000,
  session: undefined,
};

export interface DeviceEntry {
  summary: DeviceSummary;
  info?: DeviceInfo;
  capabilities?: CapabilitySet;
  headline?: Headline;
  session?: SessionState;
  latest: Partial<Record<MetricId, number>>;
  lastSampleMs?: number;
  eventCount: number;
  errorCount: number;
}

/** A core `error` message surfaced to the user (Notices stack, EmptyState). */
export interface Notice {
  id: string;
  code: ErrorCode;
  message: string;
  hint?: string;
  deviceId?: string;
  /** `Date.now()` when it was pushed. */
  at: number;
}

export type NoticeInput = Omit<Notice, "id" | "at"> & { at?: number };

/** Only the most recent notices are kept. */
export const NOTICE_LIMIT = 20;

let noticeSeq = 0;

interface AppState {
  devices: Record<string, DeviceEntry>;
  order: string[];
  activeDeviceId?: string;
  range: Range;
  streamKind: "tauri" | "replay" | "none";
  notices: Notice[];
  /** Appends a notice (dropping the oldest past `NOTICE_LIMIT`) and returns its id. */
  pushNotice(n: NoticeInput): string;
  dismissNotice(id: string): void;
  upsertDevice(summary: DeviceSummary): void;
  removeDevice(id: string): void;
  patchDevice(id: string, patch: Partial<DeviceEntry>): void;
  setState(id: string, state: ConnectionState): void;
  setLatest(id: string, tsMs: number, latest: Partial<Record<MetricId, number>>): void;
  setActive(id: string): void;
  setRange(r: Range): void;
  setStreamKind(k: AppState["streamKind"]): void;
  reset(): void;
}

const empty = (summary: DeviceSummary): DeviceEntry => ({
  summary,
  latest: {},
  eventCount: 0,
  errorCount: 0,
});

export const useAppStore = create<AppState>((set) => ({
  devices: {},
  order: [],
  activeDeviceId: undefined,
  range: "5m",
  streamKind: "none",
  notices: [],
  pushNotice: (n) => {
    noticeSeq += 1;
    const id = `notice-${noticeSeq}`;
    const notice: Notice = { ...n, id, at: n.at ?? Date.now() };
    set((s) => ({ notices: [...s.notices, notice].slice(-NOTICE_LIMIT) }));
    return id;
  },
  dismissNotice: (id) =>
    set((s) =>
      s.notices.some((n) => n.id === id) ? { notices: s.notices.filter((n) => n.id !== id) } : s,
    ),
  upsertDevice: (summary) =>
    set((s) => {
      const existing = s.devices[summary.id];
      const devices = {
        ...s.devices,
        [summary.id]: existing ? { ...existing, summary } : empty(summary),
      };
      const order = s.order.includes(summary.id) ? s.order : [...s.order, summary.id];
      return { devices, order, activeDeviceId: s.activeDeviceId ?? summary.id };
    }),
  removeDevice: (id) =>
    set((s) => {
      const { [id]: _gone, ...devices } = s.devices;
      const order = s.order.filter((x) => x !== id);
      const activeDeviceId = s.activeDeviceId === id ? order[0] : s.activeDeviceId;
      return { devices, order, activeDeviceId };
    }),
  patchDevice: (id, patch) =>
    set((s) => {
      const d = s.devices[id];
      if (!d) return s;
      return { devices: { ...s.devices, [id]: { ...d, ...patch } } };
    }),
  setState: (id, state) =>
    set((s) => {
      const d = s.devices[id];
      if (!d) return s;
      return { devices: { ...s.devices, [id]: { ...d, summary: { ...d.summary, state } } } };
    }),
  setLatest: (id, tsMs, latest) =>
    set((s) => {
      const d = s.devices[id];
      if (!d) return s;
      return {
        devices: {
          ...s.devices,
          [id]: { ...d, latest: { ...d.latest, ...latest }, lastSampleMs: tsMs },
        },
      };
    }),
  setActive: (id) => set({ activeDeviceId: id }),
  setRange: (range) => set({ range }),
  setStreamKind: (streamKind) => set({ streamKind }),
  reset: () => set({ devices: {}, order: [], activeDeviceId: undefined, notices: [] }),
}));
