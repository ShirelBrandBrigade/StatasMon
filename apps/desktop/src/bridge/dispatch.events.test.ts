import { beforeEach, expect, test } from "vitest";
import { EVENT_LIMIT, useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import { dispatch } from "./dispatch";
import { synthesize } from "./synthetic";

beforeEach(() => {
  useAppStore.getState().reset();
  telemetry.reset();
  for (const { message } of synthesize(1000)) dispatch(message);
});

test("event messages are kept per device (newest EVENT_LIMIT) and counted", () => {
  for (let i = 0; i < EVENT_LIMIT + 5; i += 1) {
    dispatch({
      type: "event",
      device_id: "fake:iphone",
      ts_ms: i,
      kind: i === 0 ? "connection" : "thermal_change",
      detail: `e${i}`,
      pid: null,
      process: null,
    });
  }
  const d = useAppStore.getState().devices["fake:iphone"];
  expect(d.eventCount).toBe(EVENT_LIMIT + 5);
  expect(d.events.length).toBe(EVENT_LIMIT);
  expect(d.events[0].detail).toBe("e5");
  expect(d.events[d.events.length - 1]).toEqual({
    ts_ms: EVENT_LIMIT + 4,
    kind: "thermal_change",
    detail: `e${EVENT_LIMIT + 4}`,
    pid: null,
    process: null,
  });
  expect(useAppStore.getState().devices["fake:ipad"].events).toEqual([]);
});

test("progress messages land in the store by task id", () => {
  dispatch({ type: "progress", task_id: "export-1", pct: 10, message: "export: collecting" });
  dispatch({ type: "progress", task_id: "export-1", pct: 60, message: "export: writing" });
  dispatch({ type: "progress", task_id: "tunnel", pct: 50, message: "tunnel: connecting" });
  const p = useAppStore.getState().progress;
  expect(p["export-1"]).toMatchObject({ pct: 60, message: "export: writing" });
  expect(p["export-1"].at).toBeGreaterThan(0);
  expect(p.tunnel.pct).toBe(50);
  useAppStore.getState().reset();
  expect(useAppStore.getState().progress).toEqual({});
});
