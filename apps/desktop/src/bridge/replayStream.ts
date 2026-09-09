import type {
  DeviceInfo,
  DeviceSummary,
  ExportRequest,
  ExportResult,
  HistoryPoint,
  HistoryQuery,
  LogFilter,
  LogPage,
  Message,
  MetricId,
  SessionDetail,
  SessionState,
  SessionSummary,
  StoredEvent,
} from "@statasmon/protocol";
import type { DeviceStream } from "./stream";
import { synthesize } from "./synthetic";

declare const __FIXTURE_DIR__: string;

const SEQ_PER_LOOP = 10_000_000;
const DAY_MS = 86_400_000;
/** Delay between the three `progress` messages of a replayed export. */
const EXPORT_STEP_MS = 120;

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * Two stored sessions from "yesterday" (different lengths) so the Sessions
 * screen has something to reopen, compare and delete in the browser.
 */
function syntheticStoredSessions(now: number): SessionSummary[] {
  const yesterday = now - DAY_MS;
  const mk = (
    id: string,
    device: Pick<SessionSummary, "device_id" | "device_name" | "model_name" | "platform">,
    startOffsetMs: number,
    elapsedMs: number,
  ): SessionSummary => ({
    id,
    ...device,
    started_at_ms: yesterday + startOffsetMs,
    ended_at_ms: yesterday + startOffsetMs + elapsedMs,
    elapsed_ms: elapsedMs,
    sample_count: Math.round(elapsedMs / 1000),
    log_count: Math.round(elapsedMs / 1000) * 5,
    event_count: 4,
    mark_count: 2,
    size_bytes: Math.round(elapsedMs / 1000) * 1_900,
    live: false,
  });
  return [
    mk(
      "stored-iphone-yesterday",
      {
        device_id: "fake:iphone",
        device_name: "iPhone 16 Pro",
        model_name: "iPhone 16 Pro",
        platform: "ios",
      },
      -3 * 3_600_000,
      42 * 60_000,
    ),
    mk(
      "stored-android-yesterday",
      {
        device_id: "fake:android",
        device_name: "Galaxy Tab S8",
        model_name: "SM-X700",
        platform: "android",
      },
      -1 * 3_600_000,
      12 * 60_000,
    ),
  ];
}

/** Metrics a stored synthetic session "recorded", with a plausible value range. */
const STORED_METRICS: ReadonlyArray<{ id: MetricId; lo: number; hi: number }> = [
  { id: "cpu.total", lo: 18, hi: 72 },
  { id: "mem.used", lo: 4.6 * 2 ** 30, hi: 5.7 * 2 ** 30 },
  { id: "thermal.state", lo: 0, hi: 1 },
  { id: "bat.percent", lo: 71, hi: 92 },
  { id: "temp.battery", lo: 27.5, hi: 35.5 },
  { id: "net.rx_bps", lo: 1.2e5, hi: 4.8e6 },
  { id: "net.tx_bps", lo: 4e4, hi: 9e5 },
];

/** Deterministic value of a stored metric at `tsMs` (smooth, differs per session). */
function storedValue(session: SessionSummary, metric: MetricId, tsMs: number): number {
  const m = STORED_METRICS.find((x) => x.id === metric);
  if (!m) return 0;
  const phase = (tsMs - session.started_at_ms) / Math.max(1, session.elapsed_ms);
  const seed = session.id.length % 7;
  if (metric === "thermal.state") return phase > 0.55 + seed * 0.03 ? 1 : 0;
  if (metric === "bat.percent") return m.hi - (m.hi - m.lo) * phase;
  const wave = 0.5 + 0.5 * Math.sin(phase * Math.PI * (3 + seed) + seed);
  const ripple = 0.08 * Math.sin(phase * Math.PI * 47);
  return m.lo + (m.hi - m.lo) * Math.min(1, Math.max(0, wave + ripple));
}

function storedMarks(session: SessionSummary): { ts_ms: number; label: string }[] {
  return [
    { ts_ms: session.started_at_ms + session.elapsed_ms * 0.25, label: "Mark 1" },
    { ts_ms: session.started_at_ms + session.elapsed_ms * 0.7, label: "Mark 2" },
  ];
}

function storedEvents(session: SessionSummary): StoredEvent[] {
  const at = (f: number) => session.started_at_ms + session.elapsed_ms * f;
  return [
    { ts_ms: at(0), kind: "connection", detail: "Session started", pid: null, process: null },
    {
      ts_ms: at(0.4),
      kind: "thermal_change",
      detail: "Nominal → Fair",
      pid: 76,
      process: "thermalmonitord",
    },
    {
      ts_ms: at(0.62),
      kind: "memory_warning",
      detail: "Memory pressure: warning",
      pid: null,
      process: null,
    },
    { ts_ms: at(1), kind: "connection", detail: "Session stopped", pid: null, process: null },
  ];
}

