import type { LogEntry, LogLevel } from "@statasmon/protocol";
import { useMemo } from "react";
import { create } from "zustand";

/**
 * Per-device log filter for the Overview log card (docs/PLAN.md step 1.5,
 * FEATURES.md §9). Pure matching lives here so the incremental scan in
 * `overview/log/logScan.ts` and, later, the Rust-side `LogFilter` share one
 * definition of "matches".
 */

/** Every level, in severity order (the order the toggles are drawn in). */
export const ALL_LEVELS: readonly LogLevel[] = [
  "debug",
  "info",
  "notice",
  "warning",
  "error",
  "fault",
];

/** Default: everything except debug, like the mockup's filter row. */
export const DEFAULT_LEVELS: readonly LogLevel[] = ["info", "notice", "warning", "error", "fault"];

export interface LogFilterState {
  /** Process names or app ids (matched against `process` and `subsystem`); empty = all. */
  processes: string[];
  /**
   * Levels that are shown. Unlike the protocol's `LogFilter`, an empty list
   * hides every entry: it means the user switched every level off.
   */
  levels: LogLevel[];
  /**
   * Free text: case-insensitive substring over message, process, tag,
   * subsystem and category. A leading `/` makes it a regex: `/pattern/flags`
   * (or `/pattern` while still typing). Flags are honored as typed.
   */
  query: string;
  /** Keep the newest line in view (Space toggles it when the list has focus). */
  followTail: boolean;
}

export const DEFAULT_LOG_FILTER: Readonly<LogFilterState> = Object.freeze({
  processes: [],
  levels: [...DEFAULT_LEVELS],
  query: "",
  followTail: true,
});

interface LogFilterStore {
  byDevice: Record<string, LogFilterState>;
  setProcesses(deviceId: string, processes: string[]): void;
  toggleProcess(deviceId: string, process: string): void;
  setLevels(deviceId: string, levels: LogLevel[]): void;
  toggleLevel(deviceId: string, level: LogLevel): void;
  setQuery(deviceId: string, query: string): void;
  setFollowTail(deviceId: string, followTail: boolean): void;
  /** Forget one device's filter, or all of them. */
  reset(deviceId?: string): void;
}

function fresh(): LogFilterState {
  return { ...DEFAULT_LOG_FILTER, processes: [], levels: [...DEFAULT_LEVELS] };
}

/** Keeps `levels` in severity order so equal sets produce equal arrays. */
function orderLevels(levels: Iterable<LogLevel>): LogLevel[] {
  const set = new Set(levels);
  return ALL_LEVELS.filter((l) => set.has(l));
}

export const useLogFilterStore = create<LogFilterStore>((set) => {
  const patch = (deviceId: string, fn: (f: LogFilterState) => Partial<LogFilterState>) =>
    set((s) => {
      const current = s.byDevice[deviceId] ?? fresh();
      return { byDevice: { ...s.byDevice, [deviceId]: { ...current, ...fn(current) } } };
    });
  return {
    byDevice: {},
    setProcesses: (id, processes) => patch(id, () => ({ processes: [...new Set(processes)] })),
    toggleProcess: (id, process) =>
      patch(id, (f) => ({
        processes: f.processes.includes(process)
          ? f.processes.filter((p) => p !== process)
          : [...f.processes, process],
      })),
    setLevels: (id, levels) => patch(id, () => ({ levels: orderLevels(levels) })),
    toggleLevel: (id, level) =>
      patch(id, (f) => {
        const next = new Set(f.levels);
        if (next.has(level)) next.delete(level);
        else next.add(level);
        return { levels: orderLevels(next) };
      }),
    setQuery: (id, query) => patch(id, () => ({ query })),
    setFollowTail: (id, followTail) => patch(id, () => ({ followTail })),
    reset: (deviceId) =>
      set((s) => {
        if (deviceId === undefined) return { byDevice: {} };
        const { [deviceId]: _gone, ...byDevice } = s.byDevice;
        return { byDevice };
      }),
  };
});

/** The filter for one device (a stable reference until it changes). */
export function useLogFilter(deviceId: string): LogFilterState {
  return useLogFilterStore((s) => s.byDevice[deviceId] ?? DEFAULT_LOG_FILTER);
}

