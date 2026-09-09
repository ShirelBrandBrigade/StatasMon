import { describe, expect, test } from "vitest";
import { compileLogFilter, DEFAULT_LOG_FILTER } from "../../store/logFilter";
import { LogRing } from "../../store/telemetry";
import { LogScanner } from "./logScan";
import { entry, mockupEntries } from "./testEntries";

const scanner = (patch = {}) =>
  new LogScanner(compileLogFilter({ ...DEFAULT_LOG_FILTER, ...patch }));

describe("LogScanner", () => {
  test("full scan then incremental append over the same mutated array", () => {
    const ring = new LogRing(100);
    const s = scanner();
    const rows = mockupEntries();
    ring.push(rows.slice(0, 4));
    const a = s.update(ring);
    expect(a.entries.map((e) => e.seq)).toEqual([1, 2, 3, 4]);
    expect(a.keys).toEqual([0, 1, 2, 3]);
    expect(a).toMatchObject({ total: 4, shown: 4, errors: 1, faults: 0, warningsSeen: false });
    expect(a.processes).toEqual([
      { process: "FieldKit", count: 3 },
      { process: "SpringBoard", count: 1 },
    ]);

    // Nothing new: same snapshot object, same entries array.
    expect(s.update(ring)).toBe(a);

    ring.push(rows.slice(4));
    const b = s.update(ring);
    expect(b).not.toBe(a);
    expect(b.entries.map((e) => e.seq)).toEqual([1, 2, 3, 4, 5, 7, 8]); // 6 is debug
    expect(b.keys).toEqual([0, 1, 2, 3, 4, 6, 7]);
    expect(b).toMatchObject({ total: 8, shown: 7, errors: 1, faults: 1, warningsSeen: true });
    expect(b.processes[0]).toEqual({ process: "FieldKit", count: 4 });
  });

  test("trims entries the ring dropped and keeps counts and keys exact", () => {
    const ring = new LogRing(3);
    const s = scanner({ levels: ["error", "fault", "info"] });
    ring.push([
      entry(1, "A", "error", "e1"),
      entry(2, "A", "fault", "f1"),
      entry(3, "A", "info", "i1"),
    ]);
    expect(s.update(ring)).toMatchObject({ errors: 1, faults: 1 });
    ring.push([entry(4, "A", "info", "i2"), entry(5, "A", "error", "e2")]);
    const r = s.update(ring);
    expect(r.entries.map((e) => e.seq)).toEqual([3, 4, 5]);
    expect(r.keys).toEqual([2, 3, 4]);
    expect(r).toMatchObject({ total: 3, shown: 3, errors: 1, faults: 0, dropped: 2 });
    // A batch larger than the capacity: everything visible is replaced.
    ring.push([6, 7, 8, 9].map((i) => entry(i, "B", "info", `i${i}`)));
    const r2 = s.update(ring);
    expect(r2.entries.map((e) => e.seq)).toEqual([7, 8, 9]);
    expect(r2.keys).toEqual([6, 7, 8]);
    expect(r2).toMatchObject({ errors: 0, faults: 0, dropped: 6 });
  });

  test("a looping replay that repeats seq still yields unique keys and no rescan", () => {
    const ring = new LogRing(100);
    const s = scanner();
    ring.push([entry(1, "A", "info", "x"), entry(2, "A", "info", "y")]);
    const r1 = s.update(ring);
    ring.push([entry(1, "A", "info", "x"), entry(2, "A", "info", "y")]);
    const r2 = s.update(ring);
    expect(r2.entries.map((e) => e.seq)).toEqual([1, 2, 1, 2]);
    expect(r2.keys).toEqual([0, 1, 2, 3]);
    expect(r2.entries.slice(0, 2)).toEqual(r1.entries);
  });

  test("a replaced ring (detach, reset) starts over", () => {
    const s = scanner();
    const ring1 = new LogRing(100);
    ring1.push([entry(10, "A", "info", "x"), entry(11, "A", "info", "y")]);
    expect(s.update(ring1).shown).toBe(2);
    const ring2 = new LogRing(100);
    ring2.push([entry(1, "B", "info", "z")]);
    const r2 = s.update(ring2);
    expect(r2.entries.map((e) => e.seq)).toEqual([1]);
    expect(r2.keys).toEqual([0]);
    expect(r2.processes).toEqual([{ process: "B", count: 1 }]);
    expect(s.update(new LogRing(100))).toMatchObject({ total: 0, shown: 0 });
  });

  test("invalid regex shows nothing and reports it", () => {
    const ring = new LogRing(100);
    ring.push(mockupEntries());
    const r = scanner({ query: "/(/" }).update(ring);
    expect(r.shown).toBe(0);
    expect(r.invalid).toBe(true);
    expect(r.total).toBe(8);
  });
});
