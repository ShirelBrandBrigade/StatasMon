import "./NetworkCard.css";
import { formatValue, type Unit, unitSuffix } from "@statasmon/protocol";
import { Card, Dash, MetricFigure, seriesColor } from "@statasmon/ui";
import type { ReactNode } from "react";
import { type ChartSeries, TimeSeriesChart } from "../../charts/TimeSeriesChart";
import { useSeriesData } from "../../charts/useSeriesData";
import { useCapability, useLatest, useRangeMs } from "../../hooks/useDevice";

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

  const subtitle = (
    <span data-testid="net-subtitle">
      Wi-Fi · {rssi === undefined ? <Dash reason={why(wifi)} /> : minus(formatValue(DBM, rssi))} ·{" "}
      {latency === undefined ? <Dash reason={why(throughput)} /> : formatValue(MS, latency)}
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
