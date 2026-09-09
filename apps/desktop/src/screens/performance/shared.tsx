import type { EventKind, MetricId, StoredEvent } from "@statasmon/protocol";
import { Dash, Pill, type Series, seriesColor, type Tone } from "@statasmon/ui";
import { useMemo } from "react";
import { type ChartMarker, type ChartSeries, formatClock, TimeSeriesChart } from "../../charts";
import { useDevice, useRangeMs } from "../../hooks/useDevice";
import { chartMarkers } from "../../overview/network/NetworkCard";
import { useMultiSeriesData } from "./useMultiSeries";

/** Series colors in the fixed order of the design system; a fifth series is "Other". */
export const SERIES_ORDER: readonly Series[] = ["cyan", "vio", "mint", "amber", "other"];

export const CHART_HEIGHT = 150;
export const SMALL_CHART_HEIGHT = 70;

export interface ChartMetric {
  id: MetricId;
  label: string;
  /** Gradient area under the line. */
  fill?: boolean;
  /** Endpoint dot on the newest point. */
  endpoint?: boolean;
  /** Overrides the positional series color (e.g. amber for the battery). */
  series?: Series;
}

export interface MetricChartProps {
  deviceId: string;
  metrics: ChartMetric[];
  height: number;
  yFormat: (value: number) => string;
  yMin?: number | "auto";
  name: string;
  unit?: string;
  markers?: ChartMarker[];
  /** Why the chart is empty; drawn as a dashed box with the reason instead of the chart. */
  offReason?: string;
  testId?: string;
}

/** Dashed placeholder where a chart would be, carrying the reason the platform gives none. */
export function ChartOff({
  reason,
  height,
  testId,
}: {
  reason: string;
  height: number;
  testId?: string;
}) {
  return (
    <div className="perf-chart-off" style={{ height }} data-testid={testId}>
      <Dash reason={reason} />
      <span>{reason}</span>
    </div>
  );
}

/**
 * One `TimeSeriesChart` over N metrics of the device, on the global range,
 * with stored history merged in. Colors follow `SERIES_ORDER` unless a metric
 * names its own series.
 */
export function MetricChart({
  deviceId,
  metrics,
  height,
  yFormat,
  yMin = 0,
  name,
  unit,
  markers,
  offReason,
  testId,
}: MetricChartProps) {
  const windowMs = useRangeMs();
  const ids = useMemo(() => metrics.map((m) => m.id), [metrics]);
  const data = useMultiSeriesData(deviceId, ids);
  const series = useMemo<ChartSeries[]>(
    () =>
      metrics.map((m, i) => ({
        id: m.id,
        label: m.label,
        color: seriesColor(m.series ?? SERIES_ORDER[Math.min(i, SERIES_ORDER.length - 1)]),
        fill: m.fill,
        endpoint: m.endpoint,
      })),
    [metrics],
  );
  if (offReason !== undefined)
    return <ChartOff reason={offReason} height={height} testId={testId} />;
  return (
    <div className="perf-chart" data-testid={testId}>
      <TimeSeriesChart
        name={name}
        series={series}
        markers={markers}
        data={data}
        height={height}
        windowMs={windowMs}
        yFormat={yFormat}
        yMin={yMin}
        unit={unit}
      />
      {data.loadingHistory ? (
        <span className="perf-history-note" data-testid="perf-history-note">
          loading history…
        </span>
      ) : null}
    </div>
  );
}

/** Marks and interesting events of the device as chart markers (same set as the Overview). */
export function useMarkers(deviceId: string): ChartMarker[] {
  const entry = useDevice(deviceId);
  const marks = entry?.session?.marks;
  const events = entry?.events;
  return useMemo(() => chartMarkers(marks ?? [], events ?? []), [marks, events]);
}

/** Local wall-clock `HH:MM:SS` for an event timestamp. */
export function formatTime(tsMs: number): string {
  return formatClock(tsMs / 1000, true);
}

export const EVENT_LABEL: Partial<Record<EventKind, { label: string; tone: Tone }>> = {
  memory_warning: { label: "Memory warning", tone: "warn" },
  jetsam: { label: "Jetsam", tone: "bad" },
  lmk_kill: { label: "LMK kill", tone: "bad" },
  thermal_change: { label: "Thermal", tone: "warn" },
};

/** The newest `limit` events of the given kinds, newest first. */
export function recentEvents(
  events: readonly StoredEvent[],
  kinds: readonly EventKind[],
  limit: number,
): StoredEvent[] {
  const wanted = new Set<EventKind>(kinds);
  const hits = events.filter((e) => wanted.has(e.kind));
  return hits.slice(-limit).reverse();
}

export interface EventListProps {
  events: StoredEvent[];
  /** Shown when there are no events and no reason blocks them. */
  emptyText: string;
  /** Why events cannot arrive (capability unavailable); wins over `emptyText`. */
  offReason?: string;
  name: string;
  testId?: string;
}

/** Newest-first rows: time · kind pill · detail · process. */
export function EventList({ events, emptyText, offReason, name, testId }: EventListProps) {
  if (offReason !== undefined) {
    return (
      <div className="perf-empty" data-testid={testId}>
        <span>{name}</span>
        <Dash reason={offReason} />
      </div>
    );
  }
  if (events.length === 0) {
    return (
      <div className="perf-empty" data-testid={testId}>
        {emptyText}
      </div>
    );
  }
  return (
    <ul className="perf-events" aria-label={name} data-testid={testId}>
      {events.map((e) => {
        const meta = EVENT_LABEL[e.kind] ?? { label: e.kind, tone: "neutral" as Tone };
        return (
          <li key={`${e.ts_ms}-${e.kind}-${e.pid ?? ""}`}>
            <time className="mono" dateTime={new Date(e.ts_ms).toISOString()}>
              {formatTime(e.ts_ms)}
            </time>
            <Pill tone={meta.tone} size="sm">
              {meta.label}
            </Pill>
            <span className="perf-ev-detail" title={e.detail}>
              {e.detail}
            </span>
            {e.process ? <span className="perf-ev-proc mono">{e.process}</span> : null}
          </li>
        );
      })}
    </ul>
  );
}
