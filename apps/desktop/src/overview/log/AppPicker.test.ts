import type { ProcessRow } from "@statasmon/protocol";
import { expect, test } from "vitest";
import { pickerRows } from "./AppPicker";

const row = (name: string, app_id: string | null): ProcessRow => ({
  pid: 1,
  name,
  app_id,
  cpu_pct: null,
  mem_bytes: null,
  threads: null,
  net_bps: null,
  state: "foreground",
  energy: null,
});

test("pickerRows: apps first (app under test, then by lines), daemons after, search narrows", () => {
  const counts = [
    { process: "runningboardd", count: 900 },
    { process: "Maps", count: 12 },
    { process: "FieldKit", count: 4 },
    { process: "lockdownd", count: 300 },
  ];
  const running = [
    row("Maps", "com.apple.Maps"),
    row("Photos", "com.apple.mobileslideshow"),
    row("runningboardd", null),
  ];
  const all = pickerRows(counts, running, "");
  expect(all.apps.map((r) => [r.process, r.count, r.me, r.running])).toEqual([
    ["FieldKit", 4, true, false],
    ["Maps", 12, false, true],
    ["Photos", 0, false, true],
  ]);
  expect(all.system.map((r) => r.process)).toEqual(["runningboardd", "lockdownd"]);
  expect(all.system[0].running).toBe(true);
  const q = pickerRows(counts, running, "slideshow");
  expect(q.apps.map((r) => r.process)).toEqual(["Photos"]);
  expect(q.system).toEqual([]);
});
