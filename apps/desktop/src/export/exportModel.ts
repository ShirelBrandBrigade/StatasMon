import type {
  ExportFormat,
  ExportRequest,
  ExportResult,
  ExportScope,
  LogFilter,
  ProcessRow,
} from "@statasmon/protocol";
import type { TaskProgress } from "../store/app";
import type { LogFilterState } from "../store/logFilter";

export const FORMAT_OPTIONS = [
  { value: "log", label: "Log" },
  { value: "jsonl", label: "JSONL" },
  { value: "csv", label: "CSV" },
  { value: "bundle", label: "Bundle" },
] as const satisfies ReadonlyArray<{ value: ExportFormat; label: string }>;

export type ScopeKind = ExportScope["scope"];

export const SCOPE_ORDER: readonly ScopeKind[] = ["session", "range", "filter", "app", "selection"];

const EXT: Record<ExportFormat, string> = { log: "log", jsonl: "jsonl", csv: "csv", bundle: "zip" };

export function extensionFor(format: ExportFormat): string {
  return EXT[format];
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/** `~/Downloads/StatasMon/<device>-<date>.<ext>`; the device name is slugged for the file system. */
export function defaultExportPath(deviceName: string, format: ExportFormat, at: Date): string {
  const slug =
    deviceName
      .normalize("NFKD")
      .replace(/[^\w-]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .toLowerCase() || "device";
  const date = `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}-${pad2(at.getHours())}${pad2(at.getMinutes())}`;
  return `~/Downloads/StatasMon/${slug}-${date}.${extensionFor(format)}`;
}

/** Swaps the extension of a path the user has not edited when the format changes. */
export function withExtension(path: string, format: ExportFormat): string {
  return path.replace(/\.(log|jsonl|csv|zip)$/i, `.${extensionFor(format)}`);
}

/** The UI's log filter as the protocol's `LogFilter` (a `/regex/` query becomes `regex: true`). */
export function toProtocolFilter(
  deviceId: string,
  sessionId: string | null,
  f: LogFilterState,
): LogFilter {
  const q = f.query.trim();
  const closed = /^\/(.*)\/[a-z]*$/s.exec(q);
  const regex = q.startsWith("/");
  const query = regex ? (closed ? closed[1] : q.slice(1)) : q;
  return {
    device_id: deviceId,
    session_id: sessionId,
    processes: [...f.processes],
    levels: [...f.levels],
    query: query.length > 0 ? query : null,
    regex,
    from_ms: null,
    to_ms: null,
    buffers: [],
  };
}

/** One row of the app picker: bundle id / package when known, else the process name. */
export interface AppChoice {
  id: string;
  label: string;
}

export function appChoices(rows: readonly ProcessRow[]): AppChoice[] {
  const seen = new Map<string, AppChoice>();
  for (const r of rows) {
    const id = r.app_id ?? r.name;
    if (!id || seen.has(id)) continue;
    seen.set(id, {
      id,
      label: r.app_id && r.app_id !== r.name ? `${r.name} · ${r.app_id}` : r.name,
    });
  }
  return [...seen.values()].sort((a, b) => a.label.localeCompare(b.label));
}

export interface ScopeInputs {
  kind: ScopeKind;
  /** Current range window; undefined = whole session (then "range" means "session"). */
  rangeMs: number | undefined;
  /** Newest sample time (or now) for the range's upper bound. */
  nowMs: number;
  filter: LogFilter;
  appId: string;
  seqs: number[];
}

/** The protocol scope for the chosen kind, or an error the user can fix. */
export function buildScope(i: ScopeInputs): { scope: ExportScope } | { error: string } {
  switch (i.kind) {
    case "session":
      return { scope: { scope: "session" } };
    case "range":
      if (i.rangeMs === undefined) return { scope: { scope: "session" } };
      return { scope: { scope: "range", from_ms: i.nowMs - i.rangeMs, to_ms: i.nowMs } };
    case "filter":
      return { scope: { scope: "filter", filter: i.filter } };
    case "app":
      if (!i.appId) return { error: "Pick an app to export." };
      return { scope: { scope: "app", app_id: i.appId } };
    case "selection":
      if (i.seqs.length === 0) return { error: "Select log lines in the log card first." };
      return { scope: { scope: "selection", seqs: [...i.seqs] } };
  }
}

export function validatePath(path: string): string | undefined {
  return path.trim().length === 0 ? "Choose where to write the file." : undefined;
}

export function buildRequest(args: {
  deviceId: string;
  sessionId: string | null;
  format: ExportFormat;
  scope: ExportScope;
  path: string;
}): ExportRequest {
  return {
    device_id: args.deviceId,
    session_id: args.sessionId,
    format: args.format,
    scope: args.scope,
    path: args.path.trim(),
  };
}

/**
 * The request carries no task id, so the export's progress is the newest
 * `progress` message that arrived after the export started and whose task id
 * or message starts with "export".
 */
export function pickExportProgress(
  progress: Record<string, TaskProgress>,
  startedAt: number,
): TaskProgress | undefined {
  let best: TaskProgress | undefined;
  for (const [taskId, p] of Object.entries(progress)) {
    if (p.at < startedAt) continue;
    const tagged =
      taskId.toLowerCase().startsWith("export") || p.message.toLowerCase().startsWith("export");
    if (!tagged) continue;
    if (!best || p.at > best.at || (p.at === best.at && p.pct > best.pct)) best = p;
  }
  return best;
}

/** "12 345" (thin-space grouping, like the rest of the UI). */
export function groupDigits(n: number): string {
  return Math.round(n).toLocaleString("en-US").replace(/,/g, " ");
}

export function resultLine(r: ExportResult): string {
  return `Wrote ${groupDigits(r.bytes)} B · ${groupDigits(r.entries)} entries · ${r.path}`;
}

export interface ExportFailure {
  code: string;
  message: string;
  hint?: string;
}

/** A rejected `export_data` (`{ code, message, hint }`, an Error, or a string) as one line. */
export function describeExportError(e: unknown): ExportFailure {
  if (e && typeof e === "object") {
    const o = e as Record<string, unknown>;
    const message =
      typeof o.message === "string" && o.message
        ? o.message
        : e instanceof Error
          ? e.message
          : "Export failed";
    return {
      code: typeof o.code === "string" && o.code ? o.code : "internal",
      message,
      hint: typeof o.hint === "string" && o.hint ? o.hint : undefined,
    };
  }
  return { code: "internal", message: typeof e === "string" && e ? e : "Export failed" };
}
