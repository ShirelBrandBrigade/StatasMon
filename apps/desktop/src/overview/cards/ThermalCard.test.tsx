import { formatValue } from "@statasmon/protocol";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { CELSIUS, NO_SAMPLE } from "./shared";
import { ThermalCard } from "./ThermalCard";
import { ANDROID, caps, fresh, IPAD, IPHONE, sample, seed } from "./testkit";

beforeEach(() => seed());
afterEach(() => cleanup());

function kv(label: string): string {
  const row = screen.getByText(label).closest("div");
  return row?.querySelector(".sm-kv-value")?.textContent ?? "";
}

test("iOS: state pill from thermal.state, battery temperature, sensors dash, sparkline", () => {
  const reason = "iOS exposes only the battery temperature to the host";
  caps(IPHONE, { thermal_state: true, temperature_sensors: reason });
  const now = Date.now();
  sample(IPHONE, { "thermal.state": 1, "temp.battery": 34.0 }, now - 1000);
  sample(IPHONE, { "thermal.state": 1, "temp.battery": 34.2 }, now);
  render(<ThermalCard deviceId={IPHONE} />);

  const pill = screen.getByTestId("thermal-state");
  expect(pill).toHaveTextContent("Fair");
  expect(pill).toHaveAttribute("data-tone", "warn");
  expect(kv("Battery")).toBe(formatValue(CELSIUS, 34.2));
  expect(screen.queryByText("SoC")).toBeNull();
  const dash = screen.getByText("Other sensors").closest("div")?.querySelector(".sm-dash");
  expect(dash).toHaveAttribute("title", reason);
  expect(screen.getByTestId("spark-line")).toBeInTheDocument();
});

test("iOS levels map 0..3 to Nominal / Fair / Serious / Critical with their tones", () => {
  const cases: Array<[number, string, string]> = [
    [0, "Nominal", "ok"],
    [1, "Fair", "warn"],
    [2, "Serious", "bad"],
    [3, "Critical", "bad"],
  ];
  for (const [state, label, tone] of cases) {
    sample(IPHONE, { "thermal.state": state });
    render(<ThermalCard deviceId={IPHONE} />);
    const pill = screen.getByTestId("thermal-state");
    expect(pill).toHaveTextContent(label);
    expect(pill).toHaveAttribute("data-tone", tone);
    cleanup();
  }
});

test("Android: state from thermal.raw, every readable sensor listed", () => {
  fresh(ANDROID, "android");
  caps(ANDROID, { thermal_state: true, temperature_sensors: true });
  sample(ANDROID, {
    "thermal.state": 1,
    "thermal.raw": 2,
    "temp.soc": 48.393,
    "temp.battery": 33.974,
    "temp.pmu": 40.453,
    "temp.display": 32.982,
    "temp.skin": 34.482,
  });
  render(<ThermalCard deviceId={ANDROID} />);

  const pill = screen.getByTestId("thermal-state");
  expect(pill).toHaveTextContent("Moderate");
  expect(pill).toHaveAttribute("data-tone", "warn");
  expect(kv("SoC")).toBe(formatValue(CELSIUS, 48.393));
  expect(kv("Battery")).toBe(formatValue(CELSIUS, 33.974));
  expect(kv("Power management")).toBe(formatValue(CELSIUS, 40.453));
  expect(kv("Display")).toBe(formatValue(CELSIUS, 32.982));
  expect(kv("Skin")).toBe(formatValue(CELSIUS, 34.482));
  expect(screen.queryByText("Other sensors")).toBeNull();
  // one point only: no trend line yet
  expect(screen.queryByTestId("spark-line")).toBeNull();
});

test("Android raw 5 is Emergency (bad)", () => {
  fresh(ANDROID, "android");
  sample(ANDROID, { "thermal.raw": 5 });
  render(<ThermalCard deviceId={ANDROID} />);
  const pill = screen.getByTestId("thermal-state");
  expect(pill).toHaveTextContent("Emergency");
  expect(pill).toHaveAttribute("data-tone", "bad");
});

test("state unavailable: the subtitle is a dash with the thermal_state reason", () => {
  const reason = "Thermal state is not reported yet";
  fresh(IPAD, "ios");
  caps(IPAD, { thermal_state: reason, temperature_sensors: true });
  render(<ThermalCard deviceId={IPAD} />);

  expect(screen.queryByTestId("thermal-state")).toBeNull();
  expect(screen.getByRole("img", { name: `Unavailable: ${reason}` })).toBeInTheDocument();
  expect(screen.getByText("Sensors").closest("div")?.querySelector(".sm-dash")).toHaveAttribute(
    "title",
    NO_SAMPLE,
  );
});

test("falls back to bat.temp for the battery row when temp.battery is absent", () => {
  fresh(IPHONE, "ios");
  sample(IPHONE, { "thermal.state": 0, "bat.temp": 30.5 });
  render(<ThermalCard deviceId={IPHONE} />);
  expect(kv("Battery")).toBe(formatValue(CELSIUS, 30.5));
});