interface Timed {
  at: number;
  message: Message;
}

/**
 * Browser replay of a recorded fixture (`fixtures/fake/*.jsonl`, one Message
 * per line written by the Rust fake device). Falls back to a small built-in
 * synthetic session when the fixture cannot be loaded, so the UI always has
 * something to show. Commands are answered from the replayed state.
 */
export function createReplayStream(opts: { fixture?: string; speed?: number } = {}): DeviceStream {
  const speed = opts.speed ?? 1;
  const state = new ReplayState();
  const listeners = new Set<(m: Message) => void>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let started = false;

  function emit(m: Message) {
    state.apply(m);
    for (const l of listeners) l(m);
  }

  async function load(): Promise<Timed[]> {
    const name = opts.fixture ?? "session-3dev-30s.jsonl";
    try {
      const res = await fetch(`/@fs${__FIXTURE_DIR__}/${name}`);
      if (!res.ok) throw new Error(String(res.status));
      const text = await res.text();
      const items = text
        .split("\n")
        .filter((l) => l.trim().length > 0)
        .map((l) => JSON.parse(l) as Message);
      const t0 = firstTs(items) ?? 0;
      return items.map((message) => ({ at: (tsOf(message) ?? t0) - t0, message }));
    } catch {
      return synthesize(30_000).map((x) => ({ at: x.at, message: x.message }));
    }
  }

  function run(items: Timed[]) {
    if (items.length === 0) return;
    const loopMs = items[items.length - 1].at + 1000;
    let i = 0;
    let loop = 0;
    const startedAt = performance.now();
    const step = () => {
      const now = (performance.now() - startedAt) * speed;
      while (i < items.length && items[i].at + loop * loopMs <= now) {
        emit(state.rebase(items[i].message, loop * loopMs, loop));
        i += 1;
      }
      if (i >= items.length) {
        i = 0;
        loop += 1;
      }
      timer = setTimeout(step, 100 / speed);
    };
    step();
  }

  return {
    kind: "replay",
    subscribe(onMessage) {
      listeners.add(onMessage);
      for (const m of state.replayForNewListener()) onMessage(m);
      if (!started) {
        started = true;
        void load().then(run);
      }
      return () => {
        listeners.delete(onMessage);
        if (listeners.size === 0 && timer) {
          clearTimeout(timer);
          timer = undefined;
          started = false;
        }
      };
    },
    async invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
      return state.command(command, args, emit) as T;
    },
  };
}

function tsOf(m: Message): number | undefined {
  return "ts_ms" in m ? m.ts_ms : undefined;
}
function firstTs(items: Message[]): number | undefined {
  for (const m of items) {
    const t = tsOf(m);
    if (t !== undefined) return t;
  }
  return undefined;
}

/** Enough replayed state to answer the commands the UI issues. */
class ReplayState {
  devices = new Map<string, DeviceSummary>();
  sticky = new Map<string, Message[]>(); // per device: attached, info, capabilities, headline, session
  logs = new Map<string, Message[]>();
  samples = new Map<string, { ts_ms: number; values: Record<string, number> }[]>();
  sessions = new Map<string, SessionState>();
  events = new Map<string, StoredEvent[]>();
  /** Synthetic stored sessions (created on first use so "yesterday" is relative to now). */
  private stored: SessionSummary[] | undefined;
  private exportSeq = 0;
  /** Shifts timestamps (and log sequence numbers) for the n-th loop of the fixture. */
  rebase(m: Message, offset: number, loop = 0): Message {
    if (offset === 0 && loop === 0) return m;
    if ("ts_ms" in m) return { ...m, ts_ms: m.ts_ms + offset } as Message;
    if (m.type === "log") {
      const seqOffset = loop * SEQ_PER_LOOP;
      return {
        ...m,
        entries: m.entries.map((e) => ({ ...e, ts_ms: e.ts_ms + offset, seq: e.seq + seqOffset })),
      };
    }
    return m;
  }

  apply(m: Message) {
    const id = "device_id" in m ? m.device_id : m.type === "device_attached" ? m.device.id : null;
    if (!id) return;
    switch (m.type) {
      case "device_attached":
        this.devices.set(id, m.device);
        this.sticky.set(id, [m]);
        break;
      case "device_detached":
        this.devices.delete(id);
        this.sticky.delete(id);
        break;
      case "info":
      case "capabilities":
      case "headline":
      case "device_state":
        this.replaceSticky(id, m);
        break;
      case "session":
        this.sessions.set(id, m.session);
        this.replaceSticky(id, m);
        break;
      case "log": {
        const arr = this.logs.get(id) ?? [];
        arr.push(m);
        if (arr.length > 2000) arr.splice(0, arr.length - 2000);
        this.logs.set(id, arr);
        break;
      }
      case "event": {
        const arr = this.events.get(id) ?? [];
        arr.push({
          ts_ms: m.ts_ms,
          kind: m.kind,
          detail: m.detail,
          pid: m.pid,
          process: m.process,
        });
        if (arr.length > 500) arr.splice(0, arr.length - 500);
        this.events.set(id, arr);
        break;
      }
      case "samples": {
        const arr = this.samples.get(id) ?? [];
        arr.push({
          ts_ms: m.ts_ms,
          values: Object.fromEntries(m.values.map((v) => [v.id, v.value])),
        });
        if (arr.length > 3600) arr.splice(0, arr.length - 3600);
        this.samples.set(id, arr);
        break;
      }
      default:
        break;
    }
  }

