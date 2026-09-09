import type { SessionDetail, SessionSummary } from "@statasmon/protocol";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bridge/commands";
import { useExportStore } from "../export/exportStore";
import { SessionsScreen } from "./SessionsScreen";

vi.mock("../bridge/commands", () => ({
  commands: {
    listSessions: vi.fn(),
    openSession: vi.fn(),
    deleteSession: vi.fn(async () => undefined),
    storeStats: vi.fn(async () => ({ root: "/store", total_bytes: 2_097_152, session_count: 3 })),
    queryHistory: vi.fn(async () => []),
  },
}));

const T0 = new Date(2026, 8, 8, 9, 46).getTime();

const mk = (over: Partial<SessionSummary>): SessionSummary => ({
  id: "s",
  device_id: "fake:iphone",
  device_name: "iPhone 16 Pro",
  model_name: "iPhone 16 Pro",
  platform: "ios",
  started_at_ms: T0,
  ended_at_ms: T0 + 60_000,
  elapsed_ms: 60_000,
  sample_count: 60,
  log_count: 300,
  event_count: 4,
  mark_count: 2,
  size_bytes: 114_000,
  live: false,
  ...over,
});

const LIVE = mk({
  id: "live-1",
  started_at_ms: T0 + 86_400_000,
  ended_at_ms: null,
  elapsed_ms: 25_000,
  live: true,
});
const STORED_A = mk({ id: "stored-a", elapsed_ms: 42 * 60_000, ended_at_ms: T0 + 42 * 60_000 });
const STORED_B = mk({
  id: "stored-b",
  device_id: "fake:android",
  device_name: "Galaxy Tab S8",
  model_name: "SM-X700",
  platform: "android",
  started_at_ms: T0 + 3_600_000,
  elapsed_ms: 12 * 60_000,
  ended_at_ms: T0 + 3_600_000 + 12 * 60_000,
});

const detailOf = (s: SessionSummary): SessionDetail => ({
  summary: s,
  info: null,
  marks: [{ ts_ms: s.started_at_ms + 10_000, label: "Mark 1" }],
  events: [
    {
      ts_ms: s.started_at_ms + 20_000,
      kind: "thermal_change",
      detail: "Nominal → Fair",
      pid: 76,
      process: "thermalmonitord",
    },
  ],
  metrics: ["mem.used", "cpu.total"],
});

const listSessions = vi.mocked(commands.listSessions);
const openSession = vi.mocked(commands.openSession);

beforeEach(() => {
  vi.clearAllMocks();
  useExportStore.getState().close();
  listSessions.mockResolvedValue([STORED_A, LIVE, STORED_B]);
  openSession.mockImplementation(async (id) => {
    const s = [LIVE, STORED_A, STORED_B].find((x) => x.id === id);
    if (!s) throw new Error("missing");
    return detailOf(s);
  });
});

afterEach(cleanup);

const flush = () => act(async () => {});

test("lists sessions newest first with counts, live badge and store totals", async () => {
  render(<SessionsScreen />);
  await flush();
  const rows = screen.getAllByTestId("session-row");
  expect(rows.map((r) => r.dataset.sessionId)).toEqual(["live-1", "stored-b", "stored-a"]);
  expect(within(rows[0]).getByText("Live")).toBeInTheDocument();
  expect(rows[0].dataset.live).toBe("true");
  expect(within(rows[0]).getByRole("button", { name: "Delete" })).toBeDisabled();
  expect(rows[2]).toHaveTextContent("60 samples");
  expect(rows[2]).toHaveTextContent("300 logs");
  expect(rows[2]).toHaveTextContent("4 events");
  expect(rows[2]).toHaveTextContent("2 marks");
  expect(rows[2]).toHaveTextContent("111.3 KB");
  expect(rows[2]).toHaveTextContent("42:00");
  expect(rows[1]).toHaveTextContent("SM-X700");
  expect(screen.getByTestId("store-stats")).toHaveTextContent("3 sessions · 2.0 MB · /store");
});

