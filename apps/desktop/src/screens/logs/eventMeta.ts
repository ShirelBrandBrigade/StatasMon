import type { EventKind, StoredEvent } from "@statasmon/protocol";
import type { Tone } from "@statasmon/ui";

/** Label and tone per event kind (status is color + label, never color alone). */
const META: Record<EventKind, { label: string; tone: Tone }> = {
  crash: { label: "Crash", tone: "bad" },
  anr: { label: "ANR", tone: "bad" },
  lmk_kill: { label: "LMK kill", tone: "warn" },
  jetsam: { label: "Jetsam", tone: "warn" },
  memory_warning: { label: "Memory warning", tone: "warn" },
  thermal_change: { label: "Thermal", tone: "warn" },
  app_launch: { label: "Launch", tone: "info" },
  app_exit: { label: "Exit", tone: "neutral" },
  mark: { label: "Mark", tone: "note" },
  connection: { label: "Connection", tone: "neutral" },
};

export function eventMeta(kind: EventKind): { label: string; tone: Tone } {
  return META[kind];
}

/** Crash reports, tombstones and ANRs (FEATURES.md §9): the events the Logs strip lists. */
export const CRASH_KINDS: ReadonlySet<EventKind> = new Set<EventKind>(["crash", "anr"]);

export function isCrashEvent(e: Pick<StoredEvent, "kind">): boolean {
  return CRASH_KINDS.has(e.kind);
}

/** Events that belong to one process: same pid, or the same process name when the pid is unknown. */
export function eventsForProcess(
  events: readonly StoredEvent[],
  pid: number,
  names: readonly string[],
): StoredEvent[] {
  return events.filter(
    (e) => (e.pid !== null && e.pid === pid) || (e.process !== null && names.includes(e.process)),
  );
}

/** A stable React key for a stored event (events carry no id). */
export function eventKey(e: StoredEvent): string {
  return `${e.ts_ms}:${e.kind}:${e.pid ?? ""}:${e.process ?? ""}:${e.detail}`;
}
