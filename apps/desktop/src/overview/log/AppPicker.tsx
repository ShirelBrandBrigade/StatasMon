import type { ProcessRow } from "@statasmon/protocol";
import { AppTile, cx } from "@statasmon/ui";
import { type MouseEvent, useEffect, useId, useMemo, useRef, useState } from "react";
import { useProcesses } from "../../hooks/useDevice";
import { useLogFilter, useLogFilterActions } from "../../store/logFilter";
import { formatCount, isAppUnderTest } from "./format";
import type { ProcessCount } from "./logScan";

/** One row of the picker. */
export interface PickerRow {
  process: string;
  /** Bundle id / package when the process is an app. */
  appId: string | null;
  /** Log lines seen from it in the mirror ring. */
  count: number;
  running: boolean;
  me: boolean;
}

export interface PickerRows {
  /** Apps: running processes with an app id, plus the app under test. */
  apps: PickerRow[];
  /** Everything else that has logged, most lines first. */
  system: PickerRow[];
}

/** System processes shown at most (the search reaches the rest). */
const SYSTEM_LIMIT = 40;

/**
 * Apps first (the app under test, then by line count), system processes
 * after, both narrowed by `query` (matches the name or the app id).
 */
export function pickerRows(
  counts: readonly ProcessCount[],
  running: readonly ProcessRow[],
  query: string,
  isMe: (process: string) => boolean = (p) => isAppUnderTest({ process: p, subsystem: null }),
): PickerRows {
  const q = query.trim().toLowerCase();
  const countOf = new Map(counts.map((c) => [c.process, c.count]));
  const apps = new Map<string, PickerRow>();
  for (const r of running) {
    if (!r.app_id || apps.has(r.name)) continue;
    apps.set(r.name, {
      process: r.name,
      appId: r.app_id,
      count: countOf.get(r.name) ?? 0,
      running: true,
      me: isMe(r.name),
    });
  }
  for (const c of counts) {
    if (!apps.has(c.process) && isMe(c.process)) {
      apps.set(c.process, {
        process: c.process,
        appId: null,
        count: c.count,
        running: running.some((r) => r.name === c.process),
        me: true,
      });
    }
  }
  const matches = (r: PickerRow) =>
    q === "" ||
    r.process.toLowerCase().includes(q) ||
    (r.appId?.toLowerCase().includes(q) ?? false);
  const appRows = [...apps.values()]
    .filter(matches)
    .sort(
      (a, b) =>
        Number(b.me) - Number(a.me) || b.count - a.count || a.process.localeCompare(b.process),
    );
  const system = counts
    .filter((c) => !apps.has(c.process))
    .map<PickerRow>((c) => ({
      process: c.process,
      appId: null,
      count: c.count,
      running: running.some((r) => r.name === c.process),
      me: false,
    }))
    .filter(matches)
    .slice(0, SYSTEM_LIMIT);
  return { apps: appRows, system };
}

export interface AppPickerProps {
  deviceId: string;
  /** Processes seen in the log with their line counts (from `useFilteredLog`). */
  processes: readonly ProcessCount[];
  /** Button text; the accessible name is always "Filter by app". */
  label?: string;
  className?: string;
}

/**
 * "+ app": a popover listing the running apps and the system processes
 * that have logged. Click selects that one app alone (the usual case);
 * Shift-click adds it to the selection. Escape or a click outside closes.
 */
export function AppPicker({ deviceId, processes, label = "+ app", className }: AppPickerProps) {
  const filter = useLogFilter(deviceId);
  const actions = useLogFilterActions(deviceId);
  const running = useProcesses(deviceId);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  // Closing forgets the search, so the next open lists everything again.
  const close = () => {
    setOpen(false);
    setQuery("");
  };
  const rootRef = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const rows = useMemo(() => pickerRows(processes, running, query), [processes, running, query]);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: globalThis.MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
        setQuery("");
      }
    };
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        setQuery("");
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const choose = (process: string, e: MouseEvent) => {
    if (e.shiftKey) {
      actions.toggleProcess(process);
      return;
    }
    actions.setProcesses([process]);
    close();
  };

  const item = (r: PickerRow) => {
    const selected = filter.processes.includes(r.process);
    return (
      <button
        key={r.process}
        type="button"
        role="option"
        aria-selected={selected}
        className="log-picker-item"
        data-process={r.process}
        data-running={r.running}
        onClick={(e) => choose(r.process, e)}
      >
        <AppTile me={r.me} />
        <span className="log-picker-name">
          {r.process}
          {r.appId && r.appId !== r.process ? (
            <span className="log-picker-sub mono">{r.appId}</span>
          ) : null}
        </span>
        <em className="mono">{r.count > 0 ? formatCount(r.count) : "—"}</em>
      </button>
    );
  };

  const empty = rows.apps.length === 0 && rows.system.length === 0;
  return (
    <div className={cx("log-picker", className)} ref={rootRef}>
      <button
        type="button"
        className="log-pill is-ghost"
        aria-label="Filter by app"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => (open ? close() : setOpen(true))}
      >
        {label}
      </button>
      {open ? (
        <div className="log-picker-panel" role="dialog" aria-label="Filter by app" id={panelId}>
          <input
            className="log-picker-search mono"
            type="search"
            aria-label="Find an app"
            placeholder="Find an app or process"
            value={query}
            // biome-ignore lint/a11y/noAutofocus: opened by the user's click on the picker
            autoFocus
            onChange={(e) => setQuery(e.target.value)}
          />
          <div className="log-picker-list" role="listbox" aria-label="Apps and processes">
            {rows.apps.length > 0 ? <div className="log-picker-group">Running apps</div> : null}
            {rows.apps.map(item)}
            {rows.system.length > 0 ? (
              <div className="log-picker-group">System processes</div>
            ) : null}
            {rows.system.map(item)}
            {empty ? (
              <p className="log-picker-empty">
                {processes.length === 0 && running.length === 0
                  ? "No processes yet"
                  : "No process matches"}
              </p>
            ) : null}
          </div>
          <div className="log-picker-foot">
            <span>
              {filter.processes.length === 0
                ? "All apps"
                : `${formatCount(filter.processes.length)} selected`}
            </span>
            {filter.processes.length > 0 ? (
              <button
                type="button"
                className="log-pill is-ghost"
                onClick={() => actions.setProcesses([])}
              >
                Clear
              </button>
            ) : null}
            <span className="log-picker-hint">Click picks one app · ⇧ click adds</span>
          </div>
        </div>
      ) : null}
    </div>
  );
}
