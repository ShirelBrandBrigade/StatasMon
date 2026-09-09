import type {
  DeviceSummary,
  HistoryQuery,
  LogFilter,
  LogPage,
  Message,
  SessionState,
} from "@statasmon/protocol";
import type { DeviceStream } from "./stream";
import { synthesize } from "./synthetic";

declare const __FIXTURE_DIR__: string;

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
        emit(state.rebase(items[i].message, loop * loopMs));
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
  rebase(m: Message, offset: number): Message {
    if (offset === 0) return m;
    if ("ts_ms" in m) return { ...m, ts_ms: m.ts_ms + offset } as Message;
    if (m.type === "log")
      return { ...m, entries: m.entries.map((e) => ({ ...e, ts_ms: e.ts_ms + offset })) };
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
      case "query_history": {
        const q = args.query as HistoryQuery;
        const rows = this.samples.get(q.device_id) ?? [];
        return rows
          .filter((r) => r.ts_ms >= q.from_ms && r.ts_ms <= q.to_ms && q.metric in r.values)
          .map((r) => ({ ts_ms: r.ts_ms, value: r.values[q.metric] }));
      }
      default:
        return null;
    }
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
