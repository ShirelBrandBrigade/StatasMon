import type { LogEntry } from "@statasmon/protocol";

const p2 = (n: number) => (n < 10 ? `0${n}` : String(n));
const p3 = (n: number) => (n < 10 ? `00${n}` : n < 100 ? `0${n}` : String(n));

/** Local wall clock `HH:MM:SS.mmm` for a log line. */
export function formatClock(tsMs: number): string {
  const d = new Date(tsMs);
  return `${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}.${p3(d.getMilliseconds())}`;
}

/** "2 148": thousands grouped with a narrow no-break space, like the mockup. */
export function formatCount(n: number): string {
  const s = String(Math.trunc(Math.abs(n)));
  let out = "";
  for (let i = 0; i < s.length; i += 1) {
    if (i > 0 && (s.length - i) % 3 === 0) out += " ";
    out += s[i];
  }
  return n < 0 ? `-${out}` : out;
}

/** "1 error" / "11 errors". */
export function plural(n: number, one: string, many = `${one}s`): string {
  return `${formatCount(n)} ${n === 1 ? one : many}`;
}

/**
 * The app under test gets the cyan → violet tile: process "FieldKit" (iOS)
 * or an app id containing "fieldkit" (Android process, iOS subsystem).
 */
export function isAppUnderTest(e: Pick<LogEntry, "process" | "subsystem">): boolean {
  if (e.process === "FieldKit") return true;
  if (e.process.toLowerCase().includes("fieldkit")) return true;
  return e.subsystem?.toLowerCase().includes("fieldkit") === true;
}
