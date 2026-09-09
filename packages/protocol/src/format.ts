import type { MetricMeta } from "../generated/MetricMeta";
import type { Unit } from "../generated/Unit";

export const METRIC_GROUPS = ["cpu", "mem", "thermal", "battery", "disk", "net", "proc"] as const;

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

function scale(value: number, base: number, units: string[], digits = 1): string {
  let v = value;
  let i = 0;
  while (Math.abs(v) >= base && i < units.length - 1) {
    v /= base;
    i += 1;
  }
  return `${v.toFixed(i === 0 ? 0 : digits)} ${units[i]}`;
}

export function unitSuffix(unit: Unit): string {
  switch (unit) {
    case "percent":
      return "%";
    case "celsius":
      return " °C";
    case "volts":
      return " V";
    case "amps":
      return " A";
    case "watts":
      return " W";
    case "milliamp_hours":
      return " mAh";
    case "seconds":
      return " s";
    case "milliseconds":
      return " ms";
    case "dbm":
      return " dBm";
    case "mbps":
      return " Mb/s";
    case "packets_per_sec":
      return " pkt/s";
    default:
      return "";
  }
}

/** Formats a metric value with its unit; `null`/`undefined` renders as a dash. */
export function formatValue(
  meta: Pick<MetricMeta, "unit">,
  value: number | null | undefined,
): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  switch (meta.unit) {
    case "bytes":
      return scale(value, 1024, BYTE_UNITS);
    case "bytes_per_sec":
      return `${scale(value, 1024, BYTE_UNITS)}/s`;
    case "bits_per_sec":
      return scale(value, 1000, ["b/s", "kb/s", "Mb/s", "Gb/s"]);
    case "bool":
      return value ? "Yes" : "No";
    case "count":
      return Math.round(value).toLocaleString("en-US").replace(/,/g, " ");
    case "load":
      return value.toFixed(2);
    case "state":
      return String(Math.round(value));
    case "volts":
    case "amps":
    case "watts":
      return `${value.toFixed(2)}${unitSuffix(meta.unit)}`;
    case "celsius":
      return `${value.toFixed(1)}${unitSuffix(meta.unit)}`;
    default:
      return `${Number.isInteger(value) ? value : value.toFixed(1)}${unitSuffix(meta.unit)}`;
  }
}
