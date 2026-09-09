import type { Capability, DeviceInfo, DeviceSummary, ToolVersion } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { formatCount, transportLabel } from "../../shell/format";

/** A row of one Device-sheet group; `null` renders as a Dash with `NOT_REPORTED`. */
export interface InfoRow {
  label: string;
  value: string | null;
}

/** Tooltip of every dash on the Device sheet (truth over polish). */
export const NOT_REPORTED = "Not reported by the device";

const text = (v: string | null | undefined): string | null =>
  v === null || v === undefined || v.trim() === "" ? null : v;

const count = (v: number | null | undefined): string | null =>
  v === null || v === undefined ? null : formatCount(v);

const bytes = (v: number | null | undefined): string | null =>
  v === null || v === undefined ? null : formatValue({ unit: "bytes" }, v);

const yesNo = (v: boolean | null | undefined): string | null =>
  v === null || v === undefined ? null : v ? "Yes" : "No";

/** `3 d 1 h 8 min`, `4 min 12 s` or `12 s`. */
export function formatUptime(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const d = Math.floor(total / 86_400);
  const h = Math.floor((total % 86_400) / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (d > 0) return `${d} d ${h} h ${m} min`;
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min ${s} s`;
  return `${s} s`;
}

/** Age of a timestamp: `just now`, `12 s ago`, `4 min ago`, `2 h 5 min ago`, `3 d ago`. */
export function formatAge(nowMs: number, tsMs: number): string {
  const secs = Math.max(0, Math.floor((nowMs - tsMs) / 1000));
  if (secs < 2) return "just now";
  if (secs < 60) return `${secs} s ago`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h ${m % 60} min ago`;
  return `${Math.floor(h / 24)} d ago`;
}

/** Local date and time, `2026-09-09 14:32:07`. */
export function formatDateTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function identityRows(info: DeviceInfo | undefined): InfoRow[] {
  const i = info?.identity;
  return [
    { label: "Device name", value: text(i?.name) },
    { label: "Marketing name", value: text(i?.marketing_name) },
    { label: "Model identifier", value: text(i?.model_identifier) },
    { label: "Model number", value: text(i?.model_number) },
    { label: "Manufacturer", value: text(i?.manufacturer) },
    { label: "Serial number", value: text(i?.serial_number) },
    { label: "Unique id", value: text(i?.unique_id) },
    { label: "Region", value: text(i?.region) },
  ];
}

export function softwareRows(info: DeviceInfo | undefined): InfoRow[] {
  const s = info?.software;
  return [
    { label: "OS", value: text(s?.os_name) },
    { label: "OS version", value: text(s?.os_version) },
    { label: "Build", value: text(s?.os_build) },
    { label: "Security patch", value: text(s?.security_patch) },
    { label: "API level", value: count(s?.api_level) },
    { label: "Kernel", value: text(s?.kernel_version) },
    { label: "Firmware / baseband", value: text(s?.firmware_version) },
  ];
}

function coreLine(cores: number | null, perf: number | null, eff: number | null): string | null {
  if (cores === null) return null;
  if (perf === null && eff === null) return formatCount(cores);
  const parts: string[] = [];
  if (perf !== null) parts.push(`${formatCount(perf)} P`);
  if (eff !== null) parts.push(`${formatCount(eff)} E`);
  return `${formatCount(cores)} (${parts.join(" + ")})`;
}

export function hardwareRows(info: DeviceInfo | undefined): InfoRow[] {
  const h = info?.hardware;
  return [
    { label: "Chip", value: text(h?.chip_name) },
    { label: "Chip identifier", value: text(h?.chip_identifier) },
    { label: "Chip manufacturer", value: text(h?.chip_manufacturer) },
    { label: "Architecture", value: text(h?.architecture) },
    { label: "ABIs", value: h && h.abis.length > 0 ? h.abis.join(", ") : null },
    {
      label: "CPU cores",
      value: h ? coreLine(h.cpu_cores, h.performance_cores, h.efficiency_cores) : null,
    },
    {
      label: "Max frequency",
      value:
        h?.cpu_max_mhz === null || h?.cpu_max_mhz === undefined
          ? null
          : `${formatCount(h.cpu_max_mhz)} MHz`,
    },
    { label: "GPU", value: text(h?.gpu_name) },
    { label: "RAM", value: bytes(h?.ram_bytes) },
    { label: "Storage", value: bytes(h?.storage_bytes) },
    {
      label: "Battery design capacity",
      value:
        h?.battery_design_mah === null || h?.battery_design_mah === undefined
          ? null
          : `${formatCount(h.battery_design_mah)} mAh`,
    },
    { label: "Board", value: text(h?.board) },
    { label: "Screen", value: text(h?.screen) },
  ];
}

export function stateRows(info: DeviceInfo | undefined): InfoRow[] {
  const s = info?.state;
  return [
    {
      label: "Uptime",
      value:
        s?.uptime_secs === null || s?.uptime_secs === undefined
          ? null
          : formatUptime(s.uptime_secs),
    },
    {
      label: "Last boot",
      value:
        s?.boot_time_ms === null || s?.boot_time_ms === undefined
          ? null
          : formatDateTime(s.boot_time_ms),
    },
    { label: "Time zone", value: text(s?.time_zone) },
    { label: "Locale", value: text(s?.locale) },
    { label: "Developer Mode", value: yesNo(s?.developer_mode) },
    { label: "USB debugging", value: yesNo(s?.usb_debugging) },
    { label: "Wireless debugging", value: yesNo(s?.wireless_debugging) },
    { label: "SELinux", value: text(s?.selinux_mode) },
  ];
}

function toolsLine(tools: ToolVersion[] | undefined): string | null {
  if (!tools || tools.length === 0) return null;
  return tools.map((t) => `${t.name} ${t.version}`).join(", ");
}

/** Connection health; the transport falls back to the summary while info is pending. */
export function connectionRows(
  info: DeviceInfo | undefined,
  summary: DeviceSummary,
  nowMs: number,
): InfoRow[] {
  const c = info?.connection;
  const transport = c?.transport ?? summary.transport;
  return [
    { label: "Transport", value: transportLabel(transport) },
    { label: "Tier", value: text(c?.tier) },
    { label: "Pairing", value: text(c?.pairing) },
    { label: "Tunnel", value: text(c?.tunnel) },
    {
      label: "Last heartbeat",
      value:
        c?.last_heartbeat_ms === null || c?.last_heartbeat_ms === undefined
          ? null
          : formatAge(nowMs, c.last_heartbeat_ms),
    },
    { label: "Tools", value: toolsLine(c?.tools) },
  ];
}

/** Every capability with the label the Device sheet shows. */
export const CAPABILITY_LABEL: Record<Capability, string> = {
  device_info: "Device information",
  cpu_total: "CPU total",
  cpu_per_core: "CPU per core",
  cpu_frequency: "CPU frequency",
  load_average: "Load average",
  memory_breakdown: "Memory breakdown",
  memory_pressure: "Memory pressure",
  memory_warnings: "Memory warnings",
  thermal_state: "Thermal state",
  temperature_sensors: "Temperature sensors",
  battery_basic: "Battery",
  battery_detail: "Battery detail",
  storage_totals: "Storage totals",
  storage_categories: "Storage categories",
  storage_per_app: "Storage per app",
  network_throughput: "Network throughput",
  network_packets: "Network packets",
  network_per_process: "Network per process",
  wifi_details: "Wi-Fi details",
  packet_capture: "Packet capture",
  processes: "Processes",
  process_threads: "Process threads",
  foreground_app: "Foreground app",
  logs: "Logs",
  per_app_logs: "Per-app logs",
  crash_reports: "Crash reports",
  energy: "Energy",
};

/** Label for a capability; an unknown wire name is humanised (`some_thing` → `Some thing`). */
export function capabilityLabel(cap: string): string {
  const known = (CAPABILITY_LABEL as Record<string, string | undefined>)[cap];
  if (known) return known;
  const words = cap.replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Raw keys sorted by name (case-insensitive) as `[key, value]` pairs. */
export function sortedRaw(raw: Record<string, string> | undefined): Array<[string, string]> {
  if (!raw) return [];
  return Object.entries(raw).sort(([a], [b]) => a.localeCompare(b, "en", { sensitivity: "base" }));
}

/** Pretty JSON of the raw keys in sorted order (what "Copy as JSON" puts on the clipboard). */
export function rawJson(raw: Record<string, string> | undefined): string {
  return JSON.stringify(Object.fromEntries(sortedRaw(raw)), null, 2);
}
