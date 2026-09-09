import type { CoreSample, EventKind } from "@statasmon/protocol";
import { act, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { dispatch } from "../../bridge/dispatch";
import { caps, fresh, IPAD, IPHONE, sample, seed } from "../../overview/cards/testkit";
import { PerformanceScreen } from "./index";

vi.mock("../../bridge/commands", () => ({
  commands: { queryHistory: vi.fn(async () => []), deviceAction: vi.fn(async () => null) },
}));

const FREQ_REASON = "iOS does not report core frequencies to the host";
const SENSORS_REASON = "iOS exposes only the battery temperature to the host";
const DEV_MODE = "Developer Mode is off on this iPad";

function core(index: number, over: Partial<CoreSample> = {}): CoreSample {
  return {
    index,
    kind: index < 2 ? "performance" : "efficiency",
    usage_pct: 40 + index * 5,
    freq_mhz: null,
    online: true,
    ...over,
  };
}

function cores(deviceId: string, list: CoreSample[]): void {
  dispatch({ type: "cores", device_id: deviceId, ts_ms: Date.now(), cores: list });
}

function event(
  deviceId: string,
  kind: EventKind,
  detail: string,
  process: string | null = null,
  tsMs = Date.now(),
): void {
  dispatch({ type: "event", device_id: deviceId, ts_ms: tsMs, kind, detail, pid: null, process });
}

function seedIphone(): void {
  caps(IPHONE, {
    cpu_total: true,
    cpu_per_core: true,
    cpu_frequency: FREQ_REASON,
    load_average: true,
    memory_breakdown: true,
    memory_pressure: true,
    memory_warnings: true,
    thermal_state: true,
    temperature_sensors: SENSORS_REASON,
    battery_basic: true,
    battery_detail: true,
  });
  sample(IPHONE, {
    "cpu.total": 52.7,
    "cpu.user": 35.9,
    "cpu.system": 16.9,
    "cpu.load1": 2.849,
    "cpu.load5": 2.621,
    "mem.total": 8 * 2 ** 30,
    "mem.used": 5.5 * 2 ** 30,
    "mem.free": 1.97 * 2 ** 30,
    "mem.app": 2.99 * 2 ** 30,
    "mem.wired": 1.34 * 2 ** 30,
    "mem.compressed": 0.82 * 2 ** 30,
    "mem.pressure": 0,
    "thermal.state": 1,
    "temp.battery": 34.1,
    "bat.percent": 87,
    "bat.voltage": 4.215,
    "bat.current": 1.12,
    "bat.temp": 34.1,
    "bat.power": 4.7,
    "bat.cycles": 214,
    "bat.health": 96,
    "bat.charging": 1,
    "bat.design_capacity": 3582,
    "bat.full_capacity": 3438.72,
  });
  cores(
    IPHONE,
    [0, 1, 2, 3, 4, 5].map((i) => core(i)),
  );
}

beforeEach(() => {
  seed();
  vi.clearAllMocks();
});

test("CPU: total chart, one row per core with the frequency reason, load averages", () => {
  act(() => seedIphone());
  render(<PerformanceScreen deviceId={IPHONE} />);

  const cpu = within(screen.getByTestId("perf-cpu"));
  expect(cpu.getByText("52.7% · 6 cores · 2P + 4E")).toBeInTheDocument();
  expect(cpu.getByRole("img", { name: "CPU utilisation over time" })).toBeInTheDocument();
  const rows = cpu.getAllByTestId("core-row");
  expect(rows).toHaveLength(6);
  expect(rows[0]).toHaveTextContent("P0");
  expect(rows[0]).toHaveTextContent("40%");
  expect(rows[2]).toHaveTextContent("E0");
  expect(rows[5]).toHaveTextContent("E3");
  expect(cpu.getAllByRole("img", { name: `Unavailable: ${FREQ_REASON}` })).toHaveLength(6);
  expect(cpu.getByTestId("cpu-load")).toHaveTextContent("load 1m 2.85 · 5m 2.62");
});

test("CPU: frequencies in the mono face when reported, offline cores marked", () => {
  act(() => {
    seedIphone();
    caps(IPHONE, { cpu_frequency: true });
    cores(IPHONE, [
      core(0, { freq_mhz: 2400 }),
      core(1, { freq_mhz: 998 }),
      core(2, { online: false, usage_pct: 0 }),
    ]);
  });
  render(<PerformanceScreen deviceId={IPHONE} />);

  const rows = within(screen.getByTestId("perf-cpu")).getAllByTestId("core-row");
  expect(rows).toHaveLength(3);
  expect(rows[0]).toHaveTextContent("2.40 GHz");
  expect(rows[1]).toHaveTextContent("998 MHz");
  expect(rows[2]).toHaveAttribute("data-online", "false");
  expect(rows[2]).toHaveTextContent("offline");
  expect(
    within(rows[2]).getByRole("img", { name: "Unavailable: Core is offline" }),
  ).toBeInTheDocument();
});

test("Memory: breakdown legend, pressure pill and the newest memory events", () => {
  act(() => {
    seedIphone();
    event(IPHONE, "memory_warning", "Memory pressure warning", null, 1_757_419_260_000);
    event(
      IPHONE,
      "jetsam",
      "Jetsam killed FieldKit (footprint 1.2 GB)",
      "FieldKit",
      1_757_419_270_000,
    );
    event(IPHONE, "crash", "Not a memory event", "Other", 1_757_419_280_000);
  });
  render(<PerformanceScreen deviceId={IPHONE} />);

  const mem = within(screen.getByTestId("perf-memory"));
  expect(mem.getByTestId("mem-pressure")).toHaveTextContent("Normal");
  expect(screen.getByTestId("perf-memory")).toHaveTextContent("5.5 GB used of 8.0 GB");
  expect(mem.getByRole("img", { name: "Memory by kind" })).toBeInTheDocument();
  // Legend labels (the chart tooltip repeats the series names, hence getAll).
  for (const label of ["App", "Wired", "Compressed"]) {
    expect(mem.getAllByText(label).length).toBeGreaterThan(0);
  }
  const list = mem.getByRole("list", { name: "Memory events" });
  const items = within(list).getAllByRole("listitem");
  expect(items).toHaveLength(2);
  expect(items[0]).toHaveTextContent("Jetsam");
  expect(items[0]).toHaveTextContent("FieldKit");
  expect(items[1]).toHaveTextContent("Memory warning");
  expect(list).not.toHaveTextContent("Not a memory event");
});

test("Memory: pressure levels map 0–3 to Normal / Warning / Urgent / Critical", () => {
  act(() => {
    seedIphone();
    sample(IPHONE, { "mem.pressure": 2 });
  });
  render(<PerformanceScreen deviceId={IPHONE} />);
  expect(screen.getByTestId("mem-pressure")).toHaveTextContent("Urgent");
});

test("Thermal: state pill, temperature chart from the battery sensor, thermal events", () => {
  act(() => {
    seedIphone();
    event(IPHONE, "thermal_change", "Nominal → Fair", "thermalmonitord", 1_757_419_265_000);
  });
  render(<PerformanceScreen deviceId={IPHONE} />);

  const th = within(screen.getByTestId("perf-thermal"));
  expect(th.getByTestId("thermal-state")).toHaveTextContent("Fair");
  expect(th.getByRole("img", { name: "Thermal state over time" })).toBeInTheDocument();
  expect(th.getByRole("img", { name: "Temperatures over time" })).toBeInTheDocument();
  const list = th.getByRole("list", { name: "Thermal events" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  expect(list).toHaveTextContent("Nominal → Fair");
  expect(list).toHaveTextContent("thermalmonitord");
});

test("Battery: percent + separate voltage / current / temperature charts and the health rows", () => {
  act(() => seedIphone());
  render(<PerformanceScreen deviceId={IPHONE} />);

  const bat = within(screen.getByTestId("perf-battery"));
  expect(bat.getByText("87% · charging")).toBeInTheDocument();
  expect(bat.getByRole("img", { name: "Battery charge over time" })).toBeInTheDocument();
  expect(bat.getByRole("img", { name: "Voltage over time" })).toBeInTheDocument();
  expect(bat.getByRole("img", { name: "Current over time" })).toBeInTheDocument();
  expect(bat.getByRole("img", { name: "Temperature over time" })).toBeInTheDocument();
  expect(bat.getByTestId("bat-voltage")).toHaveTextContent("4.21 V");
  expect(bat.getByTestId("bat-current")).toHaveTextContent("+1.12 A");
  expect(bat.getByTestId("bat-temp")).toHaveTextContent("34.1 °C");
  const kv = bat.getByTestId("bat-kv");
  expect(kv).toHaveTextContent("Health96%");
  expect(kv).toHaveTextContent("Cycles214");
  expect(kv).toHaveTextContent("Design capacity3582 mAh");
  expect(kv).toHaveTextContent("Full-charge capacity3438.7 mAh");
  expect(kv).toHaveTextContent("Power4.70 W");
  expect(bat.getByTestId("bat-state")).toHaveTextContent("charging");
  expect(kv).toHaveTextContent("Full in");
});

test("every block dashes with the capability reason on a tier-2 iPad", () => {
  act(() => {
    fresh(IPAD, "ios");
    caps(IPAD, {
      cpu_total: DEV_MODE,
      cpu_per_core: DEV_MODE,
      cpu_frequency: FREQ_REASON,
      load_average: DEV_MODE,
      memory_breakdown: DEV_MODE,
      memory_pressure: DEV_MODE,
      memory_warnings: true,
      thermal_state: true,
      temperature_sensors: SENSORS_REASON,
      battery_basic: true,
      battery_detail: DEV_MODE,
    });
  });
  render(<PerformanceScreen deviceId={IPAD} />);

  expect(screen.getByTestId("cpu-chart")).toHaveTextContent(DEV_MODE);
  expect(screen.queryAllByTestId("core-row")).toHaveLength(0);
  const cpu = within(screen.getByTestId("perf-cpu"));
  expect(cpu.getAllByRole("img", { name: `Unavailable: ${DEV_MODE}` }).length).toBeGreaterThan(1);
  expect(screen.getByTestId("mem-chart")).toHaveTextContent(DEV_MODE);
  expect(
    within(screen.getByTestId("mem-pressure")).getByRole("img", {
      name: `Unavailable: ${DEV_MODE}`,
    }),
  ).toBeInTheDocument();
  expect(screen.getByTestId("mem-events")).toHaveTextContent("No memory warnings");
  expect(screen.getByTestId("thermal-temp-chart")).toHaveTextContent(SENSORS_REASON);
  expect(screen.getByTestId("thermal-events")).toHaveTextContent("No thermal state changes");
  expect(
    within(screen.getByTestId("thermal-state")).getByRole("img", {
      name: "Unavailable: No sample yet",
    }),
  ).toBeInTheDocument();
  expect(screen.getByTestId("bat-voltage")).toHaveTextContent(DEV_MODE);
  expect(screen.getByTestId("bat-current")).toHaveTextContent(DEV_MODE);
  expect(screen.getByTestId("bat-temp")).toHaveTextContent(DEV_MODE);
  expect(
    within(screen.getByTestId("bat-kv")).getAllByRole("img", { name: `Unavailable: ${DEV_MODE}` }),
  ).toHaveLength(5);
});

test("renders the active device when no id is given and nothing without one", () => {
  act(() => seedIphone());
  const { unmount } = render(<PerformanceScreen />);
  expect(screen.getAllByTestId("core-row")).toHaveLength(6);
  unmount();
  act(() => {
    dispatch({ type: "device_detached", device_id: IPHONE });
    dispatch({ type: "device_detached", device_id: IPAD });
    dispatch({ type: "device_detached", device_id: "fake:android" });
  });
  const { container } = render(<PerformanceScreen />);
  expect(container).toBeEmptyDOMElement();
});
