import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, render } from "@testing-library/react";
import type uPlot from "uplot";
import { beforeAll, expect, test } from "vitest";
import { applyTokens, installCanvasStub } from "./testing/jsdomCanvas";
import type { ChartData } from "./useSeriesData";

// uPlot touches matchMedia and the canvas at import time, so the stub goes first.
const canvas = installCanvasStub();
const { formatElapsedAxis, MAX_MARKERS, TimeSeriesChart, visibleMarkers } = await import(
  "./TimeSeriesChart"
);

const TOKENS_CSS = resolve(__dirname, "../../../../packages/ui/src/tokens.css");
const SERIES = [{ id: "a", label: "CPU", color: "var(--cyan)", fill: true }];
const T0 = Date.UTC(2026, 8, 9, 12, 0, 0);

function stream(n: number): ChartData {
  const t: number[] = [];
  const a: number[] = [];
  for (let i = 0; i < n; i += 1) {
    t.push(T0 + i * 1000);
    a.push(30 + (i % 7));
  }
  return { t, v: [a] };
}

const flush = () => act(async () => {});

beforeAll(() => {
  applyTokens(readFileSync(TOKENS_CSS, "utf8"));
});

const count = (name: string) => canvas.calls.filter((c) => c === name).length;

test("draws one line and one label chip per visible marker, at most MAX_MARKERS", async () => {
  const ref: { current: uPlot | null } = { current: null };
  const markers = [
    { tsMs: T0 + 2000, label: "Mark 1", tone: "info" as const },
    { tsMs: T0 + 5000, label: "Thermal", tone: "warn" as const },
    { tsMs: T0 - 60_000, label: "Old", tone: "bad" as const }, // outside the window
  ];
  const props = {
    series: SERIES,
    data: stream(10),
    height: 90,
    windowMs: 30_000,
    instanceRef: ref,
  };
  const { rerender } = render(<TimeSeriesChart {...props} />);
  await flush();
  const u = ref.current;
  if (!u) throw new Error("no instance");
  // Baseline: what uPlot paints on its own (axes, grid) for this data.
  canvas.reset();
  u.redraw();
  await flush();
  const base = { fillRect: count("fillRect"), arcTo: count("arcTo"), fillText: count("fillText") };

  rerender(<TimeSeriesChart {...props} markers={markers} />);
  await flush();
  canvas.reset();
  u.redraw();
  await flush();
  // A 1px vertical line (fillRect) and a chip (arcTo ×4 + fill + fillText) per visible marker.
  expect(count("fillRect") - base.fillRect).toBe(2);
  expect(count("arcTo") - base.arcTo).toBe(8);
  expect(count("fillText") - base.fillText).toBe(2);

  const many = Array.from({ length: 30 }, (_, i) => ({
    tsMs: T0 + i * 250,
    label: `m${i}`,
    tone: "neutral" as const,
  }));
  const shown = visibleMarkers(many, T0 / 1000, (T0 + 9000) / 1000);
  expect(shown.length).toBe(MAX_MARKERS);
  expect(shown[shown.length - 1].label).toBe("m29");
});

test("the tooltip names the marker under the cursor and hides it elsewhere", async () => {
  const ref: { current: uPlot | null } = { current: null };
  const markers = [{ tsMs: T0 + 5000, label: "Thermal", tone: "warn" as const }];
  const { container } = render(
    <TimeSeriesChart
      series={SERIES}
      markers={markers}
      data={stream(10)}
      height={90}
      windowMs={30_000}
      instanceRef={ref}
    />,
  );
  await flush();
  const u = ref.current;
  if (!u) throw new Error("no instance");
  const chip = container.querySelector(".sm-chart-tip-marker") as HTMLElement | null;
  expect(chip).not.toBeNull();
  const atMarker = u.valToPos((T0 + 5000) / 1000, "x");
  act(() => {
    u.setCursor({ left: atMarker + 2, top: 10 });
  });
  expect(chip?.hidden).toBe(false);
  expect(chip?.textContent).toBe("Thermal");
  expect(chip?.dataset.tone).toBe("warn");
  act(() => {
    u.setCursor({ left: atMarker - 40, top: 10 });
  });
  expect(chip?.hidden).toBe(true);
});

test("formatElapsedAxis renders m:ss and h:mm:ss", () => {
  expect(formatElapsedAxis(0)).toBe("0:00");
  expect(formatElapsedAxis(65)).toBe("1:05");
  expect(formatElapsedAxis(3725)).toBe("1:02:05");
});
