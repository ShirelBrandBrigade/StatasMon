import type {
  DeviceInfo,
  EventKind,
  HistoryPoint,
  Mark,
  MetricId,
  Platform,
  SessionSummary,
  StoredEvent,
  Unit,
} from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import type { IconName, Tone } from "@statasmon/ui";
import type { ChartMarker, MarkerTone } from "../charts/TimeSeriesChart";
import { alignSeries, type ChartData } from "../charts/useSeriesData";

/** Newest first (started_at desc); live sessions first among equals. */
export function sortSessions(list: readonly SessionSummary[]): SessionSummary[] {
  return [...list].sort(
    (a, b) => b.started_at_ms - a.started_at_ms || Number(b.live) - Number(a.live),
  );
}

export function platformIcon(platform: Platform, modelName: string): IconName {
  if (platform === "android") return "android";
  if (platform === "ios") return /iphone/i.test(modelName) ? "phone" : "tablet";
  return "device";
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/** `2026-09-08 09:46` in local time. */
export function formatStart(tsMs: number): string {
  const d = new Date(tsMs);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

/** `2026-09-08` (used in the compare legend). */
export function formatDate(tsMs: number): string {
  const d = new Date(tsMs);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** Labels and units for the metrics a session can carry (the core's catalog is the source; this mirrors it for the UI). */
const METRIC_INFO: Partial<Record<MetricId, { label: string; unit: Unit }>> = {
  "cpu.total": { label: "CPU total", unit: "percent" },
  "cpu.user": { label: "CPU user", unit: "percent" },
  "cpu.system": { label: "CPU system", unit: "percent" },
  "cpu.load1": { label: "Load 1 min", unit: "load" },
  "cpu.load5": { label: "Load 5 min", unit: "load" },
  "mem.total": { label: "Memory total", unit: "bytes" },
  "mem.used": { label: "Memory used", unit: "bytes" },
  "mem.free": { label: "Memory free", unit: "bytes" },
  "mem.app": { label: "Memory app", unit: "bytes" },
  "mem.wired": { label: "Memory wired", unit: "bytes" },
  "mem.compressed": { label: "Memory compressed", unit: "bytes" },
  "mem.cached": { label: "Memory cached", unit: "bytes" },
  "mem.swap": { label: "Swap", unit: "bytes" },
  "mem.pressure": { label: "Memory pressure", unit: "state" },
  "thermal.state": { label: "Thermal state", unit: "state" },
  "thermal.raw": { label: "Thermal raw", unit: "state" },
  "temp.battery": { label: "Battery temperature", unit: "celsius" },
  "temp.soc": { label: "SoC temperature", unit: "celsius" },
  "temp.skin": { label: "Skin temperature", unit: "celsius" },
  "temp.pmu": { label: "PMU temperature", unit: "celsius" },
  "temp.display": { label: "Display temperature", unit: "celsius" },
  "bat.percent": { label: "Battery level", unit: "percent" },
  "bat.voltage": { label: "Battery voltage", unit: "volts" },
  "bat.current": { label: "Battery current", unit: "amps" },
  "bat.power": { label: "Battery power", unit: "watts" },
  "bat.temp": { label: "Battery temperature", unit: "celsius" },
  "bat.cycles": { label: "Battery cycles", unit: "count" },
  "bat.health": { label: "Battery health", unit: "percent" },
  "bat.charging": { label: "Charging", unit: "bool" },
  "disk.total": { label: "Storage total", unit: "bytes" },
  "disk.used": { label: "Storage used", unit: "bytes" },
  "disk.free": { label: "Storage free", unit: "bytes" },
  "disk.write_bps": { label: "Disk writes", unit: "bytes_per_sec" },
  "net.rx_bps": { label: "Download", unit: "bits_per_sec" },
  "net.tx_bps": { label: "Upload", unit: "bits_per_sec" },
  "net.rx_pps": { label: "Packets in", unit: "packets_per_sec" },
  "net.tx_pps": { label: "Packets out", unit: "packets_per_sec" },
  "net.rx_total": { label: "Received total", unit: "bytes" },
  "net.tx_total": { label: "Sent total", unit: "bytes" },
  "net.rssi": { label: "Wi-Fi signal", unit: "dbm" },
  "net.link_mbps": { label: "Link speed", unit: "mbps" },
  "net.connections": { label: "Connections", unit: "count" },
  "net.retransmits": { label: "Retransmits", unit: "count" },
  "net.dropped": { label: "Dropped", unit: "count" },
  "net.latency_ms": { label: "Latency", unit: "milliseconds" },
  "proc.count": { label: "Processes", unit: "count" },
};

export function metricLabel(id: MetricId): string {
  return METRIC_INFO[id]?.label ?? id;
}

export function metricUnit(id: MetricId): Unit | undefined {
  return METRIC_INFO[id]?.unit;
}

export function metricFormatter(id: MetricId): (v: number) => string {
  const unit = metricUnit(id);
  return unit ? (v) => formatValue({ unit }, v) : (v) => formatValue({ unit: "count" }, v);
}

/** The metric a reopened session shows first: CPU when recorded, else the first one. */
export function defaultMetric(metrics: readonly MetricId[]): MetricId | undefined {
  return metrics.includes("cpu.total") ? "cpu.total" : metrics[0];
}

const EVENT_TONE: Record<EventKind, Tone> = {
  thermal_change: "warn",
  memory_warning: "warn",
  lmk_kill: "warn",
  jetsam: "warn",
  crash: "bad",
  anr: "bad",
  app_launch: "neutral",
  app_exit: "neutral",
  mark: "info",
  connection: "neutral",
};

const EVENT_LABEL: Record<EventKind, string> = {
  thermal_change: "Thermal",
  memory_warning: "Memory",
  lmk_kill: "LMK kill",
  jetsam: "Jetsam",
  crash: "Crash",
  anr: "ANR",
  app_launch: "Launch",
  app_exit: "Exit",
  mark: "Mark",
  connection: "Connection",
};

/** Events that earn a line on a chart (docs/PLAN.md 5.3); the rest stay in the timeline only. */
const EVENT_MARKER_TONE: Partial<Record<EventKind, MarkerTone>> = {
  thermal_change: "warn",
  memory_warning: "warn",
  lmk_kill: "warn",
  jetsam: "warn",
  crash: "bad",
  anr: "bad",
  mark: "info",
};

export function eventTone(kind: EventKind): Tone {
  return EVENT_TONE[kind];
}

export function eventLabel(kind: EventKind): string {
  return EVENT_LABEL[kind];
}

/** One row of the marks + events timeline, oldest → newest. */
export interface TimelineItem {
  key: string;
  tsMs: number;
  kind: EventKind;
  label: string;
  detail: string;
  process: string | null;
}

export function timeline(marks: readonly Mark[], events: readonly StoredEvent[]): TimelineItem[] {
  const items: TimelineItem[] = marks.map((m, i) => ({
    key: `mark-${i}-${m.ts_ms}`,
    tsMs: m.ts_ms,
    kind: "mark",
    label: "Mark",
    detail: m.label,
    process: null,
  }));
  events.forEach((e, i) => {
    items.push({
      key: `event-${i}-${e.ts_ms}`,
      tsMs: e.ts_ms,
      kind: e.kind,
      label: eventLabel(e.kind),
      detail: e.detail,
      process: e.process,
    });
  });
  return items.sort((a, b) => a.tsMs - b.tsMs);
}

/** Marks (info) and events (by tone; connection/launch/exit stay off the chart) as chart markers. */
export function sessionMarkers(
  marks: readonly Mark[],
  events: readonly StoredEvent[],
  offsetMs = 0,
): ChartMarker[] {
  const out: ChartMarker[] = marks.map((m) => ({
    tsMs: m.ts_ms - offsetMs,
    label: m.label,
    tone: "info",
  }));
  for (const e of events) {
    const tone = EVENT_MARKER_TONE[e.kind];
    if (!tone) continue;
    out.push({ tsMs: e.ts_ms - offsetMs, label: eventLabel(e.kind), tone });
  }
  return out.sort((a, b) => a.tsMs - b.tsMs);
}

/** History points of one session as chart input on the absolute time axis. */
export function toChartData(points: readonly HistoryPoint[]): ChartData {
  return { t: points.map((p) => p.ts_ms), v: [points.map((p) => p.value)] };
}

export interface ElapsedInput {
  startedAtMs: number;
  points: readonly HistoryPoint[];
}

/**
 * Compare mode (docs/PLAN.md 5.5): both sessions on one axis of elapsed time
 * from each session's own start (`t − started_at`), so the "timestamps" of
 * the returned data are milliseconds since start. Points before a session's
 * start are dropped; timelines that do not line up get `NaN` gaps.
 */
export function alignToElapsed(a: ElapsedInput, b?: ElapsedInput): ChartData {
  const toInput = (x: ElapsedInput) => {
    const t: number[] = [];
    const v: number[] = [];
    for (const p of x.points) {
      const e = p.ts_ms - x.startedAtMs;
      if (e < 0) continue;
      t.push(e);
      v.push(p.value);
    }
    return { t, v };
  };
  const inputs = b ? [toInput(a), toInput(b)] : [toInput(a)];
  const out = alignSeries(inputs, { t: [], v: [] });
  return { t: out.t, v: out.v };
}

/** Rows of the read-only device sheet summary (docs/FEATURES.md §1, the short form). */
export function deviceSheetRows(info: DeviceInfo | null): { label: string; value: string }[] {
  if (!info) return [];
  const { identity, software, hardware } = info;
  const rows: { label: string; value: string | null }[] = [
    { label: "Name", value: identity.name },
    { label: "Model", value: identity.marketing_name ?? identity.model_identifier },
    { label: "Identifier", value: identity.model_identifier },
    {
      label: "OS",
      value:
        software.os_name || software.os_version
          ? `${software.os_name ?? ""} ${software.os_version ?? ""}${software.os_build ? ` (${software.os_build})` : ""}`.trim()
          : null,
    },
    { label: "Chip", value: hardware.chip_name ?? hardware.chip_identifier },
    { label: "CPU cores", value: hardware.cpu_cores === null ? null : String(hardware.cpu_cores) },
    {
      label: "RAM",
      value:
        hardware.ram_bytes === null ? null : formatValue({ unit: "bytes" }, hardware.ram_bytes),
    },
    {
      label: "Storage",
      value:
        hardware.storage_bytes === null
          ? null
          : formatValue({ unit: "bytes" }, hardware.storage_bytes),
    },
  ];
  return rows.flatMap((r) => (r.value ? [{ label: r.label, value: r.value }] : []));
}
