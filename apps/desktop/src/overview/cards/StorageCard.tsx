import { formatValue } from "@statasmon/protocol";
import { Card, Legend, MetricFigure, type Series, StackBar } from "@statasmon/ui";
import { useCapability, useLatest } from "../../hooks/useDevice";
import {
  BYTES,
  capabilityReason,
  DetailDash,
  Gauge,
  percentOf,
  Subtitle,
  useMetric,
} from "./shared";

interface Part {
  label: string;
  value: number | undefined;
  series: Series;
}

/** Storage: used ring, "x GB free", categories stack (apps / media / system / other) with a legend. */
export function StorageCard({ deviceId }: { deviceId: string }) {
  const total = useLatest(deviceId, "disk.total");
  const used = useMetric(deviceId, "disk.used", "storage_totals");
  const free = useMetric(deviceId, "disk.free", "storage_totals");
  const categories = useCapability(deviceId, "storage_categories");
  const parts: Part[] = [
    { label: "Apps", value: useLatest(deviceId, "disk.apps"), series: "mint" },
    { label: "Media", value: useLatest(deviceId, "disk.media"), series: "vio" },
    { label: "System", value: useLatest(deviceId, "disk.system"), series: "cyan" },
    { label: "Other", value: useLatest(deviceId, "disk.other"), series: "amber" },
  ];
  const present = parts.filter((p): p is Part & { value: number } => p.value !== undefined);
  const hasCategories = present.length > 0;
  const segments = hasCategories
    ? present.map((p) => ({ label: p.label, value: p.value, series: p.series }))
    : used.value !== undefined
      ? [{ label: "Used", value: used.value, series: "mint" as const }]
      : [];

  return (
    <Card
      title="Storage"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[total !== undefined ? formatValue(BYTES, total) : undefined]}
        />
      }
    >
      <div className="ov-ring-row">
        <Gauge
          value={percentOf(used.value, total)}
          reason={used.reason}
          name="Storage"
          series="mint"
        />
        <div className="ov-detail">
          <MetricFigure
            meta={BYTES}
            value={free.value}
            suffix="free"
            unavailableReason={free.reason}
          />
          {segments.length > 0 ? (
            <StackBar name="Storage by category" segments={segments} total={total} />
          ) : null}
          {hasCategories ? (
            <Legend
              items={present.map((p) => ({
                label: p.label,
                value: formatValue(BYTES, p.value),
                series: p.series,
              }))}
            />
          ) : (
            <DetailDash label="Categories" reason={capabilityReason(categories)} />
          )}
        </div>
      </div>
    </Card>
  );
}
