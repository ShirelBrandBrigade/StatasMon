import type { LogEntry, LogLevel } from "@statasmon/protocol";

const T0 = 1_757_419_200_000; // 2026-09-09T12:00:00Z

/** Builds a deterministic entry; `seq` doubles as the ms offset. */
export function entry(
  seq: number,
  process: string,
  level: LogLevel,
  message: string,
  extra: Partial<LogEntry> = {},
): LogEntry {
  return {
    seq,
    ts_ms: T0 + seq * 7,
    pid: 1842,
    process,
    tag: null,
    subsystem: process === "FieldKit" ? "com.example.fieldkit" : null,
    category: null,
    buffer: null,
    level,
    message,
    ...extra,
  };
}

/** The mockup's eight lines: one error pair, one fault, one debug (hidden by default). */
export function mockupEntries(): LogEntry[] {
  return [
    entry(1, "FieldKit", "info", "Session started · build 412 (debug) · user 8f2c"),
    entry(2, "SpringBoard", "notice", "Scene activation: com.example.fieldkit (foreground)"),
    entry(3, "FieldKit", "error", "URLSession task failed: NSURLErrorTimedOut (-1001)"),
    entry(4, "FieldKit", "info", "Retry 1/3 in 2.0 s"),
    entry(5, "thermalmonitord", "notice", "Thermal pressure level: Nominal → Fair"),
    entry(6, "Safari", "debug", "Resource load completed: 200 text/html (38 KB)"),
    entry(7, "FieldKit", "fault", "Memory warning received (level 2) · resident 318 MB"),
    entry(8, "com.example.fieldkit", "warning", "Sync: 18 pending records → POST /v2/sync", {
      tag: "FieldKit/Sync",
      buffer: "main",
    }),
  ];
}
