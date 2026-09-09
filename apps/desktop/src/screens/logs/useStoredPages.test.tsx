import type { LogFilter, LogPage } from "@statasmon/protocol";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { entry } from "../../overview/log/testEntries";
import type { Narrowing } from "./logsModel";
import { useStoredPages } from "./useStoredPages";

/**
 * The spy resolves to a page, or to an Error the wrapper below throws: a spy
 * that returns a rejected promise trips vitest's settled-result tracking,
 * which reports the rejection as unhandled although the hook catches it.
 */
const queryLogs =
  vi.fn<(f: LogFilter, offset: number, limit: number) => Promise<LogPage | Error>>();

vi.mock("../../bridge/commands", () => ({
  commands: {
    queryLogs: async (f: LogFilter, offset: number, limit: number) => {
      const r = await queryLogs(f, offset, limit);
      if (r instanceof Error) throw r;
      return r;
    },
  },
}));

const FILTER: LogFilter = {
  device_id: "fake:iphone",
  session_id: "s1",
  processes: [],
  levels: ["info"],
  query: null,
  regex: false,
  from_ms: null,
  to_ms: 999,
  buffers: [],
};

/** Narrowing that reproduces FILTER's bounds, so calls can be asserted against FILTER. */
const NARROW: Narrowing = { toMs: 999, buffers: [] };

/** A store holding `total` matching lines; pages are sliced from it. */
function store(total: number) {
  const all = Array.from({ length: total }, (_, i) =>
    entry(i + 1, "FieldKit", "info", `line ${i + 1}`),
  );
  queryLogs.mockImplementation(async (_f, offset, limit) => ({
    total,
    first_seq: all[offset]?.seq ?? 0,
    entries: all.slice(offset, offset + limit),
  }));
}

beforeEach(() => queryLogs.mockReset());

test("probes the total, then loads the last page and pages backwards until exhausted", async () => {
  store(1_234);
  const { result } = renderHook(() => useStoredPages(FILTER, NARROW, undefined, true));
  expect(result.current.state.total).toBeUndefined();
  expect(result.current.exhausted).toBe(false);

  act(() => result.current.loadOlder());
  expect(result.current.state.loading).toBe(true);
  await waitFor(() => expect(result.current.state.loading).toBe(false));
  expect(queryLogs).toHaveBeenNthCalledWith(1, FILTER, 0, 1);
  expect(queryLogs).toHaveBeenNthCalledWith(2, FILTER, 734, 500);
  expect(result.current.state).toMatchObject({ total: 1_234, offset: 734 });
  expect(result.current.state.entries).toHaveLength(500);
  expect(result.current.state.entries[0].seq).toBe(735);
  expect(result.current.state.entries[499].seq).toBe(1_234);

  act(() => result.current.loadOlder());
  await waitFor(() => expect(result.current.state.entries).toHaveLength(1_000));
  expect(queryLogs).toHaveBeenNthCalledWith(3, FILTER, 234, 500);
  expect(result.current.state.entries[0].seq).toBe(235);
  expect(result.current.state.offset).toBe(234);

  act(() => result.current.loadOlder());
  await waitFor(() => expect(result.current.state.entries).toHaveLength(1_234));
  expect(queryLogs).toHaveBeenNthCalledWith(4, FILTER, 0, 234);
  expect(result.current.state.entries[0].seq).toBe(1);
  expect(result.current.exhausted).toBe(true);

  // Nothing older: no further call.
  act(() => result.current.loadOlder());
  expect(queryLogs).toHaveBeenCalledTimes(4);
});

test("an empty store is exhausted after the probe, without a second call", async () => {
  store(0);
  const { result } = renderHook(() => useStoredPages(FILTER, NARROW, undefined, true));
  act(() => result.current.loadOlder());
  await waitFor(() => expect(result.current.state.total).toBe(0));
  expect(queryLogs).toHaveBeenCalledTimes(1);
  expect(result.current.exhausted).toBe(true);
});

test("a changed query forgets the loaded pages; disabled paging never queries", async () => {
  store(10);
  const { result, rerender } = renderHook(
    ({ f, on }: { f: LogFilter; on: boolean }) => useStoredPages(f, NARROW, undefined, on),
    { initialProps: { f: FILTER, on: true } },
  );
  act(() => result.current.loadOlder());
  await waitFor(() => expect(result.current.state.entries).toHaveLength(10));
  rerender({ f: { ...FILTER, query: "x" }, on: true });
  expect(result.current.state.entries).toHaveLength(0);
  expect(result.current.state.total).toBeUndefined();

  rerender({ f: FILTER, on: false });
  act(() => result.current.loadOlder());
  expect(queryLogs).toHaveBeenCalledTimes(2);
});

test("a failed query surfaces its message and stops loading", async () => {
  queryLogs.mockResolvedValue(new Error("store is locked"));
  const { result } = renderHook(() => useStoredPages(FILTER, NARROW, undefined, true));
  act(() => result.current.loadOlder());
  await waitFor(() => expect(result.current.state.loading).toBe(false));
  expect(result.current.state.error).toBe("store is locked");
  expect(result.current.state.entries).toHaveLength(0);
});
