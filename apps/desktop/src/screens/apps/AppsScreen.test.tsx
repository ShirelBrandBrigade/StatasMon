import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { dispatch } from "../../bridge/dispatch";
import { useExportStore } from "../../export/exportStore";
import { caps, IPHONE, proc, processes, seed } from "../../overview/cards/testkit";
import { mockupEntries } from "../../overview/log/testEntries";
import { useAppStore } from "../../store/app";
import { useLogFilterStore } from "../../store/logFilter";
import { AppsBody, AppsScreen } from "./index";
import { resetProcessHistory } from "./useProcessHistory";

const ROWS = [
  proc({
    pid: 1842,
    name: "FieldKit",
    app_id: "com.example.fieldkit",
    cpu_pct: 19.7,
    mem_bytes: 434e6,
    threads: 31,
    net_bps: 2.3e6,
    state: "foreground",
    energy: "High",
  }),
  proc({
    pid: 1207,
    name: "Safari",
    app_id: "com.apple.mobilesafari",
    cpu_pct: 5.7,
    mem_bytes: 283e6,
    threads: 22,
    net_bps: 6.6e5,
    state: "background",
    energy: "Low",
  }),
  proc({ pid: 0, name: "kernel_task", cpu_pct: 6.5, mem_bytes: 1246e6, threads: 240, net_bps: 0 }),
  proc({
    pid: 84,
    name: "mediaserverd",
    cpu_pct: null,
    mem_bytes: 99e6,
    threads: 28,
    net_bps: null,
    state: "background",
  }),
];

beforeEach(() => {
  seed(0);
  useLogFilterStore.getState().reset();
  useExportStore.getState().close();
  resetProcessHistory();
  caps(IPHONE, { processes: true, logs: true });
});

const rowNames = () =>
  screen
    .getAllByRole("row")
    .slice(1)
    .map((r) => r.querySelector("td b")?.textContent);

