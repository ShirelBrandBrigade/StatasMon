import { formatValue } from "@statasmon/protocol";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { COUNT, NO_SAMPLE, PERCENT } from "./shared";
import { TopAppsCard } from "./TopAppsCard";
import { caps, fresh, IPAD, IPHONE, proc, processes, sample, seed } from "./testkit";

beforeEach(() => seed());
afterEach(() => cleanup());

const ROWS = [
  proc({ pid: 1842, name: "FieldKit", app_id: "com.example.fieldkit", cpu_pct: 19.707 }),
  proc({ pid: 1207, name: "Safari", app_id: "com.apple.mobilesafari", cpu_pct: 5.702 }),
  proc({ pid: 0, name: "kernel_task", cpu_pct: 6.543 }),
  proc({ pid: 84, name: "mediaserverd", cpu_pct: 2.94 }),
  proc({ pid: 61, name: "SpringBoard", app_id: "com.apple.springboard", cpu_pct: 2.203 }),
  proc({ pid: 58, name: "backboardd", cpu_pct: 3.049 }),
  proc({ pid: 97, name: "locationd", cpu_pct: 0.498 }),
  proc({ pid: 110, name: "wifid", cpu_pct: 1.909 }),
];

test("six rows sorted by CPU with tiles, bars and percents", () => {
  caps(IPHONE, { processes: true });
  sample(IPHONE, { "proc.count": 187 });
  processes(IPHONE, ROWS);
  render(<TopAppsCard deviceId={IPHONE} />);

  expect(screen.getByText(`by CPU · ${formatValue(COUNT, 187)} processes`)).toBeInTheDocument();
  const items = screen.getAllByRole("listitem");
  expect(items).toHaveLength(6);
  expect(items.map((li) => li.querySelector("b")?.textContent)).toEqual([
    "FieldKit",
    "kernel_task",
    "Safari",
    "backboardd",
    "mediaserverd",
    "SpringBoard",
  ]);
  expect(items[0].querySelector(".sm-tile")).toHaveAttribute("data-me", "true");
  expect(items[1].querySelector(".sm-tile")).not.toHaveAttribute("data-me");
  expect(items[0].querySelector("em")).toHaveTextContent(formatValue(PERCENT, 19.707));
  expect(items[2].querySelector("em")).toHaveTextContent(formatValue(PERCENT, 5.702));
  const width = (li: HTMLElement) =>
    Number.parseFloat(li.querySelector<HTMLElement>(".ov-bar > i")?.style.width ?? "");
  expect(width(items[0])).toBeCloseTo(100, 1);
  expect(width(items[1])).toBeCloseTo((6.543 / 19.707) * 100, 1);
});

test("a process without CPU sorts last and shows a dash", () => {
  processes(IPHONE, [
    proc({ pid: 1, name: "a", cpu_pct: 1 }),
    proc({ pid: 2, name: "unknown", cpu_pct: null }),
    proc({ pid: 3, name: "b", cpu_pct: 2 }),
  ]);
  render(<TopAppsCard deviceId={IPHONE} />);
  const items = screen.getAllByRole("listitem");
  expect(items.map((li) => li.querySelector("b")?.textContent)).toEqual(["b", "a", "unknown"]);
  expect(items[2].querySelector(".sm-dash")).toBeInTheDocument();
});

test("dashes with the capability reason when processes are unavailable", () => {
  const reason = "Developer Mode is off on this iPad";
  fresh(IPAD, "ios");
  caps(IPAD, { processes: reason });
  render(<TopAppsCard deviceId={IPAD} />);

  expect(screen.getByText("by CPU")).toBeInTheDocument();
  expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  expect(screen.getByRole("img", { name: `Unavailable: ${reason}` })).toBeInTheDocument();
});

test("dashes with 'No sample yet' while processes are available but none arrived", () => {
  fresh(IPAD, "ios");
  caps(IPAD, { processes: true });
  render(<TopAppsCard deviceId={IPAD} />);
  expect(screen.getByRole("img", { name: `Unavailable: ${NO_SAMPLE}` })).toBeInTheDocument();
});
