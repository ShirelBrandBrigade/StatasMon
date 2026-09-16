import { create } from "zustand";

/** How the log panels show lines: raw records or plain-language stories (ADR-020). */
export type LogViewMode = "raw" | "simple";

const KEY = "statasmon.logView";

function load(): LogViewMode {
  try {
    return localStorage.getItem(KEY) === "simple" ? "simple" : "raw";
  } catch {
    return "raw";
  }
}

interface LogViewStore {
  mode: LogViewMode;
  setMode: (mode: LogViewMode) => void;
}

export const useLogViewStore = create<LogViewStore>((set) => ({
  mode: load(),
  setMode: (mode) => {
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      // Storage may be unavailable; the choice then lasts for the session.
    }
    set({ mode });
  },
}));

export const LOG_VIEW_OPTIONS = [
  { value: "raw", label: "Raw" },
  { value: "simple", label: "Simple" },
] as const satisfies ReadonlyArray<{ value: LogViewMode; label: string }>;
