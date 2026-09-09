import type { MetricId, Platform } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { Card, Dash, KeyValue, type KeyValueRow, Pill, Sparkline, type Tone } from "@statasmon/ui";
import { useCapability, useDevice, useLatest, useMetricSeries } from "../../hooks/useDevice";
import { CELSIUS, capabilityReason, NO_SAMPLE, Subtitle, useMetric } from "./shared";

interface ThermalLevel {
  label: string;
  tone: Tone;
}

/** iOS `thermal.state` 0–3 (thermalmonitord levels). */
export const IOS_LEVELS: readonly ThermalLevel[] = [
  { label: "Nominal", tone: "ok" },
  { label: "Fair", tone: "warn" },
  { label: "Serious", tone: "bad" },
  { label: "Critical", tone: "bad" },
];

/** Android `thermal.raw` 0–6 (`dumpsys thermalservice` status). */
export const ANDROID_LEVELS: readonly ThermalLevel[] = [
  { label: "None", tone: "ok" },
  { label: "Light", tone: "warn" },
  { label: "Moderate", tone: "warn" },
  { label: "Severe", tone: "bad" },
  { label: "Critical", tone: "bad" },
  { label: "Emergency", tone: "bad" },
  { label: "Shutdown", tone: "bad" },
];

/** Maps the raw level of a platform to its label and tone; undefined for values outside the table. */
export function thermalLevel(
  platform: Platform,
  value: number | undefined,
): ThermalLevel | undefined {
  if (value === undefined) return undefined;
  const table = platform === "android" ? ANDROID_LEVELS : IOS_LEVELS;
  return table[Math.round(value)];
}

const SENSORS: ReadonlyArray<{ label: string; id: MetricId }> = [
  { label: "SoC", id: "temp.soc" },
  { label: "Battery", id: "temp.battery" },
  { label: "Power management", id: "temp.pmu" },
  { label: "Display", id: "temp.display" },
  { label: "Skin", id: "temp.skin" },
];

function finite(series: { v: number[] }): number[] {
  return series.v.filter((x) => Number.isFinite(x));
}

/** Thermal: state pill, per-sensor temperatures, battery (or SoC) temperature sparkline. */
export function ThermalCard({ deviceId }: { deviceId: string }) {
  const platform = useDevice(deviceId)?.summary.platform ?? "fake";
  const metricId: MetricId = platform === "android" ? "thermal.raw" : "thermal.state";
  const state = useMetric(deviceId, metricId, "thermal_state");
  const sensors = useCapability(deviceId, "temperature_sensors");
  const soc = useLatest(deviceId, "temp.soc");
  const battery = useLatest(deviceId, "temp.battery");
  const batteryFallback = useLatest(deviceId, "bat.temp");
  const pmu = useLatest(deviceId, "temp.pmu");
  const display = useLatest(deviceId, "temp.display");
  const skin = useLatest(deviceId, "temp.skin");
  const batterySeries = useMetricSeries(deviceId, "temp.battery");
  const socSeries = useMetricSeries(deviceId, "temp.soc");

  const values: Partial<Record<MetricId, number | undefined>> = {
    "temp.soc": soc,
    "temp.battery": battery ?? batteryFallback,
    "temp.pmu": pmu,
    "temp.display": display,
    "temp.skin": skin,
  };
  const rows: KeyValueRow[] = [];
  for (const s of SENSORS) {
    const v = values[s.id];
    if (v !== undefined) rows.push({ label: s.label, value: formatValue(CELSIUS, v) });
  }
  if (sensors.available === false) {
    rows.push({ label: "Other sensors", value: <Dash reason={capabilityReason(sensors)} /> });
  } else if (rows.length === 0) {
    rows.push({ label: "Sensors", value: <Dash reason={NO_SAMPLE} /> });
  }

  const level = thermalLevel(platform, state.value);
  const pill =
    level !== undefined ? (
      <Pill tone={level.tone} data-testid="thermal-state">
        {level.label}
      </Pill>
    ) : (
      <Dash reason={state.reason} />
    );

  const trend = finite(batterySeries).length >= 2 ? finite(batterySeries) : finite(socSeries);

  return (
    <Card title="Thermal" subtitle={<Subtitle deviceId={deviceId} parts={[pill]} />}>
      <KeyValue rows={rows} />
      {trend.length >= 2 ? (
        <Sparkline
          values={trend}
          width={300}
          height={50}
          series="amber"
          name="Temperature trend"
          preserveAspectRatio="none"
          className="ov-spark"
          style={{ display: "block", width: "100%", marginTop: "auto", overflow: "visible" }}
        />
      ) : null}
    </Card>
  );
}
