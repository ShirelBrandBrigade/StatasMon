import type { LogEntry, ProcessRow } from "@statasmon/protocol";
import { formatValue, shortName } from "@statasmon/protocol";
import { AppTile, Button, IconButton, Pill } from "@statasmon/ui";
import { useMemo } from "react";
import { Virtuoso } from "react-virtuoso";
import { type ChartData, TimeSeriesChart } from "../../charts";
import { openExportDialog } from "../../export/exportStore";
import { useDevice, useRangeMs } from "../../hooks/useDevice";
import { formatClock, plural } from "../../overview/log";
import { LogRow } from "../../overview/log/LogRow";
import { useLogFilterStore } from "../../store/logFilter";
import { eventKey, eventMeta, eventsForProcess } from "../logs/eventMeta";
import { isAppUnderTest } from "./processModel";
import { PROCESS_HISTORY_POINTS, useProcessHistory } from "./useProcessHistory";
import { useProcessLog } from "./useProcessLog";

const CHART_HEIGHT = 84;
/** Row height estimate for Virtuoso before it measures (jsdom cannot). */
const ROW_HEIGHT = 26;

const percent = (v: number) => formatValue({ unit: "percent" }, v);
const bytes = (v: number) => formatValue({ unit: "bytes" }, v);
const bps = (v: number) => formatValue({ unit: "bytes_per_sec" }, v);

export interface AppDetailProps {
  deviceId: string;
  row: ProcessRow;
  /** The process is no longer in the newest table. */
  exited: boolean;
  onClose(): void;
  /** Test hook: rows Virtuoso renders before it can measure. */
  initialItemCount?: number;
}

function ProcessChart({
  name,
  t,
  v,
  color,
  yFormat,
  windowMs,
}: {
  name: string;
  t: number[];
  v: number[];
  color: string;
  yFormat: (value: number) => string;
  windowMs: number | undefined;
}) {
  const data = useMemo<ChartData>(() => ({ t, v: [v] }), [t, v]);
  const series = useMemo(
    () => [{ id: name.toLowerCase(), label: name, color, fill: true, endpoint: true }],
    [name, color],
  );
  return (
    <div className="ad-chart" data-testid={`chart-${name.toLowerCase()}`}>
      <span className="section-label">{name}</span>
      <TimeSeriesChart
        name={`${name} of this process`}
        series={series}
        data={data}
        height={CHART_HEIGHT}
        yFormat={yFormat}
        windowMs={windowMs}
      />
    </div>
  );
}

/**
 * App detail (docs/PLAN.md 6.3, FEATURES.md §8): header, three charts of the
 * process's cpu / memory / network history, its log lines, its events, and
 * the export / focus actions.
 */
export function AppDetail({ deviceId, row, exited, onClose, initialItemCount }: AppDetailProps) {
  const names = useMemo(
    () => (row.app_id && row.app_id !== row.name ? [row.name, row.app_id] : [row.name]),
    [row.name, row.app_id],
  );
  const history = useProcessHistory(deviceId, row.pid);
  const log = useProcessLog(deviceId, names);
  const events = useDevice(deviceId)?.events;
  const mine = useMemo(
    () => (events ? eventsForProcess(events, row.pid, names) : []),
    [events, row.pid, names],
  );
  const windowMs = useRangeMs();
  const me = isAppUnderTest(row);
  const points = history.t.length;

  // The Logs screen's tabs select one process name; the app id is only a
  // secondary match (iOS subsystem), so the tab for this process lights up.
  const focusInLog = () => {
    useLogFilterStore.getState().setProcesses(deviceId, [row.name]);
    window.location.hash = "#logs";
  };

  return (
    <section className="ad" aria-label={`${row.name} detail`} data-pid={row.pid}>
      <header className="ad-head">
        <AppTile me={me} label={row.app_id ?? undefined} className="ad-tile" />
        <div className="ad-title">
          <b title={row.name}>{shortName(row.name)}</b>
          <span className="mono">
            {row.app_id ?? "no app id"} · pid {row.pid}
          </span>
        </div>
        {exited ? (
          <Pill tone="neutral">Exited</Pill>
        ) : row.state === "foreground" ? (
          <Pill tone="info">Foreground</Pill>
        ) : row.state === "background" ? (
          <Pill tone="neutral">Background</Pill>
        ) : null}
        <IconButton icon="close" label="Close detail" onClick={onClose} />
      </header>

      <div className="ad-charts">
        <ProcessChart
          name="CPU"
          t={history.t}
          v={history.cpu}
          color="var(--cyan)"
          yFormat={percent}
          windowMs={windowMs}
        />
        <ProcessChart
          name="Memory"
          t={history.t}
          v={history.mem}
          color="var(--vio)"
          yFormat={bytes}
          windowMs={windowMs}
        />
        <ProcessChart
          name="Network"
          t={history.t}
          v={history.net}
          color="var(--mint)"
          yFormat={bps}
          windowMs={windowMs}
        />
        <span className="ad-note mono" data-testid="history-note">
          {points === 0
            ? "History starts now (the mirror keeps only the newest table)"
            : `${points} of ${PROCESS_HISTORY_POINTS} points since this panel opened`}
        </span>
      </div>

      <div className="ad-events">
        <span className="section-label">Events</span>
        {mine.length === 0 ? (
          <span className="ad-empty">No crash, ANR or memory events for this process</span>
        ) : (
          <ul className="ad-event-list" aria-label="Process events">
            {mine.map((e) => {
              const meta = eventMeta(e.kind);
              return (
                <li key={eventKey(e)}>
                  <time className="mono">{formatClock(e.ts_ms)}</time>
                  <Pill tone={meta.tone} size="sm">
                    {meta.label}
                  </Pill>
                  <span title={e.detail}>{e.detail}</span>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="ad-log">
        <span className="section-label">
          Log · <span className="mono">{plural(log.shown, "line")}</span>
        </span>
        <div
          className="log-list ad-log-list"
          role="listbox"
          aria-label={`Log lines of ${row.name}`}
          tabIndex={0}
          data-count={log.entries.length}
        >
          {log.entries.length === 0 ? (
            <div className="log-empty">No log lines from this process yet</div>
          ) : (
            <Virtuoso<LogEntry>
              style={{ height: "100%" }}
              data={log.entries}
              computeItemKey={(i) => log.keys[i] ?? `gap-${i}`}
              followOutput="auto"
              increaseViewportBy={120}
              defaultItemHeight={ROW_HEIGHT}
              initialItemCount={initialItemCount}
              itemContent={(i, e) =>
                e ? <LogRow entry={e} alt={i % 2 === 1} selected={false} onToggle={noop} /> : null
              }
            />
          )}
        </div>
      </div>

      <footer className="ad-actions">
        <Button
          size="sm"
          icon="download"
          onClick={() => openExportDialog({ deviceId, scope: "app", format: "log" })}
        >
          Export this app's log
        </Button>
        <Button size="sm" icon="logs" onClick={focusInLog}>
          Focus in log
        </Button>
      </footer>
    </section>
  );
}

function noop(): void {}
