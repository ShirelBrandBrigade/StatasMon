import type { ProcessRow } from "@statasmon/protocol";
import { useTelemetryValue } from "../../hooks/useDevice";
import { telemetry } from "../../store/telemetry";

/**
 * Per-process history for the app detail panel (docs/PLAN.md 6.3).
 *
 * The mirror (`telemetry.processes`) keeps only the newest process table, so
 * the detail panel builds its own bounded history: every `processes` tick
 * while a detail panel is open appends that pid's cpu / memory / network
 * values to a ring kept in a module-level map keyed by device + pid.
 *
 * Limits (documented, not hidden):
 * - `PROCESS_HISTORY_POINTS` (900) points per pid: 15 min at the 1 Hz process
 *   cadence; older points are dropped from the front.
 * - At most `PROCESS_HISTORY_RINGS` (64) rings are kept; the least recently
 *   read ring is evicted first, so pids of exited processes do not pile up.
 * - Recording starts when a panel for that pid first mounts; there is no
 *   history from before that moment. The timestamp is the arrival time in
 *   the UI, since the mirror does not keep the table's `ts_ms`.
 * - A tick where the pid is missing from the table appends a gap (NaN), so a
 *   process that exited shows a hole rather than a flat line.
 */
export const PROCESS_HISTORY_POINTS = 900;
export const PROCESS_HISTORY_RINGS = 64;

export interface ProcessHistory {
  /** Arrival timestamps in ms, oldest → newest. */
  t: number[];
  cpu: number[];
  mem: number[];
  net: number[];
}

const EMPTY_ROWS: readonly ProcessRow[] = [];

class HistoryRing {
  readonly t: number[] = [];
  readonly cpu: number[] = [];
  readonly mem: number[] = [];
  readonly net: number[] = [];
  /** The table last appended (a table is appended once, however often it is read). */
  lastRows: readonly ProcessRow[] | undefined;

  append(row: ProcessRow | undefined, tsMs: number): void {
    this.t.push(tsMs);
    this.cpu.push(row?.cpu_pct ?? Number.NaN);
    this.mem.push(row?.mem_bytes ?? Number.NaN);
    this.net.push(row?.net_bps ?? Number.NaN);
    if (this.t.length > PROCESS_HISTORY_POINTS) {
      const over = this.t.length - PROCESS_HISTORY_POINTS;
      this.t.splice(0, over);
      this.cpu.splice(0, over);
      this.mem.splice(0, over);
      this.net.splice(0, over);
    }
  }

  /** A copy, so consumers hold immutable arrays (≤ 900 × 4 numbers per read). */
  snapshot(): ProcessHistory {
    return {
      t: this.t.slice(),
      cpu: this.cpu.slice(),
      mem: this.mem.slice(),
      net: this.net.slice(),
    };
  }
}

const rings = new Map<string, HistoryRing>();

function key(deviceId: string, pid: number): string {
  return `${deviceId}:${pid}`;
}

function ringFor(deviceId: string, pid: number): HistoryRing {
  const k = key(deviceId, pid);
  let ring = rings.get(k);
  if (ring) {
    // Re-insert so Map order doubles as recency (least recently read first).
    rings.delete(k);
  } else {
    ring = new HistoryRing();
    while (rings.size >= PROCESS_HISTORY_RINGS) {
      const oldest = rings.keys().next().value;
      if (oldest === undefined) break;
      rings.delete(oldest);
    }
  }
  rings.set(k, ring);
  return ring;
}

/**
 * Appends the pid's values from `rows` (once per distinct table) and returns
 * the history so far. Exported for tests and for callers outside React.
 */
export function recordProcessRow(
  deviceId: string,
  pid: number,
  rows: readonly ProcessRow[],
  tsMs = Date.now(),
): ProcessHistory {
  const ring = ringFor(deviceId, pid);
  if (rows.length > 0 && rows !== ring.lastRows) {
    ring.lastRows = rows;
    ring.append(
      rows.find((r) => r.pid === pid),
      tsMs,
    );
  }
  return ring.snapshot();
}

/** Points recorded so far for a pid (0 when nothing was recorded). */
export function processHistorySize(deviceId: string, pid: number): number {
  return rings.get(key(deviceId, pid))?.t.length ?? 0;
}

/** Forgets every ring (tests, device reset). */
export function resetProcessHistory(): void {
  rings.clear();
}

/**
 * The cpu / memory / network history of one process, appended on every
 * `processes` tick (at most one render per animation frame) while mounted.
 */
export function useProcessHistory(deviceId: string | undefined, pid: number): ProcessHistory {
  return useTelemetryValue(
    deviceId,
    ["processes"],
    () =>
      deviceId
        ? recordProcessRow(deviceId, pid, telemetry.processes.get(deviceId) ?? EMPTY_ROWS)
        : { t: [], cpu: [], mem: [], net: [] },
    [pid],
  );
}
