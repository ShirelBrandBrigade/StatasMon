import type { DeviceAction, LogFilter, LogPage } from "@statasmon/protocol";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { dispatch } from "../../bridge/dispatch";
import { useExportStore } from "../../export/exportStore";
import { caps, IPHONE, seed } from "../../overview/cards/testkit";
import { entry, mockupEntries } from "../../overview/log/testEntries";
import { useLogFilterStore } from "../../store/logFilter";
import { LogsBody } from "./index";
import { toLocalDateTime } from "./logsModel";

const queryLogs = vi.fn<(f: LogFilter, offset: number, limit: number) => Promise<LogPage>>();
const deviceAction = vi.fn<(id: string, a: DeviceAction) => Promise<unknown>>();

vi.mock("../../bridge/commands", () => ({
  commands: {
    queryLogs: (f: LogFilter, o: number, l: number) => queryLogs(f, o, l),
    deviceAction: (id: string, a: DeviceAction) => deviceAction(id, a),
  },
}));

beforeEach(() => {
  seed(0);
  useLogFilterStore.getState().reset();
  useExportStore.getState().close();
  queryLogs.mockReset();
  deviceAction.mockReset();
  caps(IPHONE, { logs: true });
});

const list = () => screen.getByRole("listbox", { name: /Log entries/ });
const toolbar = () => screen.getByRole("toolbar", { name: "Log filters" });
const seedLog = () =>
  act(() => dispatch({ type: "log", device_id: IPHONE, entries: mockupEntries() }));

/** Lets the rAF-throttled log tick fire. */
async function frame() {
  await act(async () => {
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
  });
}

test("search accepts plain text and /regex/, flags an invalid regex, Escape clears", () => {
  seedLog();
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  expect(list().dataset.count).toBe("7");
  const search = screen.getByRole("searchbox", { name: "Search log" });
  fireEvent.change(search, { target: { value: "retry" } });
  expect(list().dataset.count).toBe("1");
  fireEvent.change(search, { target: { value: "/urlsession|thermal/i" } });
  expect(list().dataset.count).toBe("2");
  fireEvent.change(search, { target: { value: "/(/" } });
  expect(search).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByText("invalid regex")).toBeInTheDocument();
  fireEvent.keyDown(search, { key: "Escape" });
  expect(search).toHaveValue("");
  expect(list().dataset.count).toBe("7");
});

test("level toggles and the follow control drive the store; Space toggles follow on the list", () => {
  seedLog();
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  fireEvent.click(within(toolbar()).getByRole("button", { name: "Error" }));
  expect(list().dataset.count).toBe("6");
  const follow = screen.getByRole("button", { name: /Following|Follow tail/ });
  expect(follow).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(follow);
  expect(useLogFilterStore.getState().byDevice[IPHONE]?.followTail).toBe(false);
  expect(list().dataset.follow).toBe("paused");
  fireEvent.keyDown(list(), { key: " " });
  expect(useLogFilterStore.getState().byDevice[IPHONE]?.followTail).toBe(true);
});

test("buffer chips appear only once a line carries a buffer, and narrow the list", async () => {
  act(() => dispatch({ type: "log", device_id: IPHONE, entries: mockupEntries().slice(0, 7) }));
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  expect(screen.queryByTestId("buffers")).not.toBeInTheDocument();
  act(() =>
    dispatch({
      type: "log",
      device_id: IPHONE,
      entries: [
        mockupEntries()[7],
        entry(9, "system_server", "info", "boot complete", { buffer: "system" }),
      ],
    }),
  );
  await frame();
  const buffers = screen.getByTestId("buffers");
  expect(
    within(buffers)
      .getAllByRole("button")
      .map((b) => b.textContent),
  ).toEqual(["main", "system"]);
  // 6 default-visible lines from the first batch (debug hidden) + 2.
  expect(list().dataset.count).toBe("8");
  fireEvent.click(within(buffers).getByRole("button", { name: "system" }));
  expect(list().dataset.count).toBe("1");
  expect(list().dataset.narrowed).toBe("true");
  fireEvent.click(within(buffers).getByRole("button", { name: "main" }));
  expect(list().dataset.count).toBe("2");
  fireEvent.click(within(buffers).getByRole("button", { name: "main" }));
  fireEvent.click(within(buffers).getByRole("button", { name: "system" }));
  expect(list().dataset.count).toBe("8");
});

