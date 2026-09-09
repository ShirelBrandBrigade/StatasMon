import type { LogEntry, LogFilter } from "@statasmon/protocol";
import type { FilteredLog, ProcessCount } from "../../overview/log";
import type { LogRing } from "../../store/telemetry";

/**
 * Pure pieces of the Logs screen (docs/PLAN.md 6.4, FEATURES.md §9): the
 * Android buffer set, time-range / buffer narrowing over the incremental
 * filter's output, the per-app tabs, and the stored-session paging math.
 */

// ---------------------------------------------------------------------------
// Buffers

/** Android logcat buffers in display order; anything else follows alphabetically. */
export const BUFFER_ORDER: readonly string[] = ["main", "system", "crash", "events", "radio"];

export function sortBuffers(names: Iterable<string>): string[] {
  return [...new Set(names)].sort((a, b) => {
    const ia = BUFFER_ORDER.indexOf(a);
    const ib = BUFFER_ORDER.indexOf(b);
    if (ia !== -1 || ib !== -1) {
      return (ia === -1 ? BUFFER_ORDER.length : ia) - (ib === -1 ? BUFFER_ORDER.length : ib);
    }
    return a.localeCompare(b);
  });
}

/**
 * Tracks which buffers the ring has carried, scanning only the lines that
 * arrived since the last call (same absolute-position bookkeeping as
 * `LogScanner`). Returns the same array instance while the set is unchanged.
 */
export class BufferTracker {
  private ring: LogRing | undefined;
  private seen = 0;
  private readonly set = new Set<string>();
  private snapshot: readonly string[] = [];

  update(ring: LogRing): readonly string[] {
    const all = ring.all();
    const dropped = ring.droppedCount;
    const end = dropped + all.length;
    if (ring !== this.ring || end < this.seen) {
      this.ring = ring;
      this.seen = 0;
      this.set.clear();
      this.snapshot = [];
    }
    if (end > this.seen) {
      const start = Math.max(0, this.seen - dropped);
      const before = this.set.size;
      for (let i = start; i < all.length; i += 1) {
        const b = all[i].buffer;
        if (b !== null && b !== "") this.set.add(b);
      }
      this.seen = end;
      if (this.set.size !== before) this.snapshot = sortBuffers(this.set);
    }
    return this.snapshot;
  }
}

// ---------------------------------------------------------------------------
// Narrowing (time range + buffers) over the store-driven filter's output

export interface Narrowing {
  /** Inclusive bounds in ms; undefined = open (live). */
  fromMs?: number;
  toMs?: number;
  /** Android buffers to keep; empty = all. */
  buffers: readonly string[];
}

export const NO_NARROWING: Narrowing = Object.freeze({ buffers: [] as string[] });

export function narrowingActive(n: Narrowing): boolean {
  return n.fromMs !== undefined || n.toMs !== undefined || n.buffers.length > 0;
}

export function inNarrowing(e: LogEntry, n: Narrowing, buffers?: ReadonlySet<string>): boolean {
  if (n.fromMs !== undefined && e.ts_ms < n.fromMs) return false;
  if (n.toMs !== undefined && e.ts_ms > n.toMs) return false;
  if (buffers && buffers.size > 0 && (e.buffer === null || !buffers.has(e.buffer))) return false;
  return true;
}

/**
 * Applies the time range and buffer chips to the incremental filter's
 * output. The store's filter (processes, levels, query) stays incremental;
 * this second pass only runs while a range or buffer is set and only when the
 * visible entries changed (memoize on `log`).
 */
export function narrowLog(log: FilteredLog, n: Narrowing): FilteredLog {
  if (!narrowingActive(n)) return log;
  const buffers = new Set(n.buffers);
  const entries: LogEntry[] = [];
  const keys: number[] = [];
  let errors = 0;
  let faults = 0;
  for (let i = 0; i < log.entries.length; i += 1) {
    const e = log.entries[i];
    if (!inNarrowing(e, n, buffers)) continue;
    entries.push(e);
    keys.push(log.keys[i]);
    if (e.level === "error") errors += 1;
    else if (e.level === "fault") faults += 1;
  }
  return { ...log, entries, keys, shown: entries.length, errors, faults };
}

// ---------------------------------------------------------------------------
// datetime-local inputs

const pad2 = (n: number) => String(n).padStart(2, "0");

/** `YYYY-MM-DDTHH:mm:ss` in local time for a `datetime-local` input. */
export function toLocalDateTime(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}T${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

/**
 * The input's value as local-time ms; undefined when empty or unparsable.
 * Browsers may hand back seconds and fractional seconds (`…T14:05:09.000`).
 */
export function parseLocalDateTime(value: string): number | undefined {
  const v = value.trim();
  if (v === "") return undefined;
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.\d{1,3})?)?$/.exec(v);
  if (!m) return undefined;
  const ms = new Date(
    Number(m[1]),
    Number(m[2]) - 1,
    Number(m[3]),
    Number(m[4]),
    Number(m[5]),
    Number(m[6] ?? 0),
  ).getTime();
  return Number.isNaN(ms) ? undefined : ms;
}

