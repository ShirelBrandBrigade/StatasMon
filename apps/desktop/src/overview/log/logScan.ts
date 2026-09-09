import type { LogEntry } from "@statasmon/protocol";
import type { CompiledLogFilter } from "../../store/logFilter";
import type { LogRing } from "../../store/telemetry";

export interface ProcessCount {
  process: string;
  count: number;
}

/** What the log card renders: the visible entries plus the footer counts. */
export interface FilteredLog {
  /** Entries that pass the filter, oldest → newest. A new array only when it changed. */
  entries: readonly LogEntry[];
  /**
   * One key per visible entry: its absolute position in the ring (index +
   * lines dropped before it). Unique for the ring's lifetime and stable while
   * the entry is in it, unlike `seq`, which a looping replay can repeat.
   */
  keys: readonly number[];
  /** Entries currently in the mirror ring (every level, every app). */
  total: number;
  /** `entries.length`. */
  shown: number;
  /** Errors and faults among the shown entries. */
  errors: number;
  faults: number;
  /** Lines the ring dropped since attach (capacity 50k). */
  dropped: number;
  /** The query is a regex that does not compile. */
  invalid: boolean;
  /** Processes by line count, most first, over the lines this scanner has seen. */
  processes: readonly ProcessCount[];
  /** At least one `warning` line was seen (Android); the card then shows the W toggle. */
  warningsSeen: boolean;
}

/** First index in `sorted` whose value is >= `x`. */
function lowerBound(sorted: readonly number[], x: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (sorted[mid] < x) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/**
 * Incremental filter over a `LogRing`. `update` is called once per log tick;
 * it appends the matches among the lines the ring received since the last
 * call, trims the lines the ring dropped from the front, and returns the
 * previous snapshot when nothing changed. Lines are tracked by absolute ring
 * position (`index + droppedCount`), which the ring guarantees to grow
 * monotonically; `seq` is not relied on. A different `LogRing` instance
 * (detach, reset) starts the scan over.
 *
 * Cost per tick: `matches` for each new line, one binary search when the ring
 * dropped lines, and one pointer copy of the visible arrays (`concat`) when
 * lines were added — about 0.2 ms for 50k visible lines. A full rescan
 * happens only when the filter changes (a new scanner) or the ring is replaced.
 */
export class LogScanner {
  private ring: LogRing | undefined;
  /** Absolute position of the next line the ring will receive, as last seen. */
  private seen = 0;
  private entries: LogEntry[] = [];
  private keys: number[] = [];
  private errors = 0;
  private faults = 0;
  private warnings = 0;
  private readonly counts = new Map<string, number>();
  private processes: ProcessCount[] = [];
  private snapshot: FilteredLog | undefined;

  constructor(private readonly filter: CompiledLogFilter) {}

  private reset(ring: LogRing): void {
    this.ring = ring;
    this.seen = 0;
    this.entries = [];
    this.keys = [];
    this.errors = 0;
    this.faults = 0;
    this.warnings = 0;
    this.counts.clear();
    this.processes = [];
  }

  update(ring: LogRing): FilteredLog {
    const all = ring.all();
    const dropped = ring.droppedCount;
    const n = all.length;
    const end = dropped + n;
    let changed = false;
    let countsChanged = false;

    if (ring !== this.ring || end < this.seen) {
      this.reset(ring);
      changed = true;
    }

    // Trim what the ring dropped from the front.
    if (this.keys.length > 0 && this.keys[0] < dropped) {
      const cut = lowerBound(this.keys, dropped);
      for (let i = 0; i < cut; i += 1) {
        const level = this.entries[i].level;
        if (level === "error") this.errors -= 1;
        else if (level === "fault") this.faults -= 1;
      }
      this.entries = this.entries.slice(cut);
      this.keys = this.keys.slice(cut);
      changed = true;
    }

    // Append matches among the lines received since the last call.
    if (end > this.seen) {
      const start = Math.max(0, this.seen - dropped);
      const added: LogEntry[] = [];
      const addedKeys: number[] = [];
      for (let i = start; i < n; i += 1) {
        const e = all[i];
        this.counts.set(e.process, (this.counts.get(e.process) ?? 0) + 1);
        if (e.level === "warning") this.warnings += 1;
        if (this.filter.matches(e)) {
          added.push(e);
          addedKeys.push(dropped + i);
          if (e.level === "error") this.errors += 1;
          else if (e.level === "fault") this.faults += 1;
        }
      }
      countsChanged = start < n;
      this.seen = end;
      if (added.length > 0) {
        this.entries = this.entries.length === 0 ? added : this.entries.concat(added);
        this.keys = this.keys.length === 0 ? addedKeys : this.keys.concat(addedKeys);
        changed = true;
      }
    }

    if (countsChanged || changed) {
      this.processes = [...this.counts]
        .map(([process, count]) => ({ process, count }))
        .sort((a, b) => b.count - a.count || a.process.localeCompare(b.process));
    }

    const prev = this.snapshot;
    if (prev && !changed && !countsChanged && prev.total === n && prev.dropped === dropped) {
      return prev;
    }
    const snapshot: FilteredLog = {
      entries: this.entries,
      keys: this.keys,
      total: n,
      shown: this.entries.length,
      errors: this.errors,
      faults: this.faults,
      dropped,
      invalid: this.filter.invalid,
      processes: this.processes,
      warningsSeen: this.warnings > 0,
    };
    this.snapshot = snapshot;
    return snapshot;
  }
}
