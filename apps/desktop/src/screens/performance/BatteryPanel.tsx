import { formatValue, type Unit } from "@statasmon/protocol";
import { Card, Dash, KeyValue, type KeyValueRow, Pill, type Tone } from "@statasmon/ui";
import type { ReactNode } from "react";
import { useCapability, useLatest } from "../../hooks/useDevice";
import {
  batteryEstimate,
  chargeState,
  formatCurrent,
  formatMinutes,
} from "../../overview/cards/BatteryCard";
import { CELSIUS, COUNT, PERCENT, Subtitle, useMetric, VOLTS } from "../../overview/cards/shared";
import { CHART_HEIGHT, type ChartMetric, MetricChart, SMALL_CHART_HEIGHT } from "./shared";

const MAH = { unit: "milliamp_hours" } as const satisfies { unit: Unit };
const WATTS = { unit: "watts" } as const satisfies { unit: Unit };

const PERCENT_METRICS: ChartMetric[] = [
  { id: "bat.percent", label: "Charge", fill: true, endpoint: true, series: "amber" },
];
const VOLTAGE_METRICS: ChartMetric[] = [{ id: "bat.voltage", label: "Voltage", endpoint: true }];
const CURRENT_METRICS: ChartMetric[] = [
  { id: "bat.current", label: "Current", endpoint: true, series: "vio" },
];
const TEMP_METRICS: ChartMetric[] = [
  { id: "bat.temp", label: "Temperature", endpoint: true, series: "mint" },
];

const formatPct = (v: number) => formatValue(PERCENT, v);
const formatVolts = (v: number) => formatValue(VOLTS, v);
const formatAmps = (v: number) => formatCurrent(v);
const formatCelsius = (v: number) => formatValue(CELSIUS, v);

const STATE_TONE: Record<"charging" | "discharging" | "full", Tone> = {
  charging: "ok",
  full: "ok",
  discharging: "neutral",
};

/** Small chart with a caption row: label on the left, the current value (or a dash) on the right. */
function SmallChart({
  deviceId,
  label,
  metrics,
  value,
  reason,
  yFormat,
  yMin,
  unit,
  offReason,
  testId,
}: {
  deviceId: string;
  label: string;
  metrics: ChartMetric[];
  value: ReactNode;
  reason: string;
  yFormat: (v: number) => string;
  yMin?: number | "auto";
  unit: string;
  offReason?: string;
  testId: string;
}) {
  return (
    <div className="perf-small" data-testid={testId}>
      <div className="perf-chart-label">
        <span>{label}</span>
        <span className="mono">{value ?? <Dash reason={reason} />}</span>
      </div>
      <MetricChart
        deviceId={deviceId}
        metrics={metrics}
        height={SMALL_CHART_HEIGHT}
        yFormat={yFormat}
        yMin={yMin}
        name={`${label} over time`}
        unit={unit}
        offReason={offReason}
      />
    </div>
  );
}

/** Battery: percent history, voltage / current / temperature (own charts), health and capacities. */
export function BatteryPanel({ deviceId }: { deviceId: string }) {
  const percent = useMetric(deviceId, "bat.percent", "battery_basic");
  const basicCap = useCapability(deviceId, "battery_basic");
  const detailCap = useCapability(deviceId, "battery_detail");
  const charging = useLatest(deviceId, "bat.charging");
  const voltage = useMetric(deviceId, "bat.voltage", "battery_detail");
  const current = useMetric(deviceId, "bat.current", "battery_detail");
  const temp = useMetric(deviceId, "bat.temp", "battery_detail");
  const power = useMetric(deviceId, "bat.power", "battery_detail");
  const health = useMetric(deviceId, "bat.health", "battery_detail");
  const cycles = useMetric(deviceId, "bat.cycles", "battery_detail");
  const design = useMetric(deviceId, "bat.design_capacity", "battery_detail");
  const full = useMetric(deviceId, "bat.full_capacity", "battery_detail");

  const state = chargeState(charging, current.value, percent.value);
  const estimate = batteryEstimate(percent.value, current.value, full.value);
  const detailOff = detailCap.available === false ? voltage.reason : undefined;

  const statePill =
    state !== undefined ? (
      <Pill tone={STATE_TONE[state]} data-testid="bat-state">
        {state}
      </Pill>
    ) : (
      <span data-testid="bat-state">
        <Dash reason={percent.reason} />
      </span>
    );

  const kv = (value: number | undefined, text: string, reason: string): ReactNode =>
    value !== undefined ? text : <Dash reason={reason} />;
  const rows: KeyValueRow[] = [
    { label: "State", value: statePill },
    {
      label: "Health",
      value: kv(health.value, formatValue(PERCENT, health.value), health.reason),
    },
    {
      label: "Cycles",
      value: kv(cycles.value, formatValue(COUNT, cycles.value), cycles.reason),
    },
    {
      label: "Design capacity",
      value: kv(design.value, formatValue(MAH, design.value), design.reason),
    },
    {
      label: "Full-charge capacity",
      value: kv(full.value, formatValue(MAH, full.value), full.reason),
    },
    { label: "Power", value: kv(power.value, formatValue(WATTS, power.value), power.reason) },
  ];
  if (estimate) rows.push({ label: estimate.label, value: formatMinutes(estimate.minutes) });

  return (
    <Card
      title="Battery"
      className="perf-card"
      data-testid="perf-battery"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[
            percent.value !== undefined ? formatValue(PERCENT, percent.value) : undefined,
            state,
          ]}
        />
      }
    >
      <MetricChart
        deviceId={deviceId}
        metrics={PERCENT_METRICS}
        height={CHART_HEIGHT}
        yFormat={formatPct}
        name="Battery charge over time"
        unit="percent"
        offReason={basicCap.available === false ? percent.reason : undefined}
        testId="bat-chart"
      />
      <div className="perf-small-grid">
        <SmallChart
          deviceId={deviceId}
          label="Voltage"
          metrics={VOLTAGE_METRICS}
          value={voltage.value !== undefined ? formatValue(VOLTS, voltage.value) : undefined}
          reason={voltage.reason}
          yFormat={formatVolts}
          yMin="auto"
          unit="volts"
          offReason={detailOff}
          testId="bat-voltage"
        />
        <SmallChart
          deviceId={deviceId}
          label="Current"
          metrics={CURRENT_METRICS}
          value={current.value !== undefined ? formatCurrent(current.value) : undefined}
          reason={current.reason}
          yFormat={formatAmps}
          yMin="auto"
          unit="amperes"
          offReason={detailOff}
          testId="bat-current"
        />
        <SmallChart
          deviceId={deviceId}
          label="Temperature"
          metrics={TEMP_METRICS}
          value={temp.value !== undefined ? formatValue(CELSIUS, temp.value) : undefined}
          reason={temp.reason}
          yFormat={formatCelsius}
          yMin="auto"
          unit="degrees Celsius"
          offReason={detailOff}
          testId="bat-temp"
        />
      </div>
      <KeyValue rows={rows} data-testid="bat-kv" />
    </Card>
  );
}
