import type { LogEntry, LogLevel } from "@statasmon/protocol";
import { Button, Card, cx, Dash, levelMeta, toneColor } from "@statasmon/ui";
import {
  type CSSProperties,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useCapability } from "../../hooks/useDevice";
import { ALL_LEVELS, useLogFilter, useLogFilterActions } from "../../store/logFilter";
import { formatCount, plural } from "./format";
import { LogRow } from "./LogRow";
import { useFilteredLog } from "./useFilteredLog";
import "./LogCard.css";

/** How many processes get their own pill (most lines first). */
const TOP_PROCESSES = 4;
/** Row height estimate for Virtuoso before it measures (5px padding + 16px line). */
const ROW_HEIGHT = 26;
/** How long the "Export arrives in Phase 5" note stays visible. */
export const EXPORT_NOTE_MS = 2000;

const LEVEL_LETTER: Record<LogLevel, string> = {
  debug: "D",
  info: "I",
  notice: "N",
  warning: "W",
  error: "E",
  fault: "F",
};

export interface LogCardProps {
  deviceId: string;
  className?: string;
  style?: CSSProperties;
  /** Test hook: number of rows Virtuoso renders before it can measure (jsdom). */
  initialItemCount?: number;
}

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "BUTTON" || target.isContentEditable;
}

/**
 * Overview log card: filter pills (apps, levels, search), a virtualized
 * follow-tail list with fault highlighting, and a footer with counts and the
 * export button (docs/PLAN.md 1.5, DESIGN-SYSTEM.md "LogCard").
 */
export function LogCard({ deviceId, className, style, initialItemCount }: LogCardProps) {
  const logsCap = useCapability(deviceId, "logs");
  const filter = useLogFilter(deviceId);
  const actions = useLogFilterActions(deviceId);
  const log = useFilteredLog(deviceId);
  const { entries, keys, total, shown, errors, faults, invalid, processes, warningsSeen } = log;
  const { followTail } = filter;

  const listRef = useRef<VirtuosoHandle>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [searchOpen, setSearchOpen] = useState(false);
  const [selected, setSelected] = useState<ReadonlySet<number>>(() => new Set());
  const [exportNote, setExportNote] = useState(false);

  useEffect(() => {
    if (!exportNote) return undefined;
    const t = setTimeout(() => setExportNote(false), EXPORT_NOTE_MS);
    return () => clearTimeout(t);
  }, [exportNote]);

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

  const toggleSelected = useCallback((seq: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(seq)) next.delete(seq);
      else next.add(seq);
      return next;
    });
  }, []);

  if (logsCap.available === false) {
    return (
      <Card className={cx("log-card", className)} style={style} data-device={deviceId}>
        <div className="log-empty">
          <Dash reason={logsCap.reason ?? "This device does not provide logs"} />
          <span>Logs unavailable: {logsCap.reason ?? "not provided by this device"}</span>
        </div>
      </Card>
    );
  }

  // App pills: the top processes by line count plus any selected process
  // that fell out of the top list (so a filter is never invisible).
  const pillProcesses = processes.slice(0, TOP_PROCESSES).map((p) => p.process);
  for (const p of filter.processes) if (!pillProcesses.includes(p)) pillProcesses.push(p);
  const allApps = filter.processes.length === 0;
  const levelToggles = warningsSeen ? ALL_LEVELS : ALL_LEVELS.filter((l) => l !== "warning");
  const searchVisible = searchOpen || filter.query !== "";
  const following = followTail && atBottom;

  const footer = (
    <>
      <span>
        <span className="mono">{formatCount(total)}</span> entries
        {shown !== total ? (
          <>
            {" · "}
            <span className="mono">{formatCount(shown)}</span> shown
          </>
        ) : null}
        {" · "}
        <span data-follow={following ? "following" : "paused"}>
          {following ? "following" : "paused"}
        </span>
      </span>
      <span>·</span>
      <span>
        {plural(errors, "error")} · {plural(faults, "fault")}
      </span>
      {selected.size > 0 ? (
        <>
          <span>·</span>
          <span>{plural(selected.size, "selected")}</span>
        </>
      ) : null}
      {exportNote ? (
        <span className="log-note" role="status">
          Export arrives in Phase 5
        </span>
      ) : null}
      <Button
        size="sm"
        icon="download"
        className="log-foot-export"
        onClick={() => setExportNote(true)}
      >
        Export selection (.log, .json)
      </Button>
    </>
  );

  return (
    <Card
      className={cx("log-card", className)}
      style={style}
      footer={footer}
      data-device={deviceId}
    >
      <div className="log-filters" role="toolbar" aria-label="Log filters">
        <button
          type="button"
          className="log-pill"
          aria-pressed={allApps}
          onClick={() => actions.setProcesses([])}
        >
          All apps
        </button>
        {pillProcesses.map((p) => (
          <button
            key={p}
            type="button"
            className="log-pill"
            aria-pressed={filter.processes.includes(p)}
            onClick={() => actions.toggleProcess(p)}
          >
            {p}
          </button>
        ))}
        <button
          type="button"
          className="log-pill is-ghost"
          aria-pressed={searchVisible}
          aria-label={searchVisible ? "Close search" : "Add a text filter"}
          onClick={() => {
            if (searchVisible) actions.setQuery("");
            setSearchOpen(!searchVisible);
          }}
        >
          {searchVisible ? "× filter" : "+ filter"}
        </button>
        {searchVisible ? (
          <input
            className="log-search mono"
            type="search"
            aria-label="Search log"
            aria-invalid={invalid || undefined}
            placeholder="Search or /regex/i"
            value={filter.query}
            // biome-ignore lint/a11y/noAutofocus: opened by the user's click on "+ filter"
            autoFocus
            onChange={(e) => actions.setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                actions.setQuery("");
                setSearchOpen(false);
              }
            }}
          />
        ) : null}
        {invalid ? (
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
      </div>

      <div
        className="log-list"
        role="listbox"
        aria-label="Log entries. Space pauses or resumes follow-tail; Enter selects a line."
        aria-multiselectable
        tabIndex={0}
        data-count={entries.length}
        data-follow={following ? "following" : "paused"}
        onKeyDown={onListKeyDown}
      >
        {entries.length === 0 ? (
          <div className="log-empty">
            {total === 0 ? "No log lines yet" : "No entries match the filter"}
          </div>
        ) : (
          <Virtuoso<LogEntry>
            ref={listRef}
            style={{ height: "100%" }}
            data={entries}
            // Keys are ring positions (unique even when a replay repeats `seq`).
            // Virtuoso may ask for an index past the end while it still renders
            // `initialItemCount` rows after the data shrank (jsdom cannot measure).
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
  );
}