  private replaceSticky(id: string, m: Message) {
    const arr = this.sticky.get(id) ?? [];
    const idx = arr.findIndex((x) => x.type === m.type);
    if (idx >= 0) arr[idx] = m;
    else arr.push(m);
    this.sticky.set(id, arr);
  }

  replayForNewListener(): Message[] {
    return [...this.sticky.values()].flat();
  }

  command(command: string, args: Record<string, unknown>, emit: (m: Message) => void): unknown {
    const deviceId = args.deviceId as string | undefined;
    switch (command) {
      case "list_devices":
        return [...this.devices.values()];
      case "metric_catalog":
        return [];
      case "start_session":
      case "resume_session":
        return this.setSession(deviceId, "recording", emit);
      case "pause_session":
        return this.setSession(deviceId, "paused", emit);
      case "stop_session":
        return this.setSession(deviceId, "stopped", emit);
      case "mark": {
        if (!deviceId) return null;
        const s = this.session(deviceId);
        s.marks = [...s.marks, { ts_ms: Date.now(), label: String(args.label ?? "Mark") }];
        emit({ type: "session", device_id: deviceId, session: s });
        return s;
      }
      case "query_logs":
        return this.queryLogs(
          args.filter as LogFilter,
          Number(args.offset ?? 0),
          Number(args.limit ?? 500),
        );
      case "query_history":
        return this.queryHistory(args.query as HistoryQuery);
      case "list_sessions":
        return this.listSessions();
      case "open_session":
        return this.openSession(String(args.sessionId ?? ""));
      case "delete_session": {
        const id = String(args.sessionId ?? "");
        this.stored = this.storedSessions().filter((x) => x.id !== id);
        return undefined;
      }
      case "export_data":
        return this.exportData(args.request as ExportRequest, emit);
      case "store_stats": {
        const stored = this.storedSessions();
        return {
          root: "~/Library/Application Support/StatasMon/sessions (replay)",
          total_bytes: stored.reduce((n, x) => n + x.size_bytes, 0),
          session_count: stored.length + this.sessions.size,
        };
      }
      default:
        return null;
    }
  }

  private storedSessions(): SessionSummary[] {
    if (!this.stored) this.stored = syntheticStoredSessions(Date.now());
    return this.stored;
  }

  /** One live session per device with a session, then the stored ones. */
  private listSessions(): SessionSummary[] {
    const live: SessionSummary[] = [];
    for (const [deviceId, s] of this.sessions) {
      const d = this.devices.get(deviceId);
      if (!d || !s.id || s.started_at_ms === null) continue;
      const rows = this.samples.get(deviceId) ?? [];
      const logs = (this.logs.get(deviceId) ?? []).reduce(
        (n, m) => n + (m.type === "log" ? m.entries.length : 0),
        0,
      );
      live.push({
        id: s.id,
        device_id: deviceId,
        device_name: d.name,
        model_name: d.model_name,
        platform: d.platform,
        started_at_ms: s.started_at_ms,
        ended_at_ms: s.status === "stopped" ? s.started_at_ms + s.elapsed_ms : null,
        elapsed_ms: s.elapsed_ms,
        sample_count: Math.max(s.sample_count, rows.length),
        log_count: Math.max(s.log_count, logs),
        event_count: this.events.get(deviceId)?.length ?? 0,
        mark_count: s.marks.length,
        size_bytes: rows.length * 1_900 + logs * 160,
        live: s.status === "recording" || s.status === "paused",
      });
    }
    return [...live, ...this.storedSessions()];
  }

  private findSession(id: string): SessionSummary | undefined {
    return this.listSessions().find((x) => x.id === id);
  }

  private infoOf(deviceId: string): DeviceInfo | null {
    const m = (this.sticky.get(deviceId) ?? []).find((x) => x.type === "info");
    return m && m.type === "info" ? m.info : null;
  }

