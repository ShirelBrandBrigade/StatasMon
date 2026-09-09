import { describe, expect, test } from "vitest";
import { LogRing, MetricBuffer } from "./telemetry";

describe("MetricBuffer", () => {
  test("keeps the newest values and wraps at capacity", () => {
    const b = new MetricBuffer(3);
    b.push(1, [{ id: "cpu.total", value: 10 }]);
    b.push(2, [{ id: "cpu.total", value: 20 }]);
    b.push(3, [{ id: "cpu.total", value: 30 }]);
    b.push(4, [{ id: "cpu.total", value: 40 }]);
    expect(b.size).toBe(3);
    expect(b.latest("cpu.total")).toBe(40);
    expect(b.window("cpu.total")).toEqual({ t: [2, 3, 4], v: [20, 30, 40] });
  });

  test("missing metrics in a tick read as NaN, window respects time", () => {
    const b = new MetricBuffer(10);
    b.push(1000, [
      { id: "cpu.total", value: 1 },
      { id: "mem.used", value: 5 },
    ]);
    b.push(2000, [{ id: "cpu.total", value: 2 }]);
    expect(b.latest("mem.used")).toBeUndefined();
    expect(b.window("mem.used").v[1]).toBeNaN();
    expect(b.window("cpu.total", 500)).toEqual({ t: [2000], v: [2] });
  });
});

describe("LogRing", () => {
  test("drops the oldest lines past capacity", () => {
    const r = new LogRing(2);
    const e = (seq: number) => ({
      seq,
      ts_ms: seq,
      pid: null,
      process: "p",
      tag: null,
      subsystem: null,
      category: null,
      buffer: null,
      level: "info" as const,
      message: `m${seq}`,
    });
    r.push([e(1), e(2), e(3)]);
    expect(r.size).toBe(2);
    expect(r.droppedCount).toBe(1);
    expect(r.all().map((x) => x.seq)).toEqual([2, 3]);
  });
});