test("renders the table sorted by CPU, sorts on header click, searches and pins", () => {
  processes(IPHONE, ROWS);
  render(<AppsBody deviceId={IPHONE} />);
  expect(rowNames()).toEqual(["FieldKit", "kernel_task", "Safari", "mediaserverd"]);
  const cpu = screen.getByRole("columnheader", { name: /CPU/ });
  expect(cpu).toHaveAttribute("aria-sort", "descending");

  fireEvent.click(screen.getByRole("button", { name: "Memory" }));
  expect(screen.getByRole("columnheader", { name: /Memory/ })).toHaveAttribute(
    "aria-sort",
    "descending",
  );
  expect(rowNames()).toEqual(["kernel_task", "FieldKit", "Safari", "mediaserverd"]);
  fireEvent.click(screen.getByRole("button", { name: "Memory" }));
  expect(rowNames()).toEqual(["mediaserverd", "Safari", "FieldKit", "kernel_task"]);

  fireEvent.change(screen.getByRole("searchbox", { name: "Search processes" }), {
    target: { value: "com.apple" },
  });
  expect(rowNames()).toEqual(["Safari"]);
  expect(document.querySelector(".apps-count")?.textContent).toMatch(/1 of 4 rows/);
  fireEvent.change(screen.getByRole("searchbox", { name: "Search processes" }), {
    target: { value: "" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Pin kernel_task" }));
  fireEvent.click(screen.getByRole("button", { name: "Memory" }));
  expect(rowNames()).toEqual(["kernel_task", "FieldKit", "Safari", "mediaserverd"]);
  expect(screen.getByRole("button", { name: "Unpin kernel_task" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(document.querySelector('.pt-row[data-pid="0"]')).toHaveClass("is-pinned");
  // The app under test is highlighted and shows its energy label.
  const me = document.querySelector('.pt-row[data-pid="1842"]');
  expect(me).toHaveClass("is-me");
  expect(me?.querySelector(".sm-tile")).toHaveAttribute("data-me", "true");
  expect(me?.textContent).toContain("High");
  expect(me?.textContent).toContain("Foreground");
  // Unknown values are dashes with reasons.
  const md = document.querySelector('.pt-row[data-pid="84"]');
  expect(
    md?.querySelector('[aria-label="Unavailable: CPU is not reported for this process"]'),
  ).toBeInTheDocument();
});

test("selecting a row opens the detail with charts, log lines, events and actions", () => {
  processes(IPHONE, ROWS);
  act(() => dispatch({ type: "log", device_id: IPHONE, entries: mockupEntries() }));
  act(() =>
    dispatch({
      type: "event",
      device_id: IPHONE,
      ts_ms: Date.now(),
      kind: "crash",
      detail: "EXC_BAD_ACCESS in FieldKit",
      pid: 1842,
      process: "FieldKit",
    }),
  );
  render(<AppsBody deviceId={IPHONE} initialItemCount={5} />);
  expect(screen.queryByRole("region", { name: /detail/ })).not.toBeInTheDocument();
  fireEvent.click(screen.getByText("FieldKit").closest("tr") as HTMLElement);
  const detail = screen.getByRole("region", { name: "FieldKit detail" });
  expect(detail).toBeInTheDocument();
  expect(within(detail).getByText("com.example.fieldkit · pid 1842")).toBeInTheDocument();
  expect(within(detail).getByText("Foreground")).toBeInTheDocument();
  for (const name of ["cpu", "memory", "network"]) {
    expect(
      within(detail).getByTestId(`chart-${name}`).querySelector(".sm-chart"),
    ).toBeInTheDocument();
  }
  expect(within(detail).getByTestId("history-note").textContent).toMatch(/1 of 900 points/);
  // Its log: FieldKit lines by process name plus the Android-style app id line.
  const list = within(detail).getByRole("listbox", { name: "Log lines of FieldKit" });
  expect(list.dataset.count).toBe("5");
  expect(within(list).queryByText(/Scene activation/)).not.toBeInTheDocument();
  // Its events.
  expect(within(detail).getByRole("list", { name: "Process events" }).textContent).toContain(
    "EXC_BAD_ACCESS",
  );
  expect(within(detail).getByText("Crash")).toBeInTheDocument();

  fireEvent.click(within(detail).getByRole("button", { name: "Export this app's log" }));
  expect(useExportStore.getState().open).toBe(true);
  expect(useExportStore.getState().preset).toMatchObject({ deviceId: IPHONE, scope: "app" });

  fireEvent.click(within(detail).getByRole("button", { name: "Focus in log" }));
  expect(useLogFilterStore.getState().byDevice[IPHONE]?.processes).toEqual(["FieldKit"]);
  expect(window.location.hash).toBe("#logs");

  fireEvent.click(within(detail).getByRole("button", { name: "Close detail" }));
  expect(screen.queryByRole("region", { name: /detail/ })).not.toBeInTheDocument();
});

test("the detail survives the process leaving the table and says so", async () => {
  processes(IPHONE, ROWS);
  render(<AppsBody deviceId={IPHONE} />);
  fireEvent.click(screen.getByText("Safari").closest("tr") as HTMLElement);
  expect(screen.getByRole("region", { name: "Safari detail" })).toBeInTheDocument();
  act(() =>
    processes(
      IPHONE,
      ROWS.filter((r) => r.pid !== 1207),
    ),
  );
  // The table re-reads the mirror on the next animation frame.
  await act(async () => {
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
  });
  expect(document.querySelector(".pt")?.getAttribute("data-count")).toBe("3");
  const detail = screen.getByRole("region", { name: "Safari detail" });
  expect(within(detail).getByText("Exited")).toBeInTheDocument();
});

test("shows a dash with the reason when the device cannot list processes", () => {
  caps(IPHONE, { processes: "Developer Mode is off" });
  render(<AppsBody deviceId={IPHONE} />);
  expect(
    screen.getByRole("img", { name: "Unavailable: Developer Mode is off" }),
  ).toBeInTheDocument();
  expect(screen.queryByRole("table")).not.toBeInTheDocument();
});

test("AppsScreen renders the active device with the headline bar", () => {
  processes(IPHONE, ROWS);
  useAppStore.getState().setActive(IPHONE);
  render(<AppsScreen />);
  expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();
  expect(screen.getByRole("table", { name: "Processes" })).toBeInTheDocument();
});