  private openSession(id: string): SessionDetail | null {
    const summary = this.findSession(id);
    if (!summary) return null;
    if (summary.live) {
      const s = this.sessions.get(summary.device_id);
      const metrics = new Set<MetricId>();
      for (const r of this.samples.get(summary.device_id) ?? []) {
        for (const k of Object.keys(r.values)) metrics.add(k as MetricId);
      }
      return {
        summary,
        info: this.infoOf(summary.device_id),
        marks: s?.marks ?? [],
        events: this.events.get(summary.device_id) ?? [],
        metrics: [...metrics].sort(),
      };
    }
    return {
      summary,
      info: this.infoOf(summary.device_id),
      marks: storedMarks(summary),
      events: storedEvents(summary),
      metrics: STORED_METRICS.map((m) => m.id),
    };
  }

  private queryHistory(q: HistoryQuery): HistoryPoint[] {
    const stored = q.session_id ? this.storedSessions().find((x) => x.id === q.session_id) : null;
    if (stored) {
      const step = Math.max(1000, q.step_ms || 1000);
      const from = Math.max(q.from_ms, stored.started_at_ms);
      const to = Math.min(q.to_ms, stored.started_at_ms + stored.elapsed_ms);
      const out: HistoryPoint[] = [];
      for (let t = from; t <= to; t += step) {
        out.push({ ts_ms: t, value: storedValue(stored, q.metric, t) });
      }
      return out;
    }
    const rows = this.samples.get(q.device_id) ?? [];
    const hits = rows.filter(
      (r) => r.ts_ms >= q.from_ms && r.ts_ms <= q.to_ms && q.metric in r.values,
    );
    const step = q.step_ms > 0 ? q.step_ms : 0;
    const out: HistoryPoint[] = [];
    let bucket = Number.NEGATIVE_INFINITY;
    for (const r of hits) {
      if (step > 0 && r.ts_ms - bucket < step) continue;
      bucket = r.ts_ms;
      out.push({ ts_ms: r.ts_ms, value: r.values[q.metric] });
    }
    return out;
  }

  /** Three progress messages, then the result; the path is the one the UI chose. */
  private async exportData(
    request: ExportRequest,
    emit: (m: Message) => void,
  ): Promise<ExportResult> {
    this.exportSeq += 1;
    const task_id = `export-${this.exportSeq}`;
    const steps: [number, string][] = [
      [10, `export: collecting ${request.format} (${request.scope.scope})`],
      [60, "export: writing"],
      [100, "export: done"],
    ];
    for (const [pct, message] of steps) {
      await delay(EXPORT_STEP_MS);
      emit({ type: "progress", task_id, pct, message });
    }
    // Let the 100 % frame show before the result replaces the bar.
    await delay(EXPORT_STEP_MS);
    return { path: request.path, bytes: 12_345, entries: 100 };
  }

  private session(deviceId: string): SessionState {
    return (
      this.sessions.get(deviceId) ?? {
        id: null,
        status: "idle",
        started_at_ms: null,
        elapsed_ms: 0,
        sample_count: 0,
        log_count: 0,
        marks: [],
      }
    );
  }

  private setSession(
    deviceId: string | undefined,
    status: SessionState["status"],
    emit: (m: Message) => void,
  ): SessionState | null {
    if (!deviceId) return null;
    const s = { ...this.session(deviceId), status };
    if (status === "recording" && s.started_at_ms === null) {
      s.started_at_ms = Date.now();
      s.id = `replay-${deviceId}-${s.started_at_ms}`;
    }
    this.sessions.set(deviceId, s);
    emit({ type: "session", device_id: deviceId, session: s });
    return s;
  }

  private queryLogs(filter: LogFilter, offset: number, limit: number): LogPage {
    if (filter.session_id && this.storedSessions().some((x) => x.id === filter.session_id)) {
      // Stored synthetic sessions carry no log lines in the browser replay.
      return { total: 0, first_seq: 0, entries: [] };
    }
    const all = (this.logs.get(filter.device_id) ?? []).flatMap((m) =>
      m.type === "log" ? m.entries : [],
    );
    const re = filter.query ? (filter.regex ? safeRegex(filter.query) : null) : null;
    const q = filter.query?.toLowerCase();
    const hit = all.filter(
      (e) =>
        (filter.processes.length === 0 || filter.processes.includes(e.process)) &&
        (filter.levels.length === 0 || filter.levels.includes(e.level)) &&
        (filter.from_ms == null || e.ts_ms >= filter.from_ms) &&
        (filter.to_ms == null || e.ts_ms <= filter.to_ms) &&
        (!q || (re ? re.test(e.message) : e.message.toLowerCase().includes(q))),
    );
    const page = hit.slice(offset, offset + limit);
    return { total: hit.length, first_seq: page[0]?.seq ?? 0, entries: page };
  }
}

function safeRegex(src: string): RegExp | null {
  try {
    return new RegExp(src, "i");
  } catch {
    return null;
  }
}
