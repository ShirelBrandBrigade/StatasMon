import type { SessionSummary } from "@statasmon/protocol";
import { expect, test } from "vitest";
import {
  alignToElapsed,
  defaultMetric,
  deviceSheetRows,
  sessionMarkers,
  sortSessions,
  timeline,
} from "./sessionsModel";

const T0 = Date.UTC(2026, 8, 8, 9, 0, 0);

test("alignToElapsed puts both sessions on elapsed time from their own start", () => {
  const a = {
    startedAtMs: T0,
    points: [
      { ts_ms: T0, value: 10 },
      { ts_ms: T0 + 1000, value: 11 },
      { ts_ms: T0 + 2000, value: 12 },
    ],
  };
  const b = {
    startedAtMs: T0 + 3_600_000,
    points: [
      { ts_ms: T0 + 3_600_000 - 500, value: 99 }, // before its start: dropped
      { ts_ms: T0 + 3_600_000, value: 20 },
      { ts_ms: T0 + 3_600_000 + 2000, value: 22 },
      { ts_ms: T0 + 3_600_000 + 3000, value: 23 },
    ],
  };
  const out = alignToElapsed(a, b);
  expect(out.t).toEqual([0, 1000, 2000, 3000]);
  expect(out.v[0]).toEqual([10, 11, 12, Number.NaN]);
  expect(out.v[1]).toEqual([20, Number.NaN, 22, 23]);
});

test("alignToElapsed with one session is that session on elapsed time", () => {
  const out = alignToElapsed({
    startedAtMs: 5000,
    points: [
      { ts_ms: 5000, value: 1 },
      { ts_ms: 7000, value: 2 },
    ],
  });
  expect(out.t).toEqual([0, 2000]);
  expect(out.v).toEqual([[1, 2]]);
});

const summary = (id: string, started: number, live = false): SessionSummary => ({
  id,
  device_id: "d",
  device_name: "iPhone",
  model_name: "iPhone 16 Pro",
  platform: "ios",
  started_at_ms: started,
  ended_at_ms: live ? null : started + 1000,
  elapsed_ms: 1000,
  sample_count: 1,
  log_count: 0,
  event_count: 0,
  mark_count: 0,
  size_bytes: 10,
  live,
});

test("sortSessions is newest first, live first among equals", () => {
  const out = sortSessions([
    summary("old", T0 - 1),
    summary("new", T0 + 1),
    summary("same-stored", T0),
    summary("same-live", T0, true),
  ]);
  expect(out.map((s) => s.id)).toEqual(["new", "same-live", "same-stored", "old"]);
});

test("timeline merges marks and events in time order; markers skip connection events", () => {
  const marks = [{ ts_ms: T0 + 500, label: "Mark 1" }];
  const events = [
    { ts_ms: T0, kind: "connection" as const, detail: "Session started", pid: null, process: null },
    {
      ts_ms: T0 + 900,
      kind: "thermal_change" as const,
      detail: "Nominal → Fair",
      pid: 1,
      process: "thermalmonitord",
    },
    { ts_ms: T0 + 950, kind: "crash" as const, detail: "SIGSEGV", pid: 2, process: "FieldKit" },
  ];
  expect(timeline(marks, events).map((i) => i.label)).toEqual([
    "Connection",
    "Mark",
    "Thermal",
    "Crash",
  ]);
  const markers = sessionMarkers(marks, events, T0);
  expect(markers).toEqual([
    { tsMs: 500, label: "Mark 1", tone: "info" },
    { tsMs: 900, label: "Thermal", tone: "warn" },
    { tsMs: 950, label: "Crash", tone: "bad" },
  ]);
});

test("defaultMetric prefers CPU and deviceSheetRows drops empty fields", () => {
  expect(defaultMetric(["mem.used", "cpu.total"])).toBe("cpu.total");
  expect(defaultMetric(["mem.used"])).toBe("mem.used");
  expect(defaultMetric([])).toBeUndefined();
  expect(deviceSheetRows(null)).toEqual([]);
  const rows = deviceSheetRows({
    identity: {
      name: "Elia's iPad",
      marketing_name: null,
      model_identifier: "iPad13,18",
      model_number: null,
      manufacturer: null,
      serial_number: null,
      unique_id: null,
      region: null,
    },
    software: {
      os_name: "iPadOS",
      os_version: "17.4.1",
      os_build: "21E237",
      security_patch: null,
      kernel_version: null,
      api_level: null,
      firmware_version: null,
    },
    hardware: {
      chip_name: null,
      chip_identifier: null,
      chip_manufacturer: null,
      architecture: null,
      abis: [],
      cpu_cores: 6,
      performance_cores: null,
      efficiency_cores: null,
      cpu_max_mhz: null,
      gpu_name: null,
      ram_bytes: 4 * 2 ** 30,
      storage_bytes: null,
      battery_design_mah: null,
      board: null,
      screen: null,
    },
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
      transport: null,
      tier: null,
      pairing: null,
      tunnel: null,
      last_heartbeat_ms: null,
      tools: [],
    },
    raw: {},
  });
  expect(rows).toEqual([
    { label: "Name", value: "Elia's iPad" },
    { label: "Model", value: "iPad13,18" },
    { label: "Identifier", value: "iPad13,18" },
    { label: "OS", value: "iPadOS 17.4.1 (21E237)" },
    { label: "CPU cores", value: "6" },
    { label: "RAM", value: "4.0 GB" },
  ]);
});
