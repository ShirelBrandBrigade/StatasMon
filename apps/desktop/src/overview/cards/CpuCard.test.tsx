import { formatValue } from "@statasmon/protocol";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { CpuCard } from "./CpuCard";
import { LOAD, NO_SAMPLE, PERCENT } from "./shared";
import { caps, fresh, IPAD, IPHONE, info, sample, seed } from "./testkit";

beforeEach(() => seed());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

test("shows the total ring, one bar per core and the load average", () => {
  info(IPHONE, { cpu_cores: 6, performance_cores: 2, efficiency_cores: 4 });
  caps(IPHONE, { cpu_total: true, cpu_per_core: true, load_average: true });
  sample(IPHONE, { "cpu.total": 49.96, "cpu.load1": 2.698 });
  render(<CpuCard deviceId={IPHONE} />);

  expect(screen.getByText("6 cores · 2P + 4E")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: `CPU ${formatValue(PERCENT, 50)}` })).toBeInTheDocument();
  expect(screen.getAllByRole("img", { name: /^[PE]\d \d+%$/ })).toHaveLength(6);
  expect(screen.getByRole("img", { name: /^P0 / })).toBeInTheDocument();
  expect(screen.getByRole("img", { name: /^E3 / })).toBeInTheDocument();
  expect(screen.getByTestId("cpu-load")).toHaveTextContent(`load 1m ${formatValue(LOAD, 2.698)}`);
  expect(screen.queryByTestId("sample-age")).toBeNull();
});

test("derives the subtitle from the per-core sample when device info is missing", () => {
  render(<CpuCard deviceId={IPHONE} />);
  expect(screen.getByText("6 cores · 2P + 4E")).toBeInTheDocument();
});

test("dashes with the capability reason when CPU is unavailable", () => {
  const reason = "Developer Mode is off on this iPad";
  fresh(IPAD, "ios");
  caps(IPAD, { cpu_total: reason, cpu_per_core: reason, load_average: reason });
  render(<CpuCard deviceId={IPAD} />);

  expect(screen.getByTestId("gauge-cpu")).toHaveAttribute("title", reason);
  expect(screen.getByRole("img", { name: `Unavailable: ${reason}` })).toBeInTheDocument();
  expect(screen.queryByTestId("cpu-load")).toBeNull();
});

test("dashes with 'No sample yet' while the capability is available but nothing arrived", () => {
  fresh(IPAD, "ios");
  caps(IPAD, { cpu_total: true, cpu_per_core: true });
  render(<CpuCard deviceId={IPAD} />);

  expect(screen.getByTestId("gauge-cpu")).toHaveAttribute("title", NO_SAMPLE);
  expect(screen.getByRole("img", { name: `Unavailable: ${NO_SAMPLE}` })).toBeInTheDocument();
});

test("shows the sample age once the newest sample is older than 5 s", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-09-09T12:00:00Z"));
  seed();
  render(<CpuCard deviceId={IPHONE} />);
  expect(screen.queryByTestId("sample-age")).toBeNull();

  act(() => {
    vi.advanceTimersByTime(4_000);
  });
  expect(screen.queryByTestId("sample-age")).toBeNull();

  act(() => {
    vi.advanceTimersByTime(8_000);
  });
  expect(screen.getByTestId("sample-age")).toHaveTextContent("12 s ago");

  act(() => {
    sample(IPHONE, { "cpu.total": 10 });
  });
  expect(screen.queryByTestId("sample-age")).toBeNull();
});
