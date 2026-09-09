import { formatValue } from "@statasmon/protocol";
import { Card, Dash, KeyValue, type KeyValueRow } from "@statasmon/ui";
import { useLatest } from "../../hooks/useDevice";
import { AMPS, CELSIUS, COUNT, Gauge, PERCENT, Subtitle, useMetric, VOLTS } from "./shared";

/** "charging" / "discharging" / "full" from the charging flag (fallback: current sign). */
export function chargeState(
  charging: number | undefined,
  current: number | undefined,
  percent: number | undefined,
): "charging" | "discharging" | "full" | undefined {
  const isCharging =
    charging !== undefined ? charging > 0 : current !== undefined ? current > 0 : undefined;
  if (isCharging === undefined) {
    return current !== undefined && current < 0 ? "discharging" : undefined;
  }
  if (isCharging && percent !== undefined && percent >= 100) return "full";
  return isCharging ? "charging" : "discharging";
}

/** Signed current: "+1.15 A" while charging, "-0.45 A" while discharging. */
export function formatCurrent(amps: number): string {
  const text = formatValue(AMPS, amps);
  return amps > 0 ? `+${text}` : text;
}

/** Minutes → "38 min" or "1 h 12 min". */
export function formatMinutes(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

/**
 * Time to full (charging) or time left (discharging) from the instantaneous
 * current and the full-charge capacity. Undefined when it cannot be computed.
 */
export function batteryEstimate(
  percent: number | undefined,
  currentA: number | undefined,
  fullMah: number | undefined,
): { label: "Full in" | "Time left"; minutes: number } | undefined {
  if (percent === undefined || currentA === undefined || fullMah === undefined) return undefined;
  if (fullMah <= 0 || currentA === 0) return undefined;
  const mA = Math.abs(currentA) * 1000;
  if (currentA > 0) {
    if (percent >= 100) return undefined;
    return { label: "Full in", minutes: ((fullMah * (100 - percent)) / 100 / mA) * 60 };
  }
  return { label: "Time left", minutes: ((fullMah * percent) / 100 / mA) * 60 };
}

/** Battery: percent ring, state in the subtitle, voltage / current / health / temperature / estimate. */
export function BatteryCard({ deviceId }: { deviceId: string }) {
  const percent = useMetric(deviceId, "bat.percent", "battery_basic");
  const charging = useLatest(deviceId, "bat.charging");
  const voltage = useMetric(deviceId, "bat.voltage", "battery_detail");
  const current = useMetric(deviceId, "bat.current", "battery_detail");
  const health = useMetric(deviceId, "bat.health", "battery_detail");
  const cycles = useLatest(deviceId, "bat.cycles");
  const temp = useMetric(deviceId, "bat.temp", "battery_detail");
  const fullCapacity = useLatest(deviceId, "bat.full_capacity");

  const state = chargeState(charging, current.value, percent.value);
  const estimate = batteryEstimate(percent.value, current.value, fullCapacity);

  const healthText =
    health.value !== undefined || cycles !== undefined
      ? [
          health.value !== undefined ? formatValue(PERCENT, health.value) : undefined,
          cycles !== undefined ? `${formatValue(COUNT, cycles)} cyc` : undefined,
        ]
          .filter(Boolean)
          .join(" · ")
      : undefined;

  const rows: KeyValueRow[] = [
    {
      label: "Voltage",
      value:
        voltage.value !== undefined ? (
          formatValue(VOLTS, voltage.value)
        ) : (
          <Dash reason={voltage.reason} />
        ),
    },
    {
      label: "Current",
      value:
        current.value !== undefined ? (
          formatCurrent(current.value)
        ) : (
          <Dash reason={current.reason} />
        ),
    },
    {
      label: "Health",
      value: healthText !== undefined ? healthText : <Dash reason={health.reason} />,
    },
    {
      label: "Temperature",
      value:
        temp.value !== undefined ? formatValue(CELSIUS, temp.value) : <Dash reason={temp.reason} />,
    },
  ];
  if (estimate) rows.push({ label: estimate.label, value: formatMinutes(estimate.minutes) });

  return (
    <Card title="Battery" subtitle={<Subtitle deviceId={deviceId} parts={[state]} />}>
      <div className="ov-ring-row">
        <Gauge value={percent.value} reason={percent.reason} name="Battery" series="amber" />
        <KeyValue rows={rows} style={{ flex: 1 }} />
      </div>
    </Card>
  );
}
