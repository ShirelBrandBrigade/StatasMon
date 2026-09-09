import { expect, test } from "vitest";
import { HISTORY_MAX_POINTS, historyStep, historyWindow } from "./useHistory";
import { prependHistory } from "./useSeriesData";

const T0 = Date.UTC(2026, 8, 9, 12, 0, 0);

test("historyWindow is empty while the buffer holds the whole range", () => {
  expect(
    historyWindow({
      windowMs: 60_000,
      bufferSize: 60,
      oldestLiveMs: T0 - 59_000,
      newestLiveMs: T0,
      startedAtMs: T0 - 3_600_000,
      nowMs: T0,
    }),
  ).toBeUndefined();
});

test("historyWindow covers range − buffer, ending just before the oldest live point", () => {
  expect(
    historyWindow({
      windowMs: 300_000,
      bufferSize: 10,
      oldestLiveMs: T0 - 9_000,
      newestLiveMs: T0,
      startedAtMs: T0 - 3_600_000,
      nowMs: T0 + 5,
    }),
  ).toEqual({ fromMs: T0 - 300_000, toMs: T0 - 9_001 });
});

test("historyWindow for the whole session starts at started_at; nothing without a session start", () => {
  expect(
    historyWindow({
      windowMs: undefined,
      bufferSize: 10,
      oldestLiveMs: T0 - 9_000,
      newestLiveMs: T0,
      startedAtMs: T0 - 1_800_000,
      nowMs: T0,
    }),
  ).toEqual({ fromMs: T0 - 1_800_000, toMs: T0 - 9_001 });
  expect(
    historyWindow({
      windowMs: undefined,
      bufferSize: 0,
      oldestLiveMs: undefined,
      newestLiveMs: undefined,
      startedAtMs: null,
      nowMs: T0,
    }),
  ).toBeUndefined();
});

test("historyStep keeps a query under HISTORY_MAX_POINTS points, whole seconds, never below 1 s", () => {
  expect(historyStep(0, 60_000)).toBe(1000);
  expect(historyStep(0, 3_600_000)).toBe(2000);
  expect(3_600_000 / historyStep(0, 3_600_000)).toBeLessThanOrEqual(HISTORY_MAX_POINTS);
  expect(historyStep(0, 86_400_000)).toBe(48_000);
});

test("prependHistory puts older points first and drops overlap with the live buffer", () => {
  const live = { t: [T0, T0 + 1000], v: [5, 6] };
  const history = { t: [T0 - 2000, T0 - 1000, T0, T0 + 500], v: [1, 2, 99, 98] };
  expect(prependHistory(history, live)).toEqual({
    t: [T0 - 2000, T0 - 1000, T0, T0 + 1000],
    v: [1, 2, 5, 6],
  });
  // No history, or only overlapping history: the live arrays pass through untouched.
  expect(prependHistory(undefined, live)).toBe(live);
  expect(prependHistory({ t: [], v: [] }, live)).toBe(live);
  expect(prependHistory({ t: [T0], v: [1] }, live)).toBe(live);
});
