import { create } from "zustand";

/** What a rejected `device_action` carries (`{ code, message, hint }`, an Error, or a string). */
export interface CaptureFailure {
  code: string;
  message: string;
  hint?: string;
}

export type CaptureState =
  | { kind: "idle" }
  | { kind: "starting"; path: string }
  | { kind: "capturing"; path: string; sinceMs: number }
  | { kind: "stopping"; path: string; sinceMs: number };

interface CaptureStore {
  /** Per device, so leaving and re-entering the screen keeps "Capturing since". */
  byDevice: Record<string, CaptureState>;
  errors: Record<string, CaptureFailure | undefined>;
  /** The path the user last typed or chose, per device. */
  paths: Record<string, string>;
  set(deviceId: string, state: CaptureState): void;
  setError(deviceId: string, failure: CaptureFailure | undefined): void;
  setPath(deviceId: string, path: string): void;
  reset(): void;
}

/** Shared idle state so selectors return a stable reference for devices without a capture. */
export const IDLE: CaptureState = { kind: "idle" };

export const useCaptureStore = create<CaptureStore>((set) => ({
  byDevice: {},
  errors: {},
  paths: {},
  set: (deviceId, state) => set((s) => ({ byDevice: { ...s.byDevice, [deviceId]: state } })),
  setError: (deviceId, failure) => set((s) => ({ errors: { ...s.errors, [deviceId]: failure } })),
  setPath: (deviceId, path) => set((s) => ({ paths: { ...s.paths, [deviceId]: path } })),
  reset: () => set({ byDevice: {}, errors: {}, paths: {} }),
}));

export function captureStateOf(deviceId: string): CaptureState {
  return useCaptureStore.getState().byDevice[deviceId] ?? IDLE;
}

/** One line from whatever the core rejected with. */
export function describeActionError(e: unknown): CaptureFailure {
  if (e && typeof e === "object") {
    const o = e as Record<string, unknown>;
    const message =
      typeof o.message === "string" && o.message
        ? o.message
        : e instanceof Error
          ? e.message
          : "Packet capture failed";
    return {
      code: typeof o.code === "string" && o.code ? o.code : "internal",
      message,
      hint: typeof o.hint === "string" && o.hint ? o.hint : undefined,
    };
  }
  return { code: "internal", message: typeof e === "string" && e ? e : "Packet capture failed" };
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/** File-name-safe slug of a device name: lower case, runs of other characters become one dash. */
export function slug(name: string): string {
  const s = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return s || "device";
}

/** `~/Downloads/StatasMon/<device>-<YYYY-MM-DD-HHMMSS>.pcap` in local time. */
export function defaultCapturePath(deviceName: string, at: Date = new Date()): string {
  const date = `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}-${pad2(at.getHours())}${pad2(at.getMinutes())}${pad2(at.getSeconds())}`;
  return `~/Downloads/StatasMon/${slug(deviceName)}-${date}.pcap`;
}
