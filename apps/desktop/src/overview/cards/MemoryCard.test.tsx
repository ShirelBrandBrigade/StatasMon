import { formatValue } from "@statasmon/protocol";
import { splitFigure } from "@statasmon/ui";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { MemoryCard } from "./MemoryCard";
import { BYTES, NO_SAMPLE, PERCENT } from "./shared";
import { ANDROID, caps, fresh, IPAD, IPHONE, sample, seed } from "./testkit";

const GiB = 2 ** 30;

beforeEach(() => seed());
afterEach(() => cleanup());

function legend(container: HTMLElement): Array<{ label: string; value: string }> {
  return [...container.querySelectorAll(".sm-legend > span")].map((el) => ({
    label: (el.textContent ?? "").replace(el.querySelector("em")?.textContent ?? "", "").trim(),
    value: el.querySelector("em")?.textContent ?? "",
  }));
}

function segments(container: HTMLElement): string[] {
  return [...container.querySelectorAll(".sm-stack > i")].map(
    (el) => el.getAttribute("data-segment") ?? "",
  );
}

test("iOS-like data: used figure, app / wired / compressed stack and legend, pressure", () => {
  caps(IPHONE, { memory_breakdown: true, memory_pressure: true });
  sample(IPHONE, {
    "mem.total": 8 * GiB,
    "mem.used": 5.4 * GiB,
    "mem.app": 3.1 * GiB,
    "mem.wired": 1.4 * GiB,
    "mem.compressed": 0.9 * GiB,
    "mem.pressure": 0,
  });
  const { container } = render(<MemoryCard deviceId={IPHONE} />);

  expect(screen.getByText(`${formatValue(BYTES, 8 * GiB)} · pressure normal`)).toBeInTheDocument();
  expect(
    screen.getByRole("img", { name: `Memory ${formatValue(PERCENT, 68)}` }),
  ).toBeInTheDocument();
  expect(screen.getByTestId("figure-number")).toHaveTextContent(
    splitFigure(formatValue(BYTES, 5.4 * GiB))?.number ?? "",
  );
  expect(container.querySelector(".sm-figure-unit")).toHaveTextContent("GB used");
  expect(segments(container)).toEqual(["App", "Wired", "Compressed"]);
  expect(legend(container)).toEqual([
    { label: "App", value: formatValue(BYTES, 3.1 * GiB) },
    { label: "Wired", value: formatValue(BYTES, 1.4 * GiB) },
    { label: "Compressed", value: formatValue(BYTES, 0.9 * GiB) },
  ]);
  expect(screen.queryByRole("img", { name: /^Unavailable/ })).toBeNull();
});

test("Android-like data: used / cached / swap", () => {
  fresh(ANDROID, "android");
  caps(ANDROID, { memory_breakdown: true });
  sample(ANDROID, {
    "mem.total": 8 * GiB,
    "mem.used": 5 * GiB,
    "mem.cached": 1.1 * GiB,
    "mem.swap": 0.7 * GiB,
    "mem.pressure": 2,
  });
  const { container } = render(<MemoryCard deviceId={ANDROID} />);

  expect(screen.getByText(`${formatValue(BYTES, 8 * GiB)} · pressure urgent`)).toBeInTheDocument();
  expect(segments(container)).toEqual(["Used", "Cached", "Swap"]);
  expect(legend(container)).toEqual([
    { label: "Used", value: formatValue(BYTES, 5 * GiB) },
    { label: "Cached", value: formatValue(BYTES, 1.1 * GiB) },
    { label: "Swap", value: formatValue(BYTES, 0.7 * GiB) },
  ]);
});

test("no breakdown yet: a single used segment and a dash for the breakdown", () => {
  fresh(ANDROID, "android");
  caps(ANDROID, { memory_breakdown: true });
  sample(ANDROID, { "mem.total": 8 * GiB, "mem.used": 4 * GiB });
  const { container } = render(<MemoryCard deviceId={ANDROID} />);

  expect(segments(container)).toEqual(["Used"]);
  expect(screen.getByRole("img", { name: `Unavailable: ${NO_SAMPLE}` })).toBeInTheDocument();
});

test("dashes with the capability reason when memory is unavailable", () => {
  const reason = "Developer Mode is off on this iPad";
  fresh(IPAD, "ios");
  caps(IPAD, { memory_breakdown: reason });
  const { container } = render(<MemoryCard deviceId={IPAD} />);

  expect(screen.getByTestId("gauge-memory")).toHaveAttribute("title", reason);
  expect(screen.getAllByRole("img", { name: `Unavailable: ${reason}` }).length).toBeGreaterThan(0);
  expect(screen.queryByTestId("figure-number")).toBeNull();
  expect(segments(container)).toEqual([]);
});
