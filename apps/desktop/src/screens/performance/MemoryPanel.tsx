import type { EventKind, MetricId } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { Card, Dash, Legend, Pill, type Series, StackBar, type Tone } from "@statasmon/ui";
import { useMemo } from "react";
import { useCapability, useDevice, useLatest } from "../../hooks/useDevice";
import {
  BYTES,
  capabilityReason,
  DetailDash,
  Subtitle,
  useMetric,
} from "../../overview/cards/shared";
import {
  CHART_HEIGHT,
  type ChartMetric,
  EventList,
  MetricChart,
  recentEvents,
  useMarkers,
} from "./shared";

/** `mem.pressure` 0–3 → label and tone (label is always shown beside the color). */
export const PRESSURE_LEVELS: ReadonlyArray<{ label: string; tone: Tone }> = [
  { label: "Normal", tone: "ok" },
  { label: "Warning", tone: "warn" },
  { label: "Urgent", tone: "bad" },
  { label: "Critical", tone: "bad" },
];

export function pressureLevel(
  value: number | undefined,
): { label: string; tone: Tone } | undefined {
  if (value === undefined) return undefined;
  const i = Math.min(PRESSURE_LEVELS.length - 1, Math.max(0, Math.round(value)));
  return PRESSURE_LEVELS[i];
}

export const MEMORY_EVENT_KINDS: readonly EventKind[] = ["memory_warning", "jetsam", "lmk_kill"];
export const MEMORY_EVENT_LIMIT = 5;

interface Part {
  id: MetricId;
  label: string;
  value: number | undefined;
  series: Series;
}

const formatBytes = (v: number) => formatValue(BYTES, v);

/** Memory: used/free (+ breakdown) history, current breakdown bar, pressure, memory events. */
export function MemoryPanel({ deviceId }: { deviceId: string }) {
  const total = useLatest(deviceId, "mem.total");
  const used = useMetric(deviceId, "mem.used", "memory_breakdown");
  const breakdownCap = useCapability(deviceId, "memory_breakdown");
  const free = useLatest(deviceId, "mem.free");
  const app = useLatest(deviceId, "mem.app");
  const wired = useLatest(deviceId, "mem.wired");
  const compressed = useLatest(deviceId, "mem.compressed");
  const cached = useLatest(deviceId, "mem.cached");
  const swap = useLatest(deviceId, "mem.swap");
  const pressure = useMetric(deviceId, "mem.pressure", "memory_pressure");
  const warningsCap = useCapability(deviceId, "memory_warnings");
  const events = useDevice(deviceId)?.events;
  const markers = useMarkers(deviceId);

  // iOS-like data carries app/wired; Android-like data carries used/cached/swap.
  const iosLike = app !== undefined || wired !== undefined;
  const parts: Part[] = iosLike
    ? [
        { id: "mem.app", label: "App", value: app, series: "cyan" },
        { id: "mem.wired", label: "Wired", value: wired, series: "vio" },
        { id: "mem.compressed", label: "Compressed", value: compressed, series: "amber" },
      ]
    : [
        { id: "mem.used", label: "Used", value: used.value, series: "cyan" },
        { id: "mem.cached", label: "Cached", value: cached, series: "vio" },
        { id: "mem.swap", label: "Swap", value: swap, series: "amber" },
      ];
  const present = parts.filter((p): p is Part & { value: number } => p.value !== undefined);
  const hasBreakdown = iosLike || cached !== undefined || swap !== undefined;

  const hasFree = free !== undefined;
  const extraKey = present
    .map((p) => p.id)
    .filter((id) => id !== "mem.used")
    .join(",");
  const metrics = useMemo<ChartMetric[]>(() => {
    const list: ChartMetric[] = [{ id: "mem.used", label: "Used", fill: true, endpoint: true }];
    if (hasFree) list.push({ id: "mem.free", label: "Free" });
    for (const id of extraKey.split(",").filter(Boolean) as MetricId[]) {
      const label = id.replace(/^mem\./, "");
      list.push({ id, label: label.charAt(0).toUpperCase() + label.slice(1) });
    }
    return list;
  }, [hasFree, extraKey]);

  const level = pressureLevel(pressure.value);
  const pill =
    level !== undefined ? (
      <Pill tone={level.tone} data-testid="mem-pressure">
        {level.label}
      </Pill>
    ) : (
      <span data-testid="mem-pressure">
        <Dash reason={pressure.reason} />
      </span>
    );
  const memoryEvents = useMemo(
    () => recentEvents(events ?? [], MEMORY_EVENT_KINDS, MEMORY_EVENT_LIMIT),
    [events],
  );

  return (
    <Card
      title="Memory"
      className="perf-card"
      data-testid="perf-memory"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[
            used.value !== undefined
              ? `${formatValue(BYTES, used.value)} used${total !== undefined ? ` of ${formatValue(BYTES, total)}` : ""}`
              : undefined,
            pill,
          ]}
        />
      }
    >
      <MetricChart
        deviceId={deviceId}
        metrics={metrics}
        height={CHART_HEIGHT}
        yFormat={formatBytes}
        name="Memory used and free over time"
        unit="bytes"
        markers={markers}
        offReason={breakdownCap.available === false ? used.reason : undefined}
        testId="mem-chart"
      />
      <div className="section-label">Breakdown</div>
      {present.length > 0 ? (
        <>
          <StackBar name="Memory by kind" segments={present} total={total} />
          <Legend
            items={present.map((p) => ({
              label: p.label,
              value: formatValue(BYTES, p.value),
              series: p.series,
            }))}
          />
        </>
      ) : null}
      {!hasBreakdown ? <DetailDash label="Breakdown" reason={used.reason} /> : null}
      <div className="section-label">Memory events</div>
      <EventList
        name="Memory events"
        events={memoryEvents}
        emptyText="No memory warnings, jetsam or LMK kills in this session"
        offReason={warningsCap.available === false ? capabilityReason(warningsCap) : undefined}
        testId="mem-events"
      />
    </Card>
  );
}
