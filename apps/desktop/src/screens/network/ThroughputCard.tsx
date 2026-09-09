import { formatValue, type Unit, unitSuffix } from "@statasmon/protocol";
import { Card, Dash, MetricFigure, seriesColor } from "@statasmon/ui";
import type { ReactNode } from "react";
import { type ChartSeries, TimeSeriesChart, useSeriesData } from "../../charts";
import { useCapability, useLatest, useRangeMs } from "../../hooks/useDevice";
import { Subtitle } from "../../overview/cards/shared";
import { useMarkers } from "../performance/shared";
import { why } from "./why";

export const THROUGHPUT_CHART_HEIGHT = 180;
export const PACKETS_CHART_HEIGHT = 90;

const BPS = { unit: "bits_per_sec" } as const satisfies { unit: Unit };
const BYTES = { unit: "bytes" } as const satisfies { unit: Unit };
const COUNT = { unit: "count" } as const satisfies { unit: Unit };
const PPS = unitSuffix("packets_per_sec");

/** Down first (cyan), up second (violet): the fixed series order of the design system. */
const BPS_SERIES: ChartSeries[] = [
  { id: "net.rx_bps", label: "Down", color: seriesColor("cyan"), fill: true, endpoint: true },
  { id: "net.tx_bps", label: "Up", color: seriesColor("vio") },
];
const PPS_SERIES: ChartSeries[] = [
  { id: "net.rx_pps", label: "Packets in", color: seriesColor("cyan"), fill: true, endpoint: true },
  { id: "net.tx_pps", label: "Packets out", color: seriesColor("vio") },
];

const formatBps = (v: number) => formatValue(BPS, v);
const formatPps = (v: number) => `${formatValue(COUNT, v)}${PPS}`;

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

/** Throughput: down/up figures with packets and totals, a tall bit/s chart and a packets/s chart. */
export function ThroughputCard({ deviceId }: { deviceId: string }) {
  const rx = useLatest(deviceId, "net.rx_bps");
  const tx = useLatest(deviceId, "net.tx_bps");
  const rxPps = useLatest(deviceId, "net.rx_pps");
  const txPps = useLatest(deviceId, "net.tx_pps");
  const rxTotal = useLatest(deviceId, "net.rx_total");
  const txTotal = useLatest(deviceId, "net.tx_total");
  const throughput = useCapability(deviceId, "network_throughput");
  const packets = useCapability(deviceId, "network_packets");
  const windowMs = useRangeMs();
  const bps = useSeriesData(deviceId, "net.rx_bps", "net.tx_bps");
  const pps = useSeriesData(deviceId, "net.rx_pps", "net.tx_pps");
  const markers = useMarkers(deviceId);

  const totals =
    rxTotal !== undefined || txTotal !== undefined
      ? `${formatValue(BYTES, rxTotal)} down · ${formatValue(BYTES, txTotal)} up this session`
      : undefined;

  return (
    <Card
      title="Throughput"
      className="net-card net-throughput"
      data-testid="net-throughput"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[
            totals,
            bps.loadingHistory || pps.loadingHistory ? "loading history…" : undefined,
          ]}
        />
      }
    >
      <div className="net-figures">
        <div className="net-figure" data-testid="net-down">
          <MetricFigure
            meta={BPS}
            value={rx}
            suffix="down"
            series="cyan"
            size="lg"
            unavailableReason={why(throughput)}
          />
          <div className="net-caption mono">
            <Value value={rxPps} unit="count" suffix={PPS} reason={why(packets)} />
          </div>
        </div>
        <div className="net-figure" data-testid="net-up">
          <MetricFigure
            meta={BPS}
            value={tx}
            suffix="up"
            series="vio"
            size="lg"
            unavailableReason={why(throughput)}
          />
          <div className="net-caption mono">
            <Value value={txPps} unit="count" suffix={PPS} reason={why(packets)} />
          </div>
        </div>
      </div>
      {throughput.available === false ? (
        <div
          className="net-chart-off"
          style={{ height: THROUGHPUT_CHART_HEIGHT }}
          data-testid="net-chart-off"
        >
          <Dash reason={why(throughput)} />
          <span>{why(throughput)}</span>
        </div>
      ) : (
        <TimeSeriesChart
          name="Download and upload throughput over time"
          series={BPS_SERIES}
          markers={markers}
          data={bps}
          height={THROUGHPUT_CHART_HEIGHT}
          windowMs={windowMs}
          yFormat={formatBps}
          unit="bits per second"
        />
      )}
      <div className="section-label">Packets</div>
      {packets.available === false ? (
        <div
          className="net-chart-off"
          style={{ height: PACKETS_CHART_HEIGHT }}
          data-testid="net-pps-off"
        >
          <Dash reason={why(packets)} />
          <span>{why(packets)}</span>
        </div>
      ) : (
        <TimeSeriesChart
          name="Packets in and out per second over time"
          series={PPS_SERIES}
          data={pps}
          height={PACKETS_CHART_HEIGHT}
          windowMs={windowMs}
          yFormat={formatPps}
          unit="packets per second"
        />
      )}
    </Card>
  );
}
