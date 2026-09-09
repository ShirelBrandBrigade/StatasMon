import type { HistoryPoint, Mark, MetricId, StoredEvent } from "@statasmon/protocol";
import { Legend, seriesColor } from "@statasmon/ui";
import { useMemo } from "react";
import { type ChartSeries, formatElapsedAxis, TimeSeriesChart } from "../charts/TimeSeriesChart";
import {
  alignToElapsed,
  formatDate,
  metricFormatter,
  metricLabel,
  sessionMarkers,
} from "./sessionsModel";

export interface CompareSide {
  sessionId: string;
  deviceName: string;
  startedAtMs: number;
  points: HistoryPoint[];
  marks?: Mark[];
  events?: StoredEvent[];
}

export interface CompareChartProps {
  metric: MetricId;
  a: CompareSide;
  b: CompareSide;
  height?: number;
}

/**
 * Two sessions, one metric, x = elapsed time from each session's own start.
 * A is cyan, B is violet (the fixed series order of the design system).
 */
export function CompareChart({ metric, a, b, height = 220 }: CompareChartProps) {
  const label = metricLabel(metric);
  const legendA = `A · ${a.deviceName} ${formatDate(a.startedAtMs)}`;
  const legendB = `B · ${b.deviceName} ${formatDate(b.startedAtMs)}`;
  const series = useMemo<ChartSeries[]>(
    () => [
      { id: `a-${a.sessionId}`, label: legendA, color: seriesColor("cyan"), fill: true },
      { id: `b-${b.sessionId}`, label: legendB, color: seriesColor("vio") },
    ],
    [a.sessionId, b.sessionId, legendA, legendB],
  );
  const data = useMemo(
    () =>
      alignToElapsed(
        { startedAtMs: a.startedAtMs, points: a.points },
        { startedAtMs: b.startedAtMs, points: b.points },
      ),
    [a.startedAtMs, a.points, b.startedAtMs, b.points],
  );
  const markers = useMemo(
    () => sessionMarkers(a.marks ?? [], a.events ?? [], a.startedAtMs),
    [a.marks, a.events, a.startedAtMs],
  );
  const fmt = useMemo(() => metricFormatter(metric), [metric]);
  const last = (p: HistoryPoint[]) => (p.length > 0 ? fmt(p[p.length - 1].value) : "—");
  return (
    <div className="flex flex-col gap-2" data-testid="compare-chart">
      <TimeSeriesChart
        name={`${label}: ${legendA} and ${legendB}, by elapsed time`}
        series={series}
        markers={markers}
        data={data}
        height={height}
        yFormat={fmt}
        xFormat={formatElapsedAxis}
        yMin="auto"
      />
      <Legend
        items={[
          { label: legendA, value: last(a.points), series: "cyan" },
          { label: legendB, value: last(b.points), series: "vio" },
        ]}
      />
    </div>
  );
}
