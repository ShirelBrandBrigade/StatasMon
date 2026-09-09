/**
 * Test-only helpers: seed the store with the synthetic session, then layer
 * extra `samples` / `capabilities` / `processes` / `info` messages on top.
 */
import type {
  Capability,
  DeviceHardware,
  DeviceInfo,
  MetricId,
  MetricValue,
  Platform,
  ProcessRow,
} from "@statasmon/protocol";
import { dispatch } from "../../bridge/dispatch";
import { synthesize } from "../../bridge/synthetic";
import { useAppStore } from "../../store/app";
import { telemetry } from "../../store/telemetry";

export const IPHONE = "fake:iphone";
export const IPAD = "fake:ipad";
export const ANDROID = "fake:android";

/** Resets both stores and replays `durationMs` of the synthetic session. */
export function seed(durationMs = 2000): void {
  useAppStore.getState().reset();
  telemetry.reset();
  for (const { message } of synthesize(durationMs)) dispatch(message);
}

/** Detaches (if present) and re-attaches a device with no samples, info or capabilities. */
export function fresh(deviceId: string, platform: Platform): void {
  dispatch({ type: "device_detached", device_id: deviceId });
  dispatch({
    type: "device_attached",
    device: {
      id: deviceId,
      platform,
      name: deviceId,
      model_name: deviceId,
      transport: "fake",
      state: { state: "tier2" },
    },
  });
}

/** One `samples` message; `tsMs` defaults to now. */
export function sample(
  deviceId: string,
  values: Partial<Record<MetricId, number>>,
  tsMs = Date.now(),
): void {
  const list: MetricValue[] = [];
  for (const [id, value] of Object.entries(values)) {
    if (value !== undefined) list.push({ id: id as MetricId, value });
  }
  dispatch({ type: "samples", device_id: deviceId, ts_ms: tsMs, values: list });
}

/** Capability set: `true` = available, a string = unavailable with that reason. */
export function caps(deviceId: string, entries: Partial<Record<Capability, true | string>>): void {
  dispatch({
    type: "capabilities",
    device_id: deviceId,
    set: {
      entries: Object.entries(entries).map(([capability, v]) => ({
        capability: capability as Capability,
        availability:
          v === true ? { status: "available" } : { status: "unavailable", reason: String(v) },
      })),
    },
  });
}

export function proc(over: Partial<ProcessRow> & { name: string }): ProcessRow {
  return {
    pid: 1,
    app_id: null,
    cpu_pct: null,
    mem_bytes: null,
    threads: null,
    net_bps: null,
    state: "unknown",
    energy: null,
    ...over,
  };
}

export function processes(deviceId: string, rows: ProcessRow[], tsMs = Date.now()): void {
  dispatch({ type: "processes", device_id: deviceId, ts_ms: tsMs, rows });
}

const HARDWARE: DeviceHardware = {
  chip_name: null,
  chip_identifier: null,
  chip_manufacturer: null,
  architecture: null,
  abis: [],
  cpu_cores: null,
  performance_cores: null,
  efficiency_cores: null,
  cpu_max_mhz: null,
  gpu_name: null,
  ram_bytes: null,
  storage_bytes: null,
  battery_design_mah: null,
  board: null,
  screen: null,
};

/** An `info` message with every field null except the given hardware overrides. */
export function info(deviceId: string, hardware: Partial<DeviceHardware>): void {
  const doc: DeviceInfo = {
    identity: {
      name: null,
      marketing_name: null,
      model_identifier: null,
      model_number: null,
      manufacturer: null,
      serial_number: null,
      unique_id: null,
      region: null,
    },
    software: {
      os_name: null,
      os_version: null,
      os_build: null,
      security_patch: null,
      kernel_version: null,
      api_level: null,
      firmware_version: null,
    },
    hardware: { ...HARDWARE, ...hardware },
    state: {
      uptime_secs: null,
      boot_time_ms: null,
      time_zone: null,
      locale: null,
      developer_mode: null,
      usb_debugging: null,
      wireless_debugging: null,
      selinux_mode: null,
    },
    connection: {
      transport: "fake",
      tier: null,
      pairing: null,
      tunnel: null,
      last_heartbeat_ms: null,
      tools: [],
    },
    raw: {},
  };
  dispatch({ type: "info", device_id: deviceId, info: doc });
}

/** Latest stored value of a metric (what the cards read through `useLatest`). */
export function latest(deviceId: string, id: MetricId): number | undefined {
  return useAppStore.getState().devices[deviceId]?.latest[id];
}
