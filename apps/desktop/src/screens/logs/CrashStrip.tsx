import type { StoredEvent } from "@statasmon/protocol";
import { Button, Pill } from "@statasmon/ui";
import { useMemo, useState } from "react";
import { commands } from "../../bridge/commands";
import { useDevice } from "../../hooks/useDevice";
import { formatClock, plural } from "../../overview/log";
import { eventKey, eventMeta, isCrashEvent } from "./eventMeta";
import { reportCount } from "./logsModel";

/** Half-width of the time window "Show around" opens around an event. */
export const AROUND_MS = 5_000;

type Fetch =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; count: number | undefined }
  | { kind: "failed"; message: string };

function describe(e: unknown): string {
  if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return typeof e === "string" ? e : "The device did not return crash reports";
}

export interface CrashStripProps {
  deviceId: string;
  /** Narrows the list to ±5 s around the event. */
  onShowAround(tsMs: number): void;
}

/**
 * Crashes & ANRs (FEATURES.md §9): the device's crash / ANR events, newest
 * first, each with its attached report text (`detail`), and the
 * `fetch_crash_reports` action with its result inline.
 */
export function CrashStrip({ deviceId, onShowAround }: CrashStripProps) {
  const events = useDevice(deviceId)?.events;
  const crashes = useMemo<StoredEvent[]>(
    () => (events ? events.filter(isCrashEvent).slice().reverse() : []),
    [events],
  );
  const [fetch, setFetch] = useState<Fetch>({ kind: "idle" });

  const run = async () => {
    setFetch({ kind: "running" });
    try {
      const result = await commands.deviceAction(deviceId, { action: "fetch_crash_reports" });
      setFetch({ kind: "done", count: reportCount(result) });
    } catch (e) {
      setFetch({ kind: "failed", message: describe(e) });
    }
  };

  return (
    <div className="crash" data-testid="crash-strip" data-count={crashes.length}>
      <div className="crash-actions">
        <Button
          size="sm"
          icon="warning"
          disabled={fetch.kind === "running"}
          onClick={() => void run()}
        >
          {fetch.kind === "running" ? "Fetching…" : "Fetch crash reports"}
        </Button>
        <span className="crash-result mono" role="status" data-testid="crash-fetch-result">
          {fetch.kind === "done"
            ? fetch.count === undefined
              ? "no reports returned"
              : plural(fetch.count, "report")
            : fetch.kind === "failed"
              ? fetch.message
              : ""}
        </span>
      </div>
      {crashes.length === 0 ? (
        <span className="crash-empty">No crashes or ANRs this session</span>
      ) : (
        <ul className="crash-list" aria-label="Crashes and ANRs">
          {crashes.map((e) => {
            const meta = eventMeta(e.kind);
            return (
              <li key={eventKey(e)}>
                <div className="crash-head">
                  <Pill tone={meta.tone} size="sm">
                    {meta.label}
                  </Pill>
                  <b>{e.process ?? "unknown process"}</b>
                  {e.pid !== null ? <span className="mono">pid {e.pid}</span> : null}
                  <time className="mono">{formatClock(e.ts_ms)}</time>
                </div>
                <pre className="crash-detail mono">{e.detail}</pre>
                <Button size="sm" onClick={() => onShowAround(e.ts_ms)}>
                  Show log around it
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
