import type { LogEntry } from "@statasmon/protocol";
import { expect, test } from "vitest";
import { buildStories, cleanMessage, explain, ROUTINE_TITLE } from "./logExplain";

const line = (
  process: string,
  message: string,
  level: LogEntry["level"] = "notice",
  ts_ms = 1_000,
): LogEntry => ({
  seq: ts_ms,
  ts_ms,
  pid: 1,
  process,
  tag: null,
  subsystem: null,
  category: null,
  buffer: null,
  level,
  message,
});

test("explain turns the iPad's common lines into plain sentences", () => {
  expect(
    explain(
      line(
        "runningboardd",
        "[osservice<com.apple.mediaplaybackd>:432] is not RunningBoard jetsam managed.",
      ),
    ),
  ).toMatchObject({ rule: "rb-housekeeping", routine: true });
  expect(
    explain(
      line(
        "duetexpertd",
        "ATXMemoryPressureMonitor: received memory pressure warning of type: critical",
      ),
    ),
  ).toMatchObject({
    rule: "memory-pressure",
    tone: "bad",
    routine: false,
    title: "Memory is running critically low",
  });
  expect(explain(line("thermalmonitord", "thermal state: Serious"))).toMatchObject({
    tone: "warn",
    title: "The device is running hot and slowing itself down",
  });
  expect(
    explain(
      line(
        "kernel",
        "memorystatus: killing_specific_process pid 611 [MobileSafari] (vm-pageshortage)",
      ),
    ),
  ).toMatchObject({ tone: "bad", title: "The system closed MobileSafari to free memory" });
  expect(
    explain(
      line(
        "osanalyticsd",
        "Saved type '309' report (2 of max 25) at /private/var/mobile/Library/Logs/CrashReporter/FieldKit-2026-09-09-101010.ips",
      ),
    ),
  ).toMatchObject({ tone: "bad", title: "FieldKit crashed; a crash report was saved" });
  expect(explain(line("wifid", "LQM-WiFi: WiFi Latency: TxLat-hist", "error"))).toMatchObject({
    routine: true,
  });
  // wifid's own error-level chatter is routine; a real policy line is not.
  expect(
    explain(
      line(
        "wifid",
        "__WiFiDeviceManagerReleaseWakeAssertionForAutoJoin: PM assertion not held.",
        "error",
      ),
    ),
  ).toMatchObject({ routine: true });
  expect(
    explain(line("runningboardd", "Applying jetsam policy to [app<com.apple.mobilesafari>:611]")),
  ).toMatchObject({
    rule: "jetsam-policy",
    title: "mobilesafari was marked for closing if memory runs short",
  });
  expect(explain(line("kernel", "if_updown: utun6 is not attached"))).toMatchObject({
    title: "Network interface utun6 went down",
  });
  expect(explain(line("FieldKit", "upload failed: timeout", "error"))).toMatchObject({
    rule: "error",
    title: "FieldKit ran into an error",
    detail: "upload failed: timeout",
    routine: false,
  });
  expect(
    explain(
      line("system_server", "ActivityManager: Start proc 1234:com.example.maps/u0a12 for activity"),
    ),
  ).toMatchObject({
    title: "Opened maps",
  });
  expect(explain(line("SomeDaemon", "handle_get_value: <private>"))).toMatchObject({
    routine: true,
  });
  expect(cleanMessage("value <private>  here")).toBe("value [redacted by the device] here");
});

test("buildStories folds repeats and routine runs, and keeps the raw lines", () => {
  const entries = [
    line("trustd", "invalidated after getting a no-senders notification", "notice", 1_000),
    line("lockdownd", "handle_get_value: <private>", "notice", 1_500),
    line("wifid", "LQM-WiFi: WiFi Latency", "error", 2_000),
    line("duetexpertd", "received memory pressure warning of type: critical", "notice", 3_000),
    line("duetexpertd", "received memory pressure warning of type: critical", "notice", 4_000),
    line("FieldKit", "upload failed", "error", 30_000),
    line("FieldKit", "upload failed", "error", 31_000),
    line("FieldKit", "upload failed", "error", 60_000),
  ];
  const keys = entries.map((_, i) => 100 + i);
  const stories = buildStories(entries, keys);
  expect(stories.map((s) => [s.title, s.count, s.key])).toEqual([
    [ROUTINE_TITLE, 3, 100],
    ["Memory is running critically low", 2, 103],
    ["FieldKit ran into an error", 2, 105],
    ["FieldKit ran into an error", 1, 107],
  ]);
  expect(stories[0].processes).toEqual(["trustd", "lockdownd", "wifid"]);
  expect(stories[0].samples).toHaveLength(3);
  expect(stories[1].firstMs).toBe(3_000);
  expect(stories[1].lastMs).toBe(4_000);
});
