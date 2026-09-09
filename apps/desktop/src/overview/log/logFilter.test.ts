import { beforeEach, describe, expect, test } from "vitest";
import {
  ALL_LEVELS,
  compileLogFilter,
  DEFAULT_LOG_FILTER,
  type LogFilterState,
  matches,
  parseQuery,
  useLogFilterStore,
} from "../../store/logFilter";
import { entry, mockupEntries } from "./testEntries";

const f = (patch: Partial<LogFilterState> = {}): LogFilterState => ({
  ...DEFAULT_LOG_FILTER,
  processes: [],
  levels: [...DEFAULT_LOG_FILTER.levels],
  ...patch,
});

describe("parseQuery", () => {
  test("empty and whitespace mean no query", () => {
    expect(parseQuery("")).toEqual({ kind: "none" });
    expect(parseQuery("   ")).toEqual({ kind: "none" });
    expect(parseQuery("//")).toEqual({ kind: "none" });
  });
  test("plain text is lower-cased", () => {
    expect(parseQuery("  Sync ")).toEqual({ kind: "text", needle: "sync" });
  });
  test("closed and unclosed regex, flags honored, g/y stripped", () => {
    const closed = parseQuery("/sync\\b/i");
    expect(closed.kind).toBe("regex");
    if (closed.kind === "regex") expect(closed.re.flags).toBe("i");
    const open = parseQuery("/sync");
    expect(open.kind).toBe("regex");
    if (open.kind === "regex") expect(open.re.source).toBe("sync");
    const sticky = parseQuery("/a/gyi");
    if (sticky.kind === "regex") expect(sticky.re.flags).toBe("i");
  });
  test("invalid regex is flagged", () => {
    expect(parseQuery("/(/")).toEqual({ kind: "invalid", source: "(" });
  });
});

describe("compileLogFilter / matches", () => {
  const rows = mockupEntries();
  const shown = (state: LogFilterState) => rows.filter((e) => matches(e, state)).map((e) => e.seq);

  test("default hides debug and nothing else", () => {
    expect(shown(f())).toEqual([1, 2, 3, 4, 5, 7, 8]);
    expect(compileLogFilter(f()).active).toBe(true);
    expect(compileLogFilter(f({ levels: [...ALL_LEVELS] })).active).toBe(false);
  });
  test("level filter: chosen levels only; empty hides everything", () => {
    expect(shown(f({ levels: ["error", "fault"] }))).toEqual([3, 7]);
    expect(shown(f({ levels: [] }))).toEqual([]);
  });
  test("process filter matches process or subsystem; empty = all", () => {
    expect(shown(f({ processes: ["FieldKit"] }))).toEqual([1, 3, 4, 7]);
    expect(shown(f({ processes: ["com.example.fieldkit"] }))).toEqual([1, 3, 4, 7, 8]);
    expect(shown(f({ processes: ["Safari"], levels: [...ALL_LEVELS] }))).toEqual([6]);
    expect(shown(f({ processes: ["nope"] }))).toEqual([]);
  });
  test("text query is a case-insensitive substring over message, process, tag, subsystem, category", () => {
    expect(shown(f({ query: "RETRY" }))).toEqual([4]);
    expect(shown(f({ query: "springboard" }))).toEqual([2]);
    expect(shown(f({ query: "fieldkit/sync" }))).toEqual([8]);
    expect(shown(f({ query: "com.example" }))).toEqual([1, 2, 3, 4, 7, 8]);
    const cat = entry(9, "Safari", "info", "x", { category: "Loading" });
    expect(matches(cat, f({ query: "loading" }))).toBe(true);
  });
  test("regex query", () => {
    expect(shown(f({ query: "/^retry \\d/i" }))).toEqual([4]);
    expect(shown(f({ query: "/^retry/" }))).toEqual([]); // case-sensitive as typed
    expect(shown(f({ query: "/warning|Fair/" }))).toEqual([5, 7]);
  });
  test("invalid regex matches nothing and sets the flag", () => {
    const c = compileLogFilter(f({ query: "/(/" }));
    expect(c.invalid).toBe(true);
    expect(rows.some((e) => c.matches(e))).toBe(false);
  });
  test("filters combine", () => {
    expect(shown(f({ processes: ["FieldKit"], levels: ["info"], query: "session" }))).toEqual([1]);
  });
});

describe("useLogFilterStore", () => {
  beforeEach(() => useLogFilterStore.getState().reset());
  const id = "fake:iphone";
  const get = () => useLogFilterStore.getState().byDevice[id];

  test("unknown device reads the default", () => {
    expect(get()).toBeUndefined();
  });
  test("toggles keep severity order and are per device", () => {
    const s = useLogFilterStore.getState();
    s.toggleLevel(id, "info");
    expect(get().levels).toEqual(["notice", "warning", "error", "fault"]);
    s.toggleLevel(id, "debug");
    expect(get().levels).toEqual(["debug", "notice", "warning", "error", "fault"]);
    s.setLevels(id, ["fault", "debug"]);
    expect(get().levels).toEqual(["debug", "fault"]);
    s.toggleProcess(id, "Safari");
    s.toggleProcess(id, "FieldKit");
    s.toggleProcess(id, "Safari");
    expect(get().processes).toEqual(["FieldKit"]);
    s.setProcesses(id, ["a", "a", "b"]);
    expect(get().processes).toEqual(["a", "b"]);
    s.setQuery(id, "/x/");
    s.setFollowTail(id, false);
    expect(get().query).toBe("/x/");
    expect(get().followTail).toBe(false);
    expect(useLogFilterStore.getState().byDevice["fake:ipad"]).toBeUndefined();
    s.reset(id);
    expect(get()).toBeUndefined();
  });
});
