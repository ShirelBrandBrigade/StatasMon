import { formatValue } from "@statasmon/protocol";
import { Card, Legend, MetricFigure, type Series, StackBar } from "@statasmon/ui";
import { useLatest } from "../../hooks/useDevice";
import { BYTES, DetailDash, Gauge, percentOf, Subtitle, useMetric } from "./shared";

/** `mem.pressure` 0–3 → label. */
export const PRESSURE_LABELS = ["normal", "warning", "urgent", "critical"] as const;

export function pressureLabel(level: number | undefined): string | undefined {
  if (level === undefined) return undefined;
  const i = Math.min(PRESSURE_LABELS.length - 1, Math.max(0, Math.round(level)));
  return PRESSURE_LABELS[i];
}

interface Part {
  label: string;
  value: number | undefined;
  series: Series;
}

/** Memory: used ring, "x GB used", stacked breakdown with a legend, pressure in the subtitle. */
export function MemoryCard({ deviceId }: { deviceId: string }) {
  const total = useLatest(deviceId, "mem.total");
  const used = useMetric(deviceId, "mem.used", "memory_breakdown");
  const app = useLatest(deviceId, "mem.app");
  const wired = useLatest(deviceId, "mem.wired");
  const compressed = useLatest(deviceId, "mem.compressed");
  const cached = useLatest(deviceId, "mem.cached");
  const swap = useLatest(deviceId, "mem.swap");
  const pressure = pressureLabel(useLatest(deviceId, "mem.pressure"));

  // iOS-like data carries app/wired; Android-like data carries used/cached/swap.
  const iosLike = app !== undefined || wired !== undefined;
  const parts: Part[] = iosLike
    ? [
        { label: "App", value: app, series: "cyan" },
        { label: "Wired", value: wired, series: "vio" },
        { label: "Compressed", value: compressed, series: "amber" },
      ]
    : [
        { label: "Used", value: used.value, series: "cyan" },
        { label: "Cached", value: cached, series: "vio" },
        { label: "Swap", value: swap, series: "amber" },
      ];
  const present = parts.filter((p): p is Part & { value: number } => p.value !== undefined);
  const hasBreakdown = iosLike || cached !== undefined || swap !== undefined;
  const segments = present.map((p) => ({ label: p.label, value: p.value, series: p.series }));

  return (
    <Card
      title="Memory"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[
            total !== undefined ? formatValue(BYTES, total) : undefined,
            pressure !== undefined ? `pressure ${pressure}` : undefined,
          ]}
        />
      }
    >
      <div className="ov-ring-row">
        <Gauge
          value={percentOf(used.value, total)}
          reason={used.reason}
          name="Memory"
          series="vio"
        />
        <div className="ov-detail">
          <MetricFigure
            meta={BYTES}
            value={used.value}
            suffix="used"
            unavailableReason={used.reason}
          />
          {segments.length > 0 ? (
            <StackBar name="Memory by kind" segments={segments} total={total} />
          ) : null}
          {present.length > 0 ? (
            <Legend
              items={present.map((p) => ({
                label: p.label,
                value: formatValue(BYTES, p.value),
                series: p.series,
              }))}
            />
          ) : null}
          {!hasBreakdown ? <DetailDash label="Breakdown" reason={used.reason} /> : null}
        </div>
      </div>
    </Card>
  );
}
