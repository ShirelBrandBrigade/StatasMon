import { formatValue } from "@statasmon/protocol";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { BatteryCard } from "./BatteryCard";
import { AMPS, CELSIUS, COUNT, NO_SAMPLE, PERCENT, VOLTS } from "./shared";
import { ANDROID, caps, fresh, IPAD, IPHONE, sample, seed } from "./testkit";

beforeEach(() => seed());
afterEach(() => cleanup());

/** Text of the value cell for a KeyValue row. */
function kv(label: string): string {
  const row = screen.getByText(label).closest("div");
  return row?.querySelector(".sm-kv-value")?.textContent ?? "";
}

function kvDash(label: string): HTMLElement | null {
  const row = screen.getByText(label).closest("div");
  return row?.querySelector(".sm-dash") ?? null;
}

test("charging: ring, signed current, health with cycles, temperature and time to full", () => {
  caps(IPHONE, { battery_basic: true, battery_detail: true });
  sample(IPHONE, {
    "bat.percent": 87,
    "bat.charging": 1,
    "bat.voltage": 4.21,
    "bat.current": 1.145,
    "bat.health": 96,
    "bat.cycles": 214,
    "bat.temp": 34.011,
    "bat.full_capacity": 3438.72,
  });
  render(<BatteryCard deviceId={IPHONE} />);

  expect(screen.getByText("charging")).toBeInTheDocument();
  expect(
    screen.getByRole("img", { name: `Battery ${formatValue(PERCENT, 87)}` }),
  ).toBeInTheDocument();
  expect(kv("Voltage")).toBe(formatValue(VOLTS, 4.21));
  expect(kv("Current")).toBe(`+${formatValue(AMPS, 1.145)}`);
  expect(kv("Health")).toBe(`${formatValue(PERCENT, 96)} · ${formatValue(COUNT, 214)} cyc`);
  expect(kv("Temperature")).toBe(formatValue(CELSIUS, 34.011));
  // 3438.72 mAh × 13 % remaining ÷ 1145 mA ≈ 23 min
  expect(kv("Full in")).toBe("23 min");
  expect(screen.queryByText("Time left")).toBeNull();
});

test("discharging: negative current and time left in hours and minutes", () => {
  fresh(ANDROID, "android");
  caps(ANDROID, { battery_basic: true, battery_detail: true });
  sample(ANDROID, {
    "bat.percent": 50,
    "bat.charging": 0,
    "bat.current": -0.45,
    "bat.full_capacity": 4000,
  });
  render(<BatteryCard deviceId={ANDROID} />);

  expect(screen.getByText("discharging")).toBeInTheDocument();
  expect(kv("Current")).toBe(formatValue(AMPS, -0.45));
  // 4000 mAh × 50 % ÷ 450 mA ≈ 266.7 min
  expect(kv("Time left")).toBe("4 h 27 min");
  expect(screen.queryByText("Full in")).toBeNull();
});

test("full when charging at 100 %; no estimate row", () => {
  fresh(ANDROID, "android");
  sample(ANDROID, { "bat.percent": 100, "bat.charging": 1, "bat.current": 0.05 });
  render(<BatteryCard deviceId={ANDROID} />);
  expect(screen.getByText("full")).toBeInTheDocument();
  expect(screen.queryByText("Full in")).toBeNull();
  expect(screen.queryByText("Time left")).toBeNull();
});

test("basic only: detail rows dash with the battery_detail reason, no estimate", () => {
  const reason = "Battery details need a paired connection";
  fresh(IPAD, "ios");
  caps(IPAD, { battery_basic: true, battery_detail: reason });
  sample(IPAD, { "bat.percent": 42, "bat.charging": 0 });
  render(<BatteryCard deviceId={IPAD} />);

  expect(
    screen.getByRole("img", { name: `Battery ${formatValue(PERCENT, 42)}` }),
  ).toBeInTheDocument();
  expect(screen.getByText("discharging")).toBeInTheDocument();
  for (const label of ["Voltage", "Current", "Health", "Temperature"]) {
    expect(kvDash(label)).toHaveAttribute("title", reason);
  }
  expect(screen.queryByText("Full in")).toBeNull();
  expect(screen.queryByText("Time left")).toBeNull();
});

test("no sample yet: ring and rows dash with 'No sample yet', subtitle empty", () => {
  fresh(IPAD, "ios");
  caps(IPAD, { battery_basic: true, battery_detail: true });
  render(<BatteryCard deviceId={IPAD} />);

  expect(screen.getByTestId("gauge-battery")).toHaveAttribute("title", NO_SAMPLE);
  expect(kvDash("Voltage")).toHaveAttribute("title", NO_SAMPLE);
  expect(screen.queryByText("charging")).toBeNull();
  expect(screen.queryByText("discharging")).toBeNull();
});