test("the time range narrows to lines between From and To; Live clears it", () => {
  // Lines 10 s apart, since the inputs resolve to whole seconds.
  const base = new Date(2026, 8, 9, 12, 0, 0).getTime();
  const rows = Array.from({ length: 7 }, (_, i) =>
    entry(i + 1, "FieldKit", "info", `line ${i + 1}`, { ts_ms: base + i * 10_000 }),
  );
  act(() => dispatch({ type: "log", device_id: IPHONE, entries: rows }));
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  expect(list().dataset.count).toBe("7");
  fireEvent.change(screen.getByLabelText("From"), {
    target: { value: toLocalDateTime(rows[2].ts_ms) },
  });
  expect(list().dataset.count).toBe("5");
  fireEvent.change(screen.getByLabelText("To"), {
    target: { value: toLocalDateTime(rows[4].ts_ms) },
  });
  expect(list().dataset.count).toBe("3");
  expect(document.querySelector(".sm-card-footer")?.textContent).toContain("3 shown");
  fireEvent.click(screen.getByRole("button", { name: "Live" }));
  expect(list().dataset.count).toBe("7");
  expect(list().dataset.narrowed).toBeUndefined();
});

test("app tabs show the app under test and the selection; the picker selects or adds an app", () => {
  seedLog();
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  const tabs = screen.getByRole("tablist", { name: "Apps" });
  const names = () =>
    within(tabs)
      .getAllByRole("tab")
      .map((t) => t.textContent);
  // System processes (Safari, SpringBoard) get no tab of their own.
  expect(names()).toEqual(["All", "FieldKit4", "com.example.fieldkit1"]);
  expect(within(tabs).getByRole("tab", { name: /^All/ })).toHaveAttribute("aria-selected", "true");
  expect(
    within(tabs)
      .getByRole("tab", { name: /FieldKit/ })
      .querySelector(".sm-tile"),
  ).toHaveAttribute("data-me", "true");

  // The picker lists every process that logged; a click picks that app alone.
  fireEvent.click(screen.getByRole("button", { name: "Filter by app" }));
  const dialog = screen.getByRole("dialog", { name: "Filter by app" });
  const options = () =>
    within(dialog)
      .getAllByRole("option")
      .map((o) => o.getAttribute("data-process"));
  expect(options()).toEqual([
    "FieldKit",
    "com.example.fieldkit",
    "Safari",
    "SpringBoard",
    "thermalmonitord",
  ]);
  fireEvent.change(within(dialog).getByRole("searchbox", { name: "Find an app" }), {
    target: { value: "spring" },
  });
  expect(options()).toEqual(["SpringBoard"]);
  fireEvent.click(within(dialog).getByRole("option", { name: /SpringBoard/ }));
  expect(screen.queryByRole("dialog", { name: "Filter by app" })).toBeNull();
  expect(useLogFilterStore.getState().byDevice[IPHONE]?.processes).toEqual(["SpringBoard"]);
  expect(names()).toEqual(["All", "FieldKit4", "com.example.fieldkit1", "SpringBoard1"]);
  expect(within(tabs).getByRole("tab", { name: /^SpringBoard/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(list().dataset.count).toBe("1");

  // Shift-click in the picker adds; Safari's only line is debug (hidden by default).
  fireEvent.click(screen.getByRole("button", { name: "Filter by app" }));
  fireEvent.click(screen.getByRole("option", { name: /Safari/ }), { shiftKey: true });
  expect(useLogFilterStore.getState().byDevice[IPHONE]?.processes).toEqual([
    "SpringBoard",
    "Safari",
  ]);
  expect(screen.getByRole("dialog", { name: "Filter by app" })).toBeInTheDocument();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Filter by app" })).toBeNull();
  expect(screen.getByTestId("custom-selection").textContent).toContain("2 apps selected");
  fireEvent.click(within(tabs).getByRole("tab", { name: /^All/ }));
  expect(list().dataset.count).toBe("7");
});

test("`/` focuses the search from outside inputs", () => {
  seedLog();
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  const search = screen.getByRole("searchbox", { name: "Search log" });
  fireEvent.keyDown(list(), { key: "/" });
  expect(document.activeElement).toBe(search);
});

test("selection + Export selection opens the dialog with the seqs", () => {
  seedLog();
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  const rows = screen.getAllByRole("option");
  fireEvent.click(rows[0]);
  fireEvent.click(rows[2]);
  expect(document.querySelector(".sm-card-footer")?.textContent).toContain("2 selected");
  fireEvent.click(screen.getByRole("button", { name: "Export selection" }));
  const preset = useExportStore.getState().preset;
  expect(preset.scope).toBe("selection");
  expect(preset.seqs).toEqual([1, 3]);
});

test("Crashes & ANRs lists crash events, fetches reports, and narrows around one", async () => {
  seedLog();
  const at = mockupEntries()[6].ts_ms;
  act(() =>
    dispatch({
      type: "event",
      device_id: IPHONE,
      ts_ms: at,
      kind: "anr",
      detail: "Input dispatching timed out (com.example.fieldkit)",
      pid: 4123,
      process: "com.example.fieldkit",
    }),
  );
  act(() =>
    dispatch({
      type: "event",
      device_id: IPHONE,
      ts_ms: at + 1,
      kind: "thermal_change",
      detail: "Nominal → Fair",
      pid: null,
      process: null,
    }),
  );
  deviceAction.mockResolvedValue(["a.ips", "b.ips", "c.ips"]);
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  const strip = screen.getByTestId("crash-strip");
  expect(strip.dataset.count).toBe("1");
  expect(within(strip).getByText("ANR")).toBeInTheDocument();
  expect(within(strip).getByText(/Input dispatching/)).toBeInTheDocument();

  fireEvent.click(within(strip).getByRole("button", { name: "Fetch crash reports" }));
  await waitFor(() =>
    expect(screen.getByTestId("crash-fetch-result")).toHaveTextContent("3 reports"),
  );
  expect(deviceAction).toHaveBeenCalledWith(IPHONE, { action: "fetch_crash_reports" });

  fireEvent.click(within(strip).getByRole("button", { name: "Show log around it" }));
  // jsdom normalizes datetime-local values with milliseconds; compare the prefix.
  expect(
    (screen.getByLabelText("From") as HTMLInputElement).value.startsWith(
      toLocalDateTime(at - 5000),
    ),
  ).toBe(true);
  expect(list().dataset.narrowed).toBe("true");
  expect(useLogFilterStore.getState().byDevice[IPHONE]?.followTail).toBe(false);
});

test("stored session: Load older prepends pages from the core, keyed apart from live rows", async () => {
  seedLog();
  render(<LogsBody deviceId={IPHONE} initialItemCount={7} />);
  act(() =>
    dispatch({
      type: "session",
      device_id: IPHONE,
      session: {
        id: null,
        status: "idle",
        started_at_ms: null,
        elapsed_ms: 0,
        sample_count: 0,
        log_count: 0,
        marks: [],
      },
    }),
  );
  expect(screen.getByTestId("stored-session").textContent).toContain("Start a recording");

  act(() =>
    dispatch({
      type: "session",
      device_id: IPHONE,
      session: {
        id: "sess-1",
        status: "recording",
        started_at_ms: 0,
        elapsed_ms: 0,
        sample_count: 0,
        log_count: 0,
        marks: [],
      },
    }),
  );
  const older = [
    entry(-3, "FieldKit", "info", "older 1"),
    entry(-2, "FieldKit", "info", "older 2"),
    entry(-1, "FieldKit", "info", "older 3"),
  ];
  queryLogs.mockImplementation(async (_f, offset, limit) => ({
    total: 3,
    first_seq: older[offset]?.seq ?? 0,
    entries: older.slice(offset, offset + limit),
  }));
  const button = screen.getByRole("button", { name: "Load 500 older" });
  expect(button).toBeEnabled();
  fireEvent.click(button);
  await waitFor(() =>
    expect(screen.getByTestId("stored-status")).toHaveTextContent("3 loaded · 0 older"),
  );
  const [probe] = queryLogs.mock.calls[0];
  expect(probe).toMatchObject({
    device_id: IPHONE,
    session_id: "sess-1",
    levels: ["info", "notice", "warning", "error", "fault"],
    to_ms: mockupEntries()[0].ts_ms - 1,
  });
  expect(queryLogs.mock.calls[1].slice(1)).toEqual([0, 3]);
  expect(list().dataset.count).toBe("10");
  expect(screen.getByRole("button", { name: "Load 500 older" })).toBeDisabled();
  expect(document.querySelector(".sm-card-footer")?.textContent).toContain("3 from the store");
});

test("shows a dash with the reason when the device cannot provide logs", () => {
  caps(IPHONE, { logs: "Not paired" });
  render(<LogsBody deviceId={IPHONE} />);
  expect(screen.getByRole("img", { name: "Unavailable: Not paired" })).toBeInTheDocument();
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
});
