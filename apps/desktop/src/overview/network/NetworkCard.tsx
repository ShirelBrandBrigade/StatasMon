import "./NetworkCard.css";
import {
  type EventKind,
  formatValue,
  type Mark,
  type StoredEvent,
  type Unit,
  unitSuffix,
} from "@statasmon/protocol";
import { Card, Dash, MetricFigure, seriesColor } from "@statasmon/ui";
import { type ReactNode, useMemo } from "react";
import {
  type ChartMarker,
  type ChartSeries,
  type MarkerTone,
  TimeSeriesChart,
} from "../../charts/TimeSeriesChart";
import { useSeriesData } from "../../charts/useSeriesData";
import { useCapability, useDevice, useLatest, useRangeMs } from "../../hooks/useDevice";

const CHART_HEIGHT = 90;
const BPS = { unit: "bits_per_sec" } as const;
const DBM = { unit: "dbm" } as const;
const MS = { unit: "milliseconds" } as const;
const PPS = unitSuffix("packets_per_sec");

/** Down first (cyan), up second (violet): the fixed series order of the design system. */
const SERIES: ChartSeries[] = [
  { id: "net.rx_bps", label: "Down", color: seriesColor("cyan"), fill: true, endpoint: true },
  { id: "net.tx_bps", label: "Up", color: seriesColor("vio") },
];

const formatBps = (v: number) => formatValue(BPS, v);

/** Events that earn a line on the chart, with their tone (docs/PLAN.md 5.3). */
const EVENT_TONE: Partial<Record<EventKind, MarkerTone>> = {
  thermal_change: "warn",
  memory_warning: "warn",
  jetsam: "warn",
  lmk_kill: "warn",
  crash: "bad",
  anr: "bad",
  mark: "info",
};

const EVENT_LABEL: Partial<Record<EventKind, string>> = {
  thermal_change: "Thermal",
  memory_warning: "Memory",
  jetsam: "Jetsam",
  lmk_kill: "LMK kill",
  crash: "Crash",
  anr: "ANR",
};

/** Marks (info) and the interesting events as chart markers, oldest → newest. */
export function chartMarkers(
  marks: readonly Mark[],
  events: readonly StoredEvent[],
): ChartMarker[] {
  const out: ChartMarker[] = marks.map((m) => ({ tsMs: m.ts_ms, label: m.label, tone: "info" }));
  for (const e of events) {
    const tone = EVENT_TONE[e.kind];
    if (!tone) continue;
    const label = e.kind === "mark" ? e.detail : (EVENT_LABEL[e.kind] ?? e.kind);
    out.push({ tsMs: e.ts_ms, label: e.process ? `${label} · ${e.process}` : label, tone });
  }
  return out.sort((a, b) => a.tsMs - b.tsMs);
}

/** Typographic minus for negative readings (dBm). */
const minus = (text: string) => text.replace(/^-/, "−");

type Cap = ReturnType<typeof useCapability>;

/** Why a value is missing: the capability's reason, or the honest interim states. */
function why(cap: Cap): string {
  if (cap.available === false) return cap.reason ?? "Not available on this device";
  if (cap.available === undefined) return "Waiting for the device's capability list";
  return "Not reported yet";
}

function Value({
  value,
  unit,
  suffix = "",
  reason,
}: {
  value: number | undefined;
  unit: Unit;
  suffix?: string;
  reason: string;
}): ReactNode {
  if (value === undefined) return <Dash reason={reason} />;
  return `${formatValue({ unit }, value)}${suffix}`;
}

/** Network card: down/up figures, packets and totals, side counters and the throughput area chart. */
export function NetworkCard({ deviceId }: { deviceId: string }) {
  const rx = useLatest(deviceId, "net.rx_bps");
  const tx = useLatest(deviceId, "net.tx_bps");
  const rxPps = useLatest(deviceId, "net.rx_pps");
  const txPps = useLatest(deviceId, "net.tx_pps");
  const rxTotal = useLatest(deviceId, "net.rx_total");
  const txTotal = useLatest(deviceId, "net.tx_total");
  const rssi = useLatest(deviceId, "net.rssi");
  const latency = useLatest(deviceId, "net.latency_ms");
  const dropped = useLatest(deviceId, "net.dropped");
  const retransmits = useLatest(deviceId, "net.retransmits");
  const connections = useLatest(deviceId, "net.connections");

  const throughput = useCapability(deviceId, "network_throughput");
  const packets = useCapability(deviceId, "network_packets");
  const wifi = useCapability(deviceId, "wifi_details");
  const perProcess = useCapability(deviceId, "network_per_process");

  const windowMs = useRangeMs();
  const data = useSeriesData(deviceId, "net.rx_bps", "net.tx_bps");
  const entry = useDevice(deviceId);
  const marks = entry?.session?.marks;
  const events = entry?.events;
  const markers = useMemo(() => chartMarkers(marks ?? [], events ?? []), [marks, events]);

  const subtitle = (
    <span data-testid="net-subtitle">
      Wi-Fi · {rssi === undefined ? <Dash reason={why(wifi)} /> : minus(formatValue(DBM, rssi))} ·{" "}
      {latency === undefined ? <Dash reason={why(throughput)} /> : formatValue(MS, latency)}
      {data.loadingHistory ? <span data-testid="net-history-note"> · loading history…</span> : null}
    </span>
  );

  return (
    <Card title="Network" subtitle={subtitle} className="sm-net-card" data-testid="network-card">
      <div className="sm-net-figures">
        <div className="sm-net-figure" data-testid="net-down">
          <MetricFigure
            meta={BPS}
            value={rx}
            suffix="down"
            series="cyan"
            size="lg"
            unavailableReason={why(throughput)}
          />
          <div className="sm-figure-caption mono" data-testid="net-down-caption">
            <Value value={rxPps} unit="count" suffix={PPS} reason={why(packets)} /> ·{" "}
            <Value value={rxTotal} unit="bytes" reason={why(throughput)} />
          </div>
        </div>
        <div className="sm-net-figure" data-testid="net-up">
          <MetricFigure
            meta={BPS}
            value={tx}
            suffix="up"
            series="vio"
            size="lg"
            unavailableReason={why(throughput)}
          />
          <div className="sm-figure-caption mono" data-testid="net-up-caption">
            <Value value={txPps} unit="count" suffix={PPS} reason={why(packets)} /> ·{" "}
            <Value value={txTotal} unit="bytes" reason={why(throughput)} />
          </div>
        </div>
        <div className="sm-net-side" data-testid="net-side">
          <div>
            <span className="mono">
              <Value value={dropped} unit="count" reason={why(perProcess)} />
            </span>{" "}
            dropped ·{" "}
            <span className="mono">
              <Value value={retransmits} unit="count" reason={why(perProcess)} />
            </span>{" "}
            retransmits
          </div>
          <div>
            <span className="mono">
              <Value value={connections} unit="count" reason={why(perProcess)} />
            </span>{" "}
            connections
          </div>
        </div>
      </div>
      {throughput.available === false ? (
        <div
          className="sm-net-chart-off"
          style={{ height: CHART_HEIGHT }}
          data-testid="net-chart-off"
        >
          <Dash reason={why(throughput)} />
          <span>{why(throughput)}</span>
        </div>
      ) : (
        <TimeSeriesChart
          className="sm-net-chart"
          name="Download and upload throughput over time"
          series={SERIES}
          markers={markers}
          data={data}
          height={CHART_HEIGHT}
          windowMs={windowMs}
          yFormat={formatBps}
          unit="bits per second"
        />
      )}
    </Card>
  );
}