test("Delete asks inline, Keep cancels, Yes calls the core and drops the row", async () => {
  render(<SessionsScreen />);
  await flush();
  const row = screen.getAllByTestId("session-row")[2];
  fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
  expect(within(row).getByText("Delete this session?")).toBeInTheDocument();
  fireEvent.click(within(row).getByRole("button", { name: "Keep" }));
  expect(within(row).queryByText("Delete this session?")).toBeNull();
  expect(commands.deleteSession).not.toHaveBeenCalled();

  fireEvent.click(within(row).getByRole("button", { name: "Delete" }));
  listSessions.mockResolvedValue([LIVE, STORED_B]);
  await act(async () => {
    fireEvent.click(within(row).getByRole("button", { name: "Yes, delete" }));
  });
  expect(commands.deleteSession).toHaveBeenCalledWith("stored-a");
  expect(screen.getAllByTestId("session-row").map((r) => r.dataset.sessionId)).toEqual([
    "live-1",
    "stored-b",
  ]);
});

test("Open shows the read-only detail, the metric select loads history, Compare overlays a second session", async () => {
  const queryHistory = vi.mocked(commands.queryHistory);
  queryHistory.mockImplementation(async (q) => [
    { ts_ms: q.from_ms, value: 1 },
    { ts_ms: q.from_ms + 1000, value: 2 },
  ]);
  render(<SessionsScreen />);
  await flush();
  const row = screen.getAllByTestId("session-row")[2];
  await act(async () => {
    fireEvent.click(within(row).getByRole("button", { name: "Open" }));
  });
  expect(commands.openSession).toHaveBeenCalledWith("stored-a");
  const detail = screen.getByTestId("session-detail");
  expect(detail.dataset.sessionId).toBe("stored-a");
  expect(within(detail).getByText("Read-only")).toBeInTheDocument();
  expect(
    within(detail).getByText("No device information was stored with this session."),
  ).toBeInTheDocument();
  const tl = within(detail).getByRole("list", { name: "Marks and events" });
  expect(
    within(tl)
      .getAllByRole("listitem")
      .map((li) => li.dataset.kind),
  ).toEqual(["mark", "thermal_change"]);

  const metric = within(detail).getByRole("combobox", { name: "Metric" }) as HTMLSelectElement;
  expect(metric.value).toBe("cpu.total");
  expect(queryHistory).toHaveBeenLastCalledWith(
    expect.objectContaining({
      device_id: "fake:iphone",
      session_id: "stored-a",
      metric: "cpu.total",
      from_ms: STORED_A.started_at_ms,
      to_ms: STORED_A.ended_at_ms,
    }),
  );
  await act(async () => {
    fireEvent.change(metric, { target: { value: "mem.used" } });
  });
  expect(queryHistory).toHaveBeenLastCalledWith(expect.objectContaining({ metric: "mem.used" }));

  fireEvent.click(within(detail).getByRole("button", { name: "Compare" }));
  const other = within(detail).getByRole("combobox", { name: "Second session" });
  await act(async () => {
    fireEvent.change(other, { target: { value: "stored-b" } });
  });
  expect(queryHistory).toHaveBeenLastCalledWith(
    expect.objectContaining({ session_id: "stored-b", metric: "mem.used" }),
  );
  const compare = screen.getByTestId("compare-chart");
  expect(compare).toHaveTextContent("A · iPhone 16 Pro 2026-09-08");
  expect(compare).toHaveTextContent("B · Galaxy Tab S8 2026-09-08");

  fireEvent.click(within(detail).getByRole("button", { name: "Export" }));
  expect(useExportStore.getState().open).toBe(true);
  expect(useExportStore.getState().preset).toEqual({
    deviceId: "fake:iphone",
    deviceName: "iPhone 16 Pro",
    sessionId: "stored-a",
    scope: "session",
  });
});