export interface LogFilterActions {
  setProcesses(processes: string[]): void;
  toggleProcess(process: string): void;
  setLevels(levels: LogLevel[]): void;
  toggleLevel(level: LogLevel): void;
  setQuery(query: string): void;
  setFollowTail(followTail: boolean): void;
}

/** Store actions bound to one device (a stable object per device id). */
export function useLogFilterActions(deviceId: string): LogFilterActions {
  return useMemo(() => {
    const s = useLogFilterStore.getState;
    return {
      setProcesses: (processes) => s().setProcesses(deviceId, processes),
      toggleProcess: (process) => s().toggleProcess(deviceId, process),
      setLevels: (levels) => s().setLevels(deviceId, levels),
      toggleLevel: (level) => s().toggleLevel(deviceId, level),
      setQuery: (query) => s().setQuery(deviceId, query),
      setFollowTail: (followTail) => s().setFollowTail(deviceId, followTail),
    };
  }, [deviceId]);
}

// ---------------------------------------------------------------------------
// Matching (pure)

export type ParsedQuery =
  | { kind: "none" }
  | { kind: "text"; needle: string }
  | { kind: "regex"; re: RegExp }
  | { kind: "invalid"; source: string };

const CLOSED_REGEX = /^\/(.*)\/([a-z]*)$/s;

/** Parses the query box: plain text, `/regex/flags`, `/regex` (unclosed), or invalid. */
export function parseQuery(raw: string): ParsedQuery {
  const query = raw.trim();
  if (query === "") return { kind: "none" };
  if (!query.startsWith("/")) return { kind: "text", needle: query.toLowerCase() };
  const closed = CLOSED_REGEX.exec(query);
  const source = closed ? closed[1] : query.slice(1);
  // `g` and `y` make `RegExp#test` stateful; never allow them.
  const flags = (closed ? closed[2] : "").replace(/[gy]/g, "");
  if (source === "") return { kind: "none" };
  try {
    return { kind: "regex", re: new RegExp(source, flags) };
  } catch {
    return { kind: "invalid", source };
  }
}

export interface CompiledLogFilter {
  /** True when `entry` passes process, level and query. */
  matches(entry: LogEntry): boolean;
  /** The query is a regex that failed to compile; nothing matches. */
  invalid: boolean;
  /** Something narrows the stream (a process, a hidden level, or a query). */
  active: boolean;
}

const NEVER = () => false;

/** Compiles a filter once; call `matches` per entry. */
export function compileLogFilter(f: LogFilterState): CompiledLogFilter {
  const procs = f.processes.length > 0 ? new Set(f.processes) : undefined;
  const allLevels = f.levels.length === ALL_LEVELS.length;
  const levels = allLevels ? undefined : new Set(f.levels);
  const q = parseQuery(f.query);
  const active = procs !== undefined || !allLevels || q.kind !== "none";

  if (q.kind === "invalid") return { matches: NEVER, invalid: true, active };

  const textMatch: (e: LogEntry) => boolean =
    q.kind === "none"
      ? () => true
      : q.kind === "text"
        ? (e) => {
            const n = q.needle;
            return (
              e.message.toLowerCase().includes(n) ||
              e.process.toLowerCase().includes(n) ||
              e.tag?.toLowerCase().includes(n) === true ||
              e.subsystem?.toLowerCase().includes(n) === true ||
              e.category?.toLowerCase().includes(n) === true
            );
          }
        : (e) => {
            const re = q.re;
            return (
              re.test(e.message) ||
              re.test(e.process) ||
              (e.tag !== null && re.test(e.tag)) ||
              (e.subsystem !== null && re.test(e.subsystem)) ||
              (e.category !== null && re.test(e.category))
            );
          };

  return {
    invalid: false,
    active,
    matches: (e) => {
      if (levels && !levels.has(e.level)) return false;
      if (procs && !procs.has(e.process) && !(e.subsystem !== null && procs.has(e.subsystem))) {
        return false;
      }
      return textMatch(e);
    },
  };
}

/** One-off check; compile once with `compileLogFilter` when checking many entries. */
export function matches(entry: LogEntry, filter: LogFilterState): boolean {
  return compileLogFilter(filter).matches(entry);
}
