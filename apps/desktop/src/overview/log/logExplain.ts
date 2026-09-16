import { type LogEntry, shortName } from "@statasmon/protocol";
import { formatCount } from "./format";

/**
 * Simple mode for the log (ADR-020): every line is matched against a table
 * of rules that know what the common iOS and Android messages mean, and
 * turned into a plain sentence with a tone. Routine chatter (service
 * connections, property reads, housekeeping) is marked `routine` so the
 * view can fold a run of it into one line. Nothing leaves the device: the
 * rules are local and the raw line is always one click away.
 */

export type StoryTone = "ok" | "info" | "warn" | "bad" | "neutral";

export interface Explanation {
  /** Rule id; equal ids group. */
  rule: string;
  /** Plain sentence, no identifiers a user would not recognise. */
  title: string;
  /** The raw message, cleaned, when it adds something. */
  detail?: string;
  tone: StoryTone;
  /** Housekeeping the user rarely needs; folded into "Routine activity". */
  routine: boolean;
}

/** `app<com.apple.mobilesafari>` / `[app<…>:123]` → "mobilesafari". */
function appIn(message: string): string | undefined {
  const m = message.match(/(?:app|osservice|xpcservice)<([^>(:]+)/);
  return m ? shortName(m[1]) : undefined;
}

/** The device redacts values as `<private>`; say so in words. */
export function cleanMessage(message: string): string {
  return message
    .replace(/<private>/g, "[redacted by the device]")
    .replace(/\s+/g, " ")
    .trim();
}

const THERMAL_WORD: Record<string, string> = {
  nominal: "at a normal temperature",
  fair: "a little warm",
  serious: "hot and slowing itself down",
  critical: "critically hot",
};

const THERMAL_TONE: Record<string, StoryTone> = {
  nominal: "ok",
  fair: "info",
  serious: "warn",
  critical: "bad",
};

interface Rule {
  id: string;
  /** Process names this rule applies to (lower-case); empty = any. */
  process?: readonly string[];
  match: RegExp;
  explain: (m: RegExpMatchArray, e: LogEntry) => Omit<Explanation, "rule">;
}

const RULES: readonly Rule[] = [
  // --- iOS: things worth knowing ---
  {
    id: "thermal",
    process: ["thermalmonitord"],
    match: /(nominal|fair|serious|critical)/i,
    explain: (m) => ({
      title: `The device is running ${THERMAL_WORD[m[1].toLowerCase()] ?? m[1].toLowerCase()}`,
      tone: THERMAL_TONE[m[1].toLowerCase()] ?? "info",
      routine: false,
    }),
  },
  {
    id: "memory-pressure",
    match:
      /memory pressure (?:warning|notification|level)[^a-z]*(?:of type:?\s*)?(normal|warning|critical|urgent)?/i,
    explain: (m, e) => {
      const level = m[1]?.toLowerCase();
      return {
        title: level
          ? `Memory is running ${level === "critical" || level === "urgent" ? "critically " : ""}low`
          : "Memory is running low",
        detail: `The system asked ${shortName(e.process)} to free memory`,
        tone: level === "critical" || level === "urgent" ? "bad" : "warn",
        routine: false,
      };
    },
  },
  {
    id: "memory-kill",
    match: /memorystatus: killing[_ ]?\w*\s+pid\s+(\d+)\s+\[([^\]]+)\]/i,
    explain: (m) => ({
      title: `The system closed ${shortName(m[2])} to free memory`,
      tone: "bad",
      routine: false,
    }),
  },
  {
    id: "rb-housekeeping",
    process: ["runningboardd"],
    match:
      /jetsam managed|will not be managed|_resolveProcess|Resolved pid|encoding handle|assertion|Acquiring|Invalidating/i,
    explain: () => ({
      title: "iPadOS managed apps running in the background",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "jetsam-policy",
    match: /^(?!.*(?:not RunningBoard jetsam managed|will not be managed)).*jetsam/i,
    explain: (_m, e) => {
      const app = appIn(e.message);
      return {
        title: app
          ? `${app} was marked for closing if memory runs short`
          : "An app was marked for closing if memory runs short",
        detail: cleanMessage(e.message),
        tone: "warn",
        routine: false,
      };
    },
  },
  {
    id: "crash-report",
    match:
      /Saved type '\d+' report .* at .*\/(JetsamEvent|[^/]+?)-\d{4}-\d{2}-\d{2}-\d{6}[^/]*\.ips/i,
    explain: (m) =>
      m[1] === "JetsamEvent"
        ? {
            title: "iPadOS closed something to free memory and saved a report",
            tone: "warn",
            routine: false,
          }
        : {
            title: `${shortName(m[1])} crashed; a crash report was saved`,
            tone: "bad",
            routine: false,
          },
  },
  {
    id: "crash",
    match: /Formulating .*report for corpse\[\d+\]\s+(\S+)/i,
    explain: (m) => ({ title: `${shortName(m[1])} crashed`, tone: "bad", routine: false }),
  },
  {
    id: "app-open",
    match: /\bLaunch(?:ing|ed)\b/,
    explain: (_m, e) => {
      const app = appIn(e.message);
      return app
        ? { title: `Opened ${app}`, tone: "info", routine: false }
        : {
            title: "An app was launched",
            detail: cleanMessage(e.message),
            tone: "info",
            routine: false,
          };
    },
  },
  {
    id: "app-exit",
    match: /\b(?:exited|terminat(?:ed|ing)|was killed)\b/i,
    explain: (_m, e) => {
      const app = appIn(e.message);
      return app
        ? { title: `Closed ${app}`, tone: "neutral", routine: false }
        : {
            title: "A process ended",
            detail: cleanMessage(e.message),
            tone: "neutral",
            routine: false,
          };
    },
  },
  {
    id: "iface",
    process: ["kernel"],
    match: /if_updown: (\S+) is (not )?attached/,
    explain: (m) => ({
      title: `Network interface ${m[1]} ${m[2] ? "went down" : "came up"}`,
      tone: "info",
      routine: false,
    }),
  },
  {
    id: "net-config",
    process: ["wifid", "configd"],
    match: /SCNetworkConfiguration|State:\/Network/,
    explain: () => ({
      title: "The network settings changed (for example Wi-Fi reconnected)",
      tone: "info",
      routine: false,
    }),
  },
  {
    id: "statasmon-hidden",
    process: ["statasmon"],
    match: /hid (\d+) lines/,
    explain: (m, e) => ({
      title: `StatasMon hid ${formatCount(Number(m[1]))} routine lines to keep this readable`,
      detail: cleanMessage(e.message),
      tone: "neutral",
      routine: false,
    }),
  },
  // --- iOS: routine chatter ---
  {
    id: "wifi-telemetry",
    process: ["wifid"],
    match: /LQM-WiFi|link quality|rssi=|snr=/i,
    explain: () => ({ title: "Wi-Fi measured its signal quality", tone: "neutral", routine: true }),
  },
  {
    id: "wifi-housekeeping",
    process: ["wifid"],
    match: /./,
    explain: () => ({
      title: "Wi-Fi did some work in the background",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "xpc",
    match:
      /xpc_connection|no-senders notification|invalidated|activating connection|mach=true|listener=false/i,
    explain: () => ({
      title: "Two parts of the system talked to each other",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "property-read",
    process: ["lockdownd", "mobile_diagnostics_relay"],
    match: /./,
    explain: () => ({
      title: "StatasMon asked the device for its status",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "keychain",
    process: ["trustd", "securityd", "ctkd"],
    match: /./,
    explain: () => ({
      title: "The system checked security certificates",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "storage-housekeeping",
    process: ["containermanagerd", "installd", "ind", "cache_delete", "mobile_installation_proxy"],
    match: /./,
    explain: () => ({ title: "The system tidied app storage", tone: "neutral", routine: true }),
  },
  {
    id: "location",
    process: ["locationd"],
    match: /./,
    explain: () => ({ title: "Location services did some work", tone: "neutral", routine: true }),
  },
  {
    id: "bluetooth",
    process: ["bluetoothd"],
    match: /./,
    explain: () => ({ title: "Bluetooth did some work", tone: "neutral", routine: true }),
  },
  {
    id: "push",
    process: ["apsd"],
    match: /./,
    explain: () => ({
      title: "The notification service did some work",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "background-net",
    process: ["nsurlsessiond", "networkd", "symptomsd", "nehelper", "mdnsresponder"],
    match: /./,
    explain: () => ({
      title: "Apps used the network in the background",
      tone: "neutral",
      routine: true,
    }),
  },
  {
    id: "springboard",
    process: ["springboard", "backboardd", "frontboardd"],
    match: /./,
    explain: () => ({ title: "The home screen updated itself", tone: "neutral", routine: true }),
  },
  {
    id: "kernel",
    process: ["kernel"],
    match: /./,
    explain: () => ({ title: "A low-level system message", tone: "neutral", routine: true }),
  },
  // --- Android ---
  {
    id: "android-open",
    match: /ActivityManager:?\s*Start proc \d+:([\w.:$]+)/,
    explain: (m) => ({
      title: `Opened ${shortName(m[1].split(":")[0])}`,
      tone: "info",
      routine: false,
    }),
  },
  {
    id: "android-displayed",
    match: /Displayed ([\w.]+)\//,
    explain: (m) => ({ title: `${shortName(m[1])} showed a screen`, tone: "info", routine: false }),
  },
  {
    id: "android-kill",
    match: /(?:am_kill|Killing \d+:([\w.:]+))/,
    explain: (m, e) => ({
      title: m[1] ? `Android closed ${shortName(m[1].split(":")[0])}` : "Android closed an app",
      detail: cleanMessage(e.message),
      tone: "warn",
      routine: false,
    }),
  },
  {
    id: "android-anr",
    match: /ANR in ([\w.]+)/,
    explain: (m) => ({
      title: `${shortName(m[1])} stopped responding`,
      tone: "bad",
      routine: false,
    }),
  },
  {
    id: "android-crash",
    match: /FATAL EXCEPTION/,
    explain: (_m, e) => ({
      title: `${shortName(e.process)} crashed`,
      detail: cleanMessage(e.message),
      tone: "bad",
      routine: false,
    }),
  },
  {
    id: "android-lowmem",
    match: /lowmemorykiller|Low on memory/i,
    explain: () => ({ title: "Android is low on memory", tone: "warn", routine: false }),
  },
  {
    id: "android-gc",
    match: /\bGC\b.*(?:freed|paused)|Background concurrent copying GC/i,
    explain: () => ({ title: "An app tidied up its memory", tone: "neutral", routine: true }),
  },
  {
    id: "android-system",
    process: ["system_server", "surfaceflinger", "windowmanager", "batterystats", "batteryservice"],
    match: /./,
    explain: () => ({
      title: "Android system services did some work",
      tone: "neutral",
      routine: true,
    }),
  },
];

/** iOS daemons are lower-case names ending in "d" (cfprefsd, wifid, trustd). */
export function isSystemService(process: string): boolean {
  return /^[a-z0-9_.-]+d$/.test(process);
}

/** Plain-language reading of one line. */
export function explain(e: LogEntry): Explanation {
  const process = e.process.toLowerCase();
  for (const r of RULES) {
    if (r.process && !r.process.includes(process)) continue;
    const m = e.message.match(r.match);
    if (!m) continue;
    return { rule: r.id, ...r.explain(m, e) };
  }
  const who = shortName(e.process);
  switch (e.level) {
    case "fault":
      return {
        rule: "fault",
        title: `${who} hit a serious problem`,
        detail: cleanMessage(e.message),
        tone: "bad",
        routine: false,
      };
    case "error":
      // System services (lower-case names ending in "d": cfprefsd, wifid…)
      // log errors constantly that mean nothing to a person; apps do not.
      return isSystemService(e.process)
        ? {
            rule: "service-error",
            title: `${who} (a system service) noted a minor error`,
            detail: cleanMessage(e.message),
            tone: "info",
            routine: false,
          }
        : {
            rule: "error",
            title: `${who} ran into an error`,
            detail: cleanMessage(e.message),
            tone: "warn",
            routine: false,
          };
    case "warning":
      return {
        rule: "warning",
        title: `${who} noticed something worth a look`,
        detail: cleanMessage(e.message),
        tone: "info",
        routine: false,
      };
    default:
      return { rule: "routine", title: `${who} did routine work`, tone: "neutral", routine: true };
  }
}

export interface Story {
  /** Ring key of the first entry (stable while it is in the ring). */
  key: number;
  rule: string;
  title: string;
  detail?: string;
  tone: StoryTone;
  routine: boolean;
  /** Processes involved, most lines first. */
  processes: string[];
  count: number;
  firstMs: number;
  lastMs: number;
  /** Up to `SAMPLE_LINES` raw lines for the expander. */
  samples: LogEntry[];
}

/** Lines that continue a story must fall within this many ms of the last one. */
export const STORY_WINDOW_MS = 5_000;
/** Raw lines kept per story for the expander. */
export const SAMPLE_LINES = 25;
export const ROUTINE_TITLE = "Normal background activity";

/**
 * Folds filtered entries (oldest → newest) into stories: consecutive lines
 * with the same rule and process within `STORY_WINDOW_MS` become one story
 * with a count; consecutive routine lines from any process become one
 * "Routine system activity" story until something notable interrupts.
 */
export function buildStories(entries: readonly LogEntry[], keys: readonly number[]): Story[] {
  const out: Story[] = [];
  const counts = new Map<Story, Map<string, number>>();
  for (let i = 0; i < entries.length; i += 1) {
    const e = entries[i];
    const x = explain(e);
    const last = out[out.length - 1];
    const joins =
      last !== undefined &&
      last.routine === x.routine &&
      (x.routine ? true : last.rule === x.rule && last.processes[0] === e.process) &&
      e.ts_ms - last.lastMs <= STORY_WINDOW_MS;
    if (joins && last) {
      last.count += 1;
      last.lastMs = Math.max(last.lastMs, e.ts_ms);
      if (last.samples.length < SAMPLE_LINES) last.samples.push(e);
      const c = counts.get(last);
      if (c) c.set(e.process, (c.get(e.process) ?? 0) + 1);
      continue;
    }
    const story: Story = {
      key: keys[i] ?? i,
      rule: x.routine ? "routine" : x.rule,
      title: x.routine ? ROUTINE_TITLE : x.title,
      detail: x.routine ? undefined : x.detail,
      tone: x.routine ? "neutral" : x.tone,
      routine: x.routine,
      processes: [e.process],
      count: 1,
      firstMs: e.ts_ms,
      lastMs: e.ts_ms,
      samples: [e],
    };
    counts.set(story, new Map([[e.process, 1]]));
    out.push(story);
  }
  for (const s of out) {
    const c = counts.get(s);
    if (c) s.processes = [...c.entries()].sort((a, b) => b[1] - a[1]).map(([p]) => p);
  }
  return out;
}
