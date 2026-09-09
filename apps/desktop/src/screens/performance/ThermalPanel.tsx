import type { MetricId, Platform } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { Card, Dash, Pill } from "@statasmon/ui";
import { useMemo } from "react";
import { useCapability, useDevice, useLatest } from "../../hooks/useDevice";
import { CELSIUS, capabilityReason, Subtitle, useMetric } from "../../overview/cards/shared";
import { ANDROID_LEVELS, IOS_LEVELS, thermalLevel } from "../../overview/cards/ThermalCard";
import {
  CHART_HEIGHT,
  type ChartMetric,
  EventList,
  MetricChart,
  recentEvents,
  useMarkers,
} from "./shared";

export const THERMAL_EVENT_LIMIT = 6;
const STATE_CHART_HEIGHT = 90;

/** Every temperature sensor the protocol knows, in display order. */
export const TEMPERATURE_SENSORS: ReadonlyArray<{ id: MetricId; label: string }> = [
  { id: "temp.soc", label: "SoC" },
  { id: "temp.battery", label: "Battery" },
  { id: "temp.pmu", label: "PMU" },
  { id: "temp.display", label: "Display" },
  { id: "temp.skin", label: "Skin" },
];

/** Axis label for a thermal level: only whole levels get a name, the rest stay blank. */
export function thermalAxisLabel(platform: Platform, value: number): string {
  if (!Number.isInteger(value)) return "";
  const table = platform === "android" ? ANDROID_LEVELS : IOS_LEVELS;
  return table[value]?.label ?? "";
}

const formatCelsius = (v: number) => formatValue(CELSIUS, v);

/** Thermal: state history (step-like), every readable temperature, thermal-change events. */
export function ThermalPanel({ deviceId }: { deviceId: string }) {
  const platform = useDevice(deviceId)?.summary.platform ?? "fake";
  const stateId: MetricId = platform === "android" ? "thermal.raw" : "thermal.state";
  const state = useMetric(deviceId, stateId, "thermal_state");
  const stateCap = useCapability(deviceId, "thermal_state");
  const sensorsCap = useCapability(deviceId, "temperature_sensors");
  const soc = useLatest(deviceId, "temp.soc");
  const battery = useLatest(deviceId, "temp.battery");
  const pmu = useLatest(deviceId, "temp.pmu");
  const display = useLatest(deviceId, "temp.display");
  const skin = useLatest(deviceId, "temp.skin");
  const batteryFallback = useLatest(deviceId, "bat.temp");
  const events = useDevice(deviceId)?.events;
  const markers = useMarkers(deviceId);

  const latest: Partial<Record<MetricId, number | undefined>> = {
    "temp.soc": soc,
    "temp.battery": battery,
    "temp.pmu": pmu,
    "temp.display": display,
    "temp.skin": skin,
  };
  const presentKey = TEMPERATURE_SENSORS.filter((s) => latest[s.id] !== undefined)
    .map((s) => s.id)
    .join(",");
  const useFallback = presentKey === "" && batteryFallback !== undefined;
  const stateMetrics = useMemo<ChartMetric[]>(
    () => [{ id: stateId, label: "State", fill: true, endpoint: true }],
    [stateId],
  );
  const tempMetrics = useMemo<ChartMetric[]>(() => {
    if (useFallback) return [{ id: "bat.temp", label: "Battery", endpoint: true }];
    const ids = new Set(presentKey.split(",").filter(Boolean));
    return TEMPERATURE_SENSORS.filter((s) => ids.has(s.id)).map((s, i) => ({
      id: s.id,
      label: s.label,
      endpoint: i === 0,
    }));
  }, [presentKey, useFallback]);

  const level = thermalLevel(platform, state.value);
  const pill =
    level !== undefined ? (
      <Pill tone={level.tone} data-testid="thermal-state">
        {level.label}
      </Pill>
    ) : (
      <span data-testid="thermal-state">
        <Dash reason={state.reason} />
      </span>
    );
  const thermalEvents = useMemo(
    () => recentEvents(events ?? [], ["thermal_change"], THERMAL_EVENT_LIMIT),
    [events],
  );
  const stateAxis = (v: number) => thermalAxisLabel(platform, v);
  const tempReason = tempMetrics.length === 0 ? capabilityReason(sensorsCap) : undefined;

  return (
    <Card
      title="Thermal"
      className="perf-card"
      data-testid="perf-thermal"
      subtitle={<Subtitle deviceId={deviceId} parts={[pill]} />}
    >
      <div className="section-label">State</div>
      <MetricChart
        deviceId={deviceId}
        metrics={stateMetrics}
        height={STATE_CHART_HEIGHT}
        yFormat={stateAxis}
        name="Thermal state over time"
        markers={markers}
        offReason={stateCap.available === false ? state.reason : undefined}
        testId="thermal-state-chart"
      />
      <div className="section-label">Temperatures</div>
      <MetricChart
        deviceId={deviceId}
        metrics={tempMetrics}
        height={CHART_HEIGHT}
        yFormat={formatCelsius}
        yMin="auto"
        name="Temperatures over time"
        unit="degrees Celsius"
        offReason={tempReason}
        testId="thermal-temp-chart"
      />
      <div className="section-label">Thermal events</div>
      <EventList
        name="Thermal events"
        events={thermalEvents}
        emptyText="No thermal state changes in this session"
        offReason={stateCap.available === false ? capabilityReason(stateCap) : undefined}
        testId="thermal-events"
      />
    </Card>
  );
}
