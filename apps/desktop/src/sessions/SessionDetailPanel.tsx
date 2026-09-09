import type { HistoryPoint, MetricId, SessionDetail, SessionSummary } from "@statasmon/protocol";
import { Button, KeyValue, Pill, seriesColor } from "@statasmon/ui";
import { useEffect, useId, useMemo, useState } from "react";
import { commands } from "../bridge/commands";
import { type ChartSeries, TimeSeriesChart } from "../charts/TimeSeriesChart";
import { historyStep } from "../charts/useHistory";
import { formatCount, formatElapsed } from "../shell/format";
import { CompareChart } from "./CompareChart";
import {
  defaultMetric,
  deviceSheetRows,
  eventTone,
  formatStart,
  metricFormatter,
  metricLabel,
  sessionMarkers,
  timeline,
  toChartData,
} from "./sessionsModel";

const CHART_HEIGHT = 220;

/** Whole-session history of one metric from the store (docs/PLAN.md 5.2). */
async function loadHistory(s: SessionSummary, metric: MetricId): Promise<HistoryPoint[]> {
  const from_ms = s.started_at_ms;
  const to_ms = s.ended_at_ms ?? Date.now();
  const points = await commands.queryHistory({
    device_id: s.device_id,
    session_id: s.id,
    metric,
    from_ms,
    to_ms,
    step_ms: historyStep(from_ms, to_ms),
  });
  return points ?? [];
}

function useHistoryOf(
  summary: SessionSummary | undefined,
  metric: MetricId | undefined,
): { points: HistoryPoint[]; loading: boolean } {
  const [state, setState] = useState<{ key: string; points: HistoryPoint[]; loading: boolean }>({
    key: "",
    points: [],
    loading: false,
  });
  const key = summary && metric ? `${summary.id}|${metric}` : "";
  useEffect(() => {
    if (!summary || !metric) {
      setState({ key: "", points: [], loading: false });
      return undefined;
    }
    let cancelled = false;
    setState((s) => ({ key, points: s.key === key ? s.points : [], loading: true }));
    void loadHistory(summary, metric)
      .then((points) => {
        if (!cancelled) setState({ key, points, loading: false });
      })
      .catch(() => {
        if (!cancelled) setState({ key, points: [], loading: false });
      });
    return () => {
      cancelled = true;
    };
  }, [summary, metric, key]);
  return state.key === key ? state : { points: [], loading: Boolean(key) };
}

