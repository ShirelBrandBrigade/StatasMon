import "../../overview/log/LogCard.css";
import "./logs.css";
import type { LogEntry, LogLevel } from "@statasmon/protocol";
import { AppTile, Button, Card, Dash, levelMeta, toneColor } from "@statasmon/ui";
import {
  type CSSProperties,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { toProtocolFilter } from "../../export/exportModel";
import { openExportDialog } from "../../export/exportStore";
import { useCapability, useProcesses, useTelemetryValue } from "../../hooks/useDevice";
import { formatCount, isAppUnderTest, plural } from "../../overview/log";
import { AppPicker } from "../../overview/log/AppPicker";
import { LogRow } from "../../overview/log/LogRow";
import { useFilteredLog } from "../../overview/log/useFilteredLog";
import { TopBar } from "../../shell/TopBar";
import { useAppStore } from "../../store/app";
import { ALL_LEVELS, useLogFilter, useLogFilterActions } from "../../store/logFilter";
import { telemetry } from "../../store/telemetry";
import { AROUND_MS, CrashStrip } from "./CrashStrip";
import {
  activeTabId,
  appTabs,
  BufferTracker,
  type Narrowing,
  narrowingActive,
  narrowLog,
  parseLocalDateTime,
  STORED_PAGE,
  storedKey,
  storedRemaining,
  toLocalDateTime,
} from "./logsModel";
import { useStoredPages } from "./useStoredPages";

/** Row height estimate for Virtuoso before it measures (jsdom cannot). */
const ROW_HEIGHT = 26;

const LEVEL_LETTER: Record<LogLevel, string> = {
  debug: "D",
  info: "I",
  notice: "N",
  warning: "W",
  error: "E",
  fault: "F",
};

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    tag === "BUTTON" ||
    target.isContentEditable
  );
}

const meProcess = (process: string) => isAppUnderTest({ process, subsystem: null });

interface RingFacts {
  buffers: readonly string[];
  /** Timestamp of the oldest line in the mirror ring (any level, any app). */
  oldestMs: number | undefined;
}

/** Buffers the ring has carried and its oldest line, re-read once per log tick. */
function useRingFacts(deviceId: string): RingFacts {
  const tracker = useRef<BufferTracker>(undefined);
  if (!tracker.current) tracker.current = new BufferTracker();
  const last = useRef<RingFacts>({ buffers: [], oldestMs: undefined });
  return useTelemetryValue(deviceId, ["log"], () => {
    const ring = telemetry.logsFor(deviceId);
    const buffers = tracker.current?.update(ring) ?? [];
    const oldestMs = ring.all()[0]?.ts_ms;
    const prev = last.current;
    if (prev.buffers === buffers && prev.oldestMs === oldestMs) return prev;
    last.current = { buffers, oldestMs };
    return last.current;
  });
}

export interface LogsBodyProps {
  deviceId: string;
  /** Test hook: rows Virtuoso renders before it can measure. */
  initialItemCount?: number;
}

