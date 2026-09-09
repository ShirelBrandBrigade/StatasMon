import type { ProcessRow } from "@statasmon/protocol";

/**
 * Pure table logic for the Apps & processes screen (docs/PLAN.md 6.3,
 * FEATURES.md §8): sort, search and pin over the newest `ProcessRow` table.
 */

/** The app under test (the mockups' FieldKit placeholder); its rows are highlighted. */
export const ME_APP_ID = "com.example.fieldkit";

export type SortKey =
  | "name"
  | "app_id"
  | "pid"
  | "cpu_pct"
  | "mem_bytes"
  | "threads"
  | "net_bps"
  | "state"
  | "energy";

export type SortDir = "asc" | "desc";

export interface SortSpec {
  key: SortKey;
  dir: SortDir;
}

/** Default: CPU, highest first. */
export const DEFAULT_SORT: SortSpec = { key: "cpu_pct", dir: "desc" };

/** Columns whose natural first click is "highest first". */
const NUMERIC: ReadonlySet<SortKey> = new Set(["cpu_pct", "mem_bytes", "threads", "net_bps"]);

/** The sort after clicking a header: same key flips direction, a new key starts in its natural order. */
export function nextSort(current: SortSpec, key: SortKey): SortSpec {
  if (current.key === key) return { key, dir: current.dir === "asc" ? "desc" : "asc" };
  return { key, dir: NUMERIC.has(key) ? "desc" : "asc" };
}

const STATE_RANK: Record<ProcessRow["state"], number> = {
  foreground: 0,
  background: 1,
  unknown: 2,
};

/** Energy labels the platforms use, from lightest to heaviest; unknown labels sort after. */
const ENERGY_RANK = ["none", "low", "moderate", "medium", "high", "very high"];

function energyRank(label: string): number {
  const i = ENERGY_RANK.indexOf(label.toLowerCase());
  return i === -1 ? ENERGY_RANK.length : i;
}

/** Value of one column as a comparable, or null when the platform does not report it. */
function sortValue(row: ProcessRow, key: SortKey): number | string | null {
  switch (key) {
    case "name":
      return row.name.toLowerCase();
    case "app_id":
      return row.app_id?.toLowerCase() ?? null;
    case "pid":
      return row.pid;
    case "cpu_pct":
    case "mem_bytes":
    case "threads":
    case "net_bps":
      return row[key];
    case "state":
      return STATE_RANK[row.state];
    case "energy":
      return row.energy === null ? null : energyRank(row.energy);
  }
}

function compareValues(a: number | string, b: number | string): number {
  if (typeof a === "number" && typeof b === "number") return a - b;
  return String(a).localeCompare(String(b));
}

/**
 * Comparator for `sort`: unknown values ("—") always sort last regardless of
 * direction, ties break on the name and then the pid so the order is stable
 * between ticks.
 */
export function compareRows(a: ProcessRow, b: ProcessRow, sort: SortSpec): number {
  const va = sortValue(a, sort.key);
  const vb = sortValue(b, sort.key);
  if (va === null && vb === null) return tieBreak(a, b);
  if (va === null) return 1;
  if (vb === null) return -1;
  const c = compareValues(va, vb);
  if (c !== 0) return sort.dir === "asc" ? c : -c;
  return tieBreak(a, b);
}

function tieBreak(a: ProcessRow, b: ProcessRow): number {
  return a.name.localeCompare(b.name) || a.pid - b.pid;
}

/** True when the row's name or app id contains `query` (case-insensitive); an empty query matches all. */
export function matchesSearch(row: ProcessRow, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  return row.name.toLowerCase().includes(q) || (row.app_id?.toLowerCase().includes(q) ?? false);
}

/**
 * The rows the table shows: searched, then sorted, with the pinned pids
 * lifted to the top (in the same sort order among themselves).
 */
export function arrangeRows(
  rows: readonly ProcessRow[],
  opts: { sort: SortSpec; query: string; pinned: ReadonlySet<number> },
): ProcessRow[] {
  const kept = rows.filter((r) => matchesSearch(r, opts.query));
  kept.sort((a, b) => {
    const pa = opts.pinned.has(a.pid) ? 0 : 1;
    const pb = opts.pinned.has(b.pid) ? 0 : 1;
    return pa - pb || compareRows(a, b, opts.sort);
  });
  return kept;
}

/** The app under test: the FieldKit app id, or a name/id containing "fieldkit". */
export function isAppUnderTest(row: Pick<ProcessRow, "name" | "app_id">): boolean {
  if (row.app_id === ME_APP_ID) return true;
  return row.name.toLowerCase().includes("fieldkit") || row.app_id?.includes("fieldkit") === true;
}

/** Largest CPU share among the rows (for the inline bars), 0 when none is reported. */
export function maxCpu(rows: readonly ProcessRow[]): number {
  let max = 0;
  for (const r of rows) if (r.cpu_pct !== null && r.cpu_pct > max) max = r.cpu_pct;
  return max;
}
