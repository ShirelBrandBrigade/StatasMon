import type { CoreSample, LogEntry, MetricId, MetricValue, ProcessRow } from "@statasmon/protocol";

/**
 * Live mirror of the core's ring buffers, kept OUTSIDE React so charts and
 * the log list can read typed arrays directly (docs/PLAN.md §3.6).
 */
export class MetricBuffer {
  readonly capacity: number;
  private readonly ts: Float64Array;
  private readonly series = new Map<MetricId, Float64Array>();
  private head = 0;
  private length = 0;

  constructor(capacity = 3600) {
    this.capacity = capacity;
    this.ts = new Float64Array(capacity);
  }

  get size(): number {
    return this.length;
  }

  push(tsMs: number, values: MetricValue[]): void {
    const i = this.head;
    this.ts[i] = tsMs;
    for (const s of this.series.values()) s[i] = Number.NaN;
    for (const { id, value } of values) {
      let s = this.series.get(id);
      if (!s) {
        s = new Float64Array(this.capacity).fill(Number.NaN);
        this.series.set(id, s);
      }
      s[i] = value;
    }
    this.head = (this.head + 1) % this.capacity;
    this.length = Math.min(this.length + 1, this.capacity);
  }

  latest(id: MetricId): number | undefined {
    if (this.length === 0) return undefined;
    const s = this.series.get(id);
    if (!s) return undefined;
    const i = (this.head - 1 + this.capacity) % this.capacity;
    const v = s[i];
    return Number.isNaN(v) ? undefined : v;
  }

  /** Oldest → newest, optionally limited to the last `windowMs`. */
  window(id: MetricId, windowMs?: number): { t: number[]; v: number[] } {
    const t: number[] = [];
    const v: number[] = [];
    const s = this.series.get(id);
    if (!s || this.length === 0) return { t, v };
    const start = (this.head - this.length + this.capacity) % this.capacity;
    const newest = this.ts[(this.head - 1 + this.capacity) % this.capacity];
    for (let k = 0; k < this.length; k += 1) {
      const i = (start + k) % this.capacity;
      if (windowMs !== undefined && newest - this.ts[i] > windowMs) continue;
      t.push(this.ts[i]);
      v.push(s[i]);
    }
    return { t, v };
  }
}

export class LogRing {
  readonly capacity: number;
  private items: LogEntry[] = [];
  private dropped = 0;

  constructor(capacity = 50_000) {
    this.capacity = capacity;
  }

  push(entries: LogEntry[]): void {
    this.items.push(...entries);
    if (this.items.length > this.capacity) {
      const over = this.items.length - this.capacity;
      this.items.splice(0, over);
      this.dropped += over;
    }
  }

  get size(): number {
    return this.items.length;
  }

  get droppedCount(): number {
    return this.dropped;
  }

  all(): readonly LogEntry[] {
    return this.items;
  }
}

export type TelemetryKind = "samples" | "cores" | "processes" | "log";
type Listener = (deviceId: string, kind: TelemetryKind) => void;

class Telemetry {
  readonly metrics = new Map<string, MetricBuffer>();
  readonly logs = new Map<string, LogRing>();
  readonly cores = new Map<string, CoreSample[]>();
  readonly processes = new Map<string, ProcessRow[]>();
  private listeners = new Set<Listener>();

  metricsFor(deviceId: string): MetricBuffer {
    let b = this.metrics.get(deviceId);
    if (!b) {
      b = new MetricBuffer();
      this.metrics.set(deviceId, b);
    }
    return b;
  }

  logsFor(deviceId: string): LogRing {
    let r = this.logs.get(deviceId);
    if (!r) {
      r = new LogRing();
      this.logs.set(deviceId, r);
    }
    return r;
  }

  subscribe(l: Listener): () => void {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  }

  notify(deviceId: string, kind: TelemetryKind): void {
    for (const l of this.listeners) l(deviceId, kind);
  }

  forget(deviceId: string): void {
    this.metrics.delete(deviceId);
    this.logs.delete(deviceId);
    this.cores.delete(deviceId);
    this.processes.delete(deviceId);
  }

  reset(): void {
    this.metrics.clear();
    this.logs.clear();
    this.cores.clear();
    this.processes.clear();
  }
}

export const telemetry = new Telemetry();