/** The Logs screen for one device; keyed by device so local state (range, buffers, selection) resets. */
export function LogsBody({ deviceId, initialItemCount }: LogsBodyProps) {
  const logsCap = useCapability(deviceId, "logs");
  const filter = useLogFilter(deviceId);
  const actions = useLogFilterActions(deviceId);
  const log = useFilteredLog(deviceId);
  const facts = useRingFacts(deviceId);
  const sessionId = useAppStore((s) => s.devices[deviceId]?.session?.id ?? undefined);
  const { followTail } = filter;

  const [fromText, setFromText] = useState("");
  const [toText, setToText] = useState("");
  const [buffers, setBuffers] = useState<readonly string[]>([]);
  const [selected, setSelected] = useState<ReadonlySet<number>>(() => new Set());
  const [atBottom, setAtBottom] = useState(true);
  const listRef = useRef<VirtuosoHandle>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const fromMs = parseLocalDateTime(fromText);
  const toMs = parseLocalDateTime(toText);
  const bufferKey = buffers.join(",");
  // biome-ignore lint/correctness/useExhaustiveDependencies: `bufferKey` is the content key for `buffers`
  const narrowing = useMemo<Narrowing>(
    () => ({ fromMs, toMs, buffers }),
    [fromMs, toMs, bufferKey],
  );
  const narrowed = useMemo(() => narrowLog(log, narrowing), [log, narrowing]);

  // Stored-session paging: the same query, bounded to lines older than the ring.
  const storedEnabled = sessionId !== undefined && filter.levels.length > 0;
  const base = useMemo(
    () => toProtocolFilter(deviceId, sessionId ?? null, filter),
    [deviceId, sessionId, filter],
  );
  const stored = useStoredPages(base, narrowing, facts.oldestMs, storedEnabled);

  const merged = useMemo(() => {
    const older = stored.state.entries;
    if (older.length === 0) return { entries: narrowed.entries, keys: narrowed.keys };
    const offset = stored.state.offset;
    const keys = older.map((_, i) => storedKey(offset, i));
    return {
      entries: older.concat(narrowed.entries),
      keys: keys.concat(narrowed.keys),
    };
  }, [stored.state.entries, stored.state.offset, narrowed]);
  const storedCount = stored.state.entries.length;

  const running = useProcesses(deviceId);
  const appNames = useMemo(
    () => running.filter((r) => r.app_id !== null).map((r) => r.name),
    [running],
  );
  const tabs = useMemo(
    () => appTabs(log.processes, meProcess, filter.processes, appNames),
    [log.processes, filter.processes, appNames],
  );
  const activeTab = activeTabId(tabs, filter.processes);

  const jumpToLatest = useCallback(() => {
    listRef.current?.scrollToIndex({ index: "LAST", align: "end" });
    actions.setFollowTail(true);
  }, [actions.setFollowTail]);

  const toggleFollow = useCallback(() => {
    if (followTail) actions.setFollowTail(false);
    else jumpToLatest();
  }, [followTail, actions.setFollowTail, jumpToLatest]);

  const onListKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === " " && !isEditable(e.target)) {
      e.preventDefault();
      toggleFollow();
    }
  };

  // `/` focuses the search from anywhere on the screen (not while typing).
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === "/" && !isEditable(e.target)) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const toggleSelected = useCallback((seq: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(seq)) next.delete(seq);
      else next.add(seq);
      return next;
    });
  }, []);

  const toggleBuffer = (b: string) =>
    setBuffers((cur) => (cur.includes(b) ? cur.filter((x) => x !== b) : [...cur, b]));

  const showAround = useCallback(
    (tsMs: number) => {
      setFromText(toLocalDateTime(tsMs - AROUND_MS));
      setToText(toLocalDateTime(tsMs + AROUND_MS));
      actions.setFollowTail(false);
    },
    [actions.setFollowTail],
  );

  const clearRange = () => {
    setFromText("");
    setToText("");
  };

  if (logsCap.available === false) {
    const reason = logsCap.reason ?? "This device does not provide logs";
    return (
      <div className="logs" data-testid="logs-screen">
        <Card className="logs-card">
          <div className="log-empty">
            <Dash reason={reason} />
            <span>Logs unavailable: {reason}</span>
          </div>
        </Card>
      </div>
    );
  }

  const { entries, keys } = merged;
  const following = followTail && atBottom;
  const levelToggles = log.warningsSeen ? ALL_LEVELS : ALL_LEVELS.filter((l) => l !== "warning");
  const narrowed_ = narrowingActive(narrowing);

  const footer = (
    <>
      <span>
        <span className="mono">{formatCount(log.total)}</span> entries
        {narrowed.shown !== log.total || storedCount > 0 ? (
          <>
            {" · "}
            <span className="mono">{formatCount(narrowed.shown + storedCount)}</span> shown
          </>
        ) : null}
        {storedCount > 0 ? (
          <>
            {" · "}
            <span className="mono">{formatCount(storedCount)}</span> from the store
          </>
        ) : null}
        {" · "}
        <span data-follow={following ? "following" : "paused"}>
          {following ? "following" : "paused"}
        </span>
      </span>
      <span>·</span>
      <span>
        {plural(narrowed.errors, "error")} · {plural(narrowed.faults, "fault")}
      </span>
      {selected.size > 0 ? (
        <>
          <span>·</span>
          <span>{plural(selected.size, "selected")}</span>
        </>
      ) : null}
      <Button
        size="sm"
        icon="download"
        className="logs-foot-export"
        onClick={() =>
          openExportDialog(
            selected.size > 0
              ? { deviceId, scope: "selection", seqs: [...selected], format: "log" }
              : { deviceId, scope: "filter", format: "log" },
          )
        }
      >
        {selected.size > 0 ? "Export selection" : "Export filtered log"}
      </Button>
    </>
  );

  return (
    <div className="logs" data-testid="logs-screen">
      <Card className="logs-card" footer={footer} data-device={deviceId}>
        <div className="logs-toolbar" role="toolbar" aria-label="Log filters">
          <input
            ref={searchRef}
            className="log-search mono"
            type="search"
            aria-label="Search log"
            aria-invalid={log.invalid || undefined}
            placeholder="Search, or /regex/i  ( / to focus )"
            value={filter.query}
            onChange={(e) => actions.setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                actions.setQuery("");
              }
            }}
          />
          {log.invalid ? (
            <span className="log-note" role="status">
              invalid regex
            </span>
          ) : null}
          <fieldset className="log-levels">
            <legend className="log-legend">Levels</legend>
            {levelToggles.map((level) => {
              const { tone, label } = levelMeta(level);
              const on = filter.levels.includes(level);
              return (
                <button
                  key={level}
                  type="button"
                  className="log-lvl"
                  data-level={level}
                  aria-pressed={on}
                  aria-label={label}
                  title={`${label}: ${on ? "shown" : "hidden"}`}
                  style={{ "--lvl-tone": toneColor(tone) } as CSSProperties}
                  onClick={() => actions.toggleLevel(level)}
                >
                  {LEVEL_LETTER[level]}
                </button>
              );
            })}
          </fieldset>
          {facts.buffers.length > 0 ? (
            <fieldset className="logs-group" data-testid="buffers">
              <legend>Buffer</legend>
              {facts.buffers.map((b) => (
                <button
                  key={b}
                  type="button"
                  className="logs-chip"
                  aria-pressed={buffers.includes(b)}
                  onClick={() => toggleBuffer(b)}
                >
                  {b}
                </button>
              ))}
            </fieldset>
          ) : null}
          <fieldset className="logs-group">
            <legend>From</legend>
            <input
              className="logs-time"
              type="datetime-local"
              step={1}
              aria-label="From"
              value={fromText}
              onChange={(e) => setFromText(e.target.value)}
            />
            <span className="logs-group-label">To</span>
            <input
              className="logs-time"
              type="datetime-local"
              step={1}
              aria-label="To"
              value={toText}
              onChange={(e) => setToText(e.target.value)}
            />
            {fromText !== "" || toText !== "" ? (
              <Button size="sm" icon="close" onClick={clearRange}>
                Live
              </Button>
            ) : null}
          </fieldset>
        </div>

        <div className="logs-tabs" role="tablist" aria-label="Apps">
          {tabs.map((t) => {
            const on = t.id === activeTab;
            const count =
              t.processes.length === 1
                ? log.processes.find((p) => p.process === t.processes[0])?.count
                : undefined;
            return (
              <button
                key={t.id}
                type="button"
                role="tab"
                className="logs-tab"
                aria-selected={on}
                title="Click selects this app alone; Shift-click adds it to the selection"
                onClick={(e) => {
                  if (e.shiftKey && t.processes.length === 1) actions.toggleProcess(t.processes[0]);
                  else actions.setProcesses(t.processes);
                }}
              >
                {t.processes.length === 1 ? <AppTile me={t.me} /> : null}
                {t.label}
                {count !== undefined ? <em>{formatCount(count)}</em> : null}
              </button>
            );
          })}
          <AppPicker deviceId={deviceId} processes={log.processes} className="logs-picker" />
          <Button
            size="sm"
            icon={followTail ? "pause" : "chevron-down"}
            className="logs-follow"
            aria-pressed={followTail}
            title="Space toggles follow-tail when the list has focus"
            onClick={toggleFollow}
          >
            {followTail ? "Following" : "Follow tail"}
          </Button>
          {activeTab === undefined ? (
            <span className="logs-selected" data-testid="custom-selection">
              {plural(filter.processes.length, "app")} selected
              <button
                type="button"
                className="log-pill is-ghost"
                onClick={() => actions.setProcesses([])}
              >
                × clear
              </button>
            </span>
          ) : null}
        </div>

        <div
          className="log-list logs-list"
          role="listbox"
          aria-label="Log entries. Space pauses or resumes follow-tail; Enter selects a line."
          aria-multiselectable
          tabIndex={0}
          data-count={entries.length}
          data-follow={following ? "following" : "paused"}
          data-narrowed={narrowed_ ? "true" : undefined}
          onKeyDown={onListKeyDown}
        >
          {entries.length === 0 ? (
            <div className="log-empty">
              {log.total === 0 ? "No log lines yet" : "No entries match the filter"}
            </div>
          ) : (
            <Virtuoso<LogEntry>
              ref={listRef}
              style={{ height: "100%" }}
              data={entries}
              computeItemKey={(i) => keys[i] ?? `gap-${i}`}
              followOutput={followTail ? "auto" : false}
              atBottomStateChange={setAtBottom}
              increaseViewportBy={200}
              defaultItemHeight={ROW_HEIGHT}
              initialItemCount={initialItemCount}
              itemContent={(i, e) =>
                e ? (
                  <LogRow
                    entry={e}
                    alt={i % 2 === 1}
                    selected={selected.has(e.seq)}
                    onToggle={toggleSelected}
                  />
                ) : null
              }
            />
          )}
          {!atBottom && entries.length > 0 ? (
            <Button size="sm" className="log-jump" icon="chevron-down" onClick={jumpToLatest}>
              Jump to latest
            </Button>
          ) : null}
        </div>
      </Card>

      <aside className="logs-side">
        <Card title="Crashes & ANRs" subtitle="events with the report attached">
          <CrashStrip deviceId={deviceId} onShowAround={showAround} />
        </Card>
        <Card title="Stored session" subtitle="older lines from the recording">
          <div className="stored" data-testid="stored-session">
            {!sessionId ? (
              <span className="stored-note">
                Start a recording to page older lines from the store.
              </span>
            ) : (
              <>
                <span className="stored-id mono" title={sessionId}>
                  session {sessionId}
                </span>
                <Button
                  size="sm"
                  disabled={!storedEnabled || stored.state.loading || stored.exhausted}
                  onClick={stored.loadOlder}
                >
                  {stored.state.loading ? "Loading…" : `Load ${STORED_PAGE} older`}
                </Button>
                <span className="stored-note mono" role="status" data-testid="stored-status">
                  {stored.state.total === undefined
                    ? "Lines the mirror dropped stay in the session store."
                    : stored.state.total === 0
                      ? "No older lines stored for this filter."
                      : `${formatCount(storedCount)} loaded · ${formatCount(storedRemaining(stored.state))} older in the store`}
                </span>
                {stored.state.error ? (
                  <span className="stored-error" role="alert">
                    {stored.state.error}
                  </span>
                ) : null}
              </>
            )}
          </div>
        </Card>
      </aside>
    </div>
  );
}

/** Logs (docs/PLAN.md 6.4): the active device's full-height log with every FEATURES.md §9 filter. */
export function LogsScreen({ deviceId: given }: { deviceId?: string }) {
  const active = useAppStore((s) => s.activeDeviceId);
  const deviceId = given ?? active;
  if (!deviceId) return null;
  return (
    <>
      <TopBar deviceId={deviceId} />
      <LogsBody key={deviceId} deviceId={deviceId} />
    </>
  );
}