const pad2 = (n: number) => String(n).padStart(2, "0");
function clock(tsMs: number): string {
  const d = new Date(tsMs);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

export interface SessionDetailPanelProps {
  detail: SessionDetail;
  /** Every other session that can be compared against this one. */
  others: SessionSummary[];
  onExport: (session: SessionSummary) => void;
}

/**
 * Read-only view of a stored (or still-recording) session: device sheet
 * summary, marks + events timeline, and one metric charted from the store.
 * "Compare" overlays the same metric from a second session on elapsed time.
 */
export function SessionDetailPanel({ detail, others, onExport }: SessionDetailPanelProps) {
  const uid = useId();
  const { summary, info, marks, events, metrics } = detail;
  const [metric, setMetric] = useState<MetricId | undefined>(() => defaultMetric(metrics));
  const [compare, setCompare] = useState(false);
  const [otherId, setOtherId] = useState<string>("");
  // A newly opened session resets the metric to its own default.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `summary.id` is the reset key.
  useEffect(() => {
    setMetric((m) => (m && metrics.includes(m) ? m : defaultMetric(metrics)));
    setCompare(false);
    setOtherId("");
  }, [summary.id]);

  const other = useMemo(() => others.find((o) => o.id === otherId), [others, otherId]);
  const a = useHistoryOf(summary, metric);
  const b = useHistoryOf(compare ? other : undefined, metric);

  const items = useMemo(() => timeline(marks, events), [marks, events]);
  const markers = useMemo(() => sessionMarkers(marks, events), [marks, events]);
  const series = useMemo<ChartSeries[]>(
    () =>
      metric
        ? [{ id: metric, label: metricLabel(metric), color: seriesColor("cyan"), fill: true }]
        : [],
    [metric],
  );
  const data = useMemo(() => toChartData(a.points), [a.points]);
  const fmt = useMemo(() => (metric ? metricFormatter(metric) : undefined), [metric]);
  const sheet = useMemo(() => deviceSheetRows(info), [info]);
  const metricId = `${uid}-metric`;
  const otherSelectId = `${uid}-other`;

  return (
    <div className="sess-detail" data-testid="session-detail" data-session-id={summary.id}>
      <div className="flex flex-wrap items-center gap-2.5">
        <h2 className="m-0 text-title font-bold">
          {summary.device_name} <span className="text-mut">{summary.model_name}</span>
        </h2>
        {summary.live ? (
          <Pill tone="bad" size="sm">
            Live
          </Pill>
        ) : (
          <Pill tone="neutral" size="sm">
            Read-only
          </Pill>
        )}
        <span className="mono text-subtitle text-mut">
          {formatStart(summary.started_at_ms)} · {formatElapsed(summary.elapsed_ms)}
        </span>
        <Button size="sm" icon="download" className="ml-auto" onClick={() => onExport(summary)}>
          Export
        </Button>
      </div>

      <div className="sess-detail-grid">
        <section aria-labelledby={`${uid}-dev`} className="flex flex-col gap-2">
          <h3 id={`${uid}-dev`} className="section-label m-0">
            Device
          </h3>
          {sheet.length > 0 ? (
            <KeyValue rows={sheet} />
          ) : (
            <p className="sess-note m-0">No device information was stored with this session.</p>
          )}
        </section>
        <section aria-labelledby={`${uid}-tl`} className="flex min-w-0 flex-col gap-2">
          <h3 id={`${uid}-tl`} className="section-label m-0">
            Marks and events · {formatCount(items.length)}
          </h3>
          {items.length > 0 ? (
            <ul className="sess-timeline" aria-label="Marks and events">
              {items.map((it) => (
                <li key={it.key} data-kind={it.kind}>
                  <time className="mono" dateTime={new Date(it.tsMs).toISOString()}>
                    {clock(it.tsMs)}
                  </time>
                  <Pill tone={eventTone(it.kind)} size="sm">
                    {it.label}
                  </Pill>
                  <span className="sess-timeline-detail">
                    {it.detail}
                    {it.process ? <span> · {it.process}</span> : null}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="sess-note m-0">No marks or events.</p>
          )}
        </section>
      </div>

      <section aria-labelledby={`${uid}-chart`} className="flex flex-col gap-2">
        <div className="sess-controls">
          <h3 id={`${uid}-chart`} className="section-label m-0">
            Metric
          </h3>
          <label htmlFor={metricId}>
            <select
              id={metricId}
              className="sess-select"
              aria-label="Metric"
              value={metric ?? ""}
              disabled={metrics.length === 0}
              onChange={(e) => setMetric(e.target.value as MetricId)}
            >
              {metrics.length === 0 ? <option value="">No samples stored</option> : null}
              {metrics.map((m) => (
                <option key={m} value={m}>
                  {metricLabel(m)}
                </option>
              ))}
            </select>
          </label>
          <Button
            size="sm"
            aria-pressed={compare}
            disabled={others.length === 0}
            title={others.length === 0 ? "No other session to compare with" : undefined}
            onClick={() => setCompare((c) => !c)}
          >
            Compare
          </Button>
          {compare ? (
            <label htmlFor={otherSelectId}>
              <span>with</span>
              <select
                id={otherSelectId}
                className="sess-select"
                aria-label="Second session"
                value={otherId}
                onChange={(e) => setOtherId(e.target.value)}
              >
                <option value="">Choose a session…</option>
                {others.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.device_name} · {formatStart(o.started_at_ms)}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          {a.loading || b.loading ? (
            <span className="sess-note" role="status">
              loading history…
            </span>
          ) : null}
        </div>
        {!metric ? (
          <p className="sess-note m-0">This session stored no samples.</p>
        ) : compare && other ? (
          <CompareChart
            metric={metric}
            a={{
              sessionId: summary.id,
              deviceName: summary.device_name,
              startedAtMs: summary.started_at_ms,
              points: a.points,
              marks,
              events,
            }}
            b={{
              sessionId: other.id,
              deviceName: other.device_name,
              startedAtMs: other.started_at_ms,
              points: b.points,
            }}
            height={CHART_HEIGHT}
          />
        ) : (
          <TimeSeriesChart
            name={`${metricLabel(metric)} over the session`}
            series={series}
            markers={markers}
            data={data}
            height={CHART_HEIGHT}
            yFormat={fmt}
            yMin="auto"
          />
        )}
      </section>
    </div>
  );
}