// ---------------------------------------------------------------------------
// Per-app tabs

export interface AppTab {
  id: string;
  label: string;
  /** Store `processes` this tab selects; empty = all. */
  processes: string[];
  /** The app under test (gradient tile). */
  me?: boolean;
}

export const TAB_LIMIT = 4;
export const ALL_TAB_ID = "all";

/**
 * "All" · the top processes by line count · the app under test (when it has
 * logged anything, and not already among the top ones).
 */
export function appTabs(
  processes: readonly ProcessCount[],
  isMe: (process: string) => boolean,
  limit = TAB_LIMIT,
): AppTab[] {
  const tabs: AppTab[] = [{ id: ALL_TAB_ID, label: "All", processes: [] }];
  const top = processes.slice(0, limit);
  for (const p of top) {
    tabs.push({
      id: `p:${p.process}`,
      label: p.process,
      processes: [p.process],
      me: isMe(p.process),
    });
  }
  const me = processes.find((p) => isMe(p.process) && !top.includes(p));
  if (me)
    tabs.push({ id: `p:${me.process}`, label: me.process, processes: [me.process], me: true });
  return tabs;
}

/** The tab matching the store's process selection exactly, if any. */
export function activeTabId(
  tabs: readonly AppTab[],
  selected: readonly string[],
): string | undefined {
  if (selected.length === 0) return ALL_TAB_ID;
  if (selected.length !== 1) return undefined;
  return tabs.find((t) => t.processes.length === 1 && t.processes[0] === selected[0])?.id;
}

// ---------------------------------------------------------------------------
// Stored-session paging

/** Lines per "Load older" click (matches `commands.queryLogs`' default page). */
export const STORED_PAGE = 500;

export interface StoredPages {
  /** Older lines from the store, oldest → newest, all before the live ring. */
  entries: LogEntry[];
  /** Absolute offset (within the query's matches) of `entries[0]`. */
  offset: number;
  /** Matches the store reports for the query; undefined until the first page. */
  total: number | undefined;
  loading: boolean;
  error?: string;
}

export const EMPTY_STORED: StoredPages = {
  entries: [],
  offset: 0,
  total: undefined,
  loading: false,
};

/** The window to request next: the page just before what is loaded, or undefined when nothing older remains. */
export function nextStoredPage(
  s: StoredPages,
  page = STORED_PAGE,
): { offset: number; limit: number } | undefined {
  if (s.total === undefined) return { offset: 0, limit: page };
  if (s.entries.length === 0) {
    if (s.total === 0) return undefined;
    const offset = Math.max(0, s.total - page);
    return { offset, limit: s.total - offset };
  }
  if (s.offset === 0) return undefined;
  const offset = Math.max(0, s.offset - page);
  return { offset, limit: s.offset - offset };
}

/** Lines still in the store before the loaded ones. */
export function storedRemaining(s: StoredPages): number {
  if (s.total === undefined) return 0;
  return s.entries.length === 0 ? s.total : s.offset;
}

/**
 * Stable, unique keys for stored rows: negative, so they never collide with
 * the live ring's positions (which are ≥ 0), and derived from the absolute
 * offset so a prepend does not re-key existing rows.
 */
export function storedKey(offset: number, index: number): number {
  return -(1 + offset + index);
}

/**
 * The stored query: the UI filter plus the narrowing, bounded to lines older
 * than the live ring (`oldestLiveMs`) so pages never overlap what is shown.
 */
export function storedFilter(
  base: LogFilter,
  n: Narrowing,
  oldestLiveMs: number | undefined,
): LogFilter {
  const bound = oldestLiveMs === undefined ? undefined : oldestLiveMs - 1;
  const to = n.toMs === undefined ? bound : bound === undefined ? n.toMs : Math.min(n.toMs, bound);
  return {
    ...base,
    from_ms: n.fromMs ?? null,
    to_ms: to ?? null,
    buffers: [...n.buffers],
  };
}

/** Same query → same pages; anything else restarts paging. */
export function storedFilterKey(f: LogFilter): string {
  return JSON.stringify(f);
}

/** How many crash reports a `fetch_crash_reports` action returned, when the shape says. */
export function reportCount(result: unknown): number | undefined {
  if (typeof result === "number") return result;
  if (Array.isArray(result)) return result.length;
  if (result && typeof result === "object") {
    const o = result as Record<string, unknown>;
    for (const k of ["count", "fetched", "reports", "paths", "files"]) {
      const v = o[k];
      if (typeof v === "number") return v;
      if (Array.isArray(v)) return v.length;
    }
  }
  return undefined;
}
