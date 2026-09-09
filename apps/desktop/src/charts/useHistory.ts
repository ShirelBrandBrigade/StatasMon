import type { HistoryPoint, MetricId } from "@statasmon/protocol";
import { useEffect, useState } from "react";
import { commands } from "../bridge/commands";
import { useRangeMs } from "../hooks/useDevice";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";
import type { SeriesInput } from "./useSeriesData";

/** Upper bound on points one history query returns (docs/PLAN.md 5.2). */
export const HISTORY_MAX_POINTS = 1800;
/** Older points are re-fetched this often while the range still exceeds the buffer. */
export const HISTORY_REFRESH_MS = 30_000;

export interface HistoryState {
  /** Older points per metric (oldest → newest), all strictly before the live buffer. */
  series: SeriesInput[];
  loading: boolean;
}

const IDLE: HistoryState = { series: [], loading: false };

/** Downsampling step so at most `HISTORY_MAX_POINTS` points come back. */
export function historyStep(fromMs: number, toMs: number): number {
  const span = Math.max(0, toMs - fromMs);
  return Math.max(1000, Math.ceil(span / HISTORY_MAX_POINTS / 1000) * 1000);
}

/** The window the chart needs that the mirror buffer does not hold, or undefined when it suffices. */
export function historyWindow(args: {
  windowMs: number | undefined;
  bufferSize: number;
  oldestLiveMs: number | undefined;
  newestLiveMs: number | undefined;
  startedAtMs: number | null | undefined;
  nowMs: number;
}): { fromMs: number; toMs: number } | undefined {
  const { windowMs, bufferSize, oldestLiveMs, newestLiveMs, startedAtMs, nowMs } = args;
  const newest = newestLiveMs ?? nowMs;
  const from = windowMs === undefined ? (startedAtMs ?? undefined) : newest - windowMs;
  if (from === undefined) return undefined;
  const needSecs = (newest - from) / 1000;
  if (bufferSize >= needSecs) return undefined;
  const to = (oldestLiveMs ?? newest) - 1;
  return to > from ? { fromMs: from, toMs: to } : undefined;
}

function toInput(points: HistoryPoint[], beforeMs: number): SeriesInput {
  const t: number[] = [];
  const v: number[] = [];
  for (const p of points) {
    if (p.ts_ms >= beforeMs) break;
    t.push(p.ts_ms);
    v.push(p.value);
  }
  return { t, v };
}

/**
 * Older points from the store for the part of the selected range that the
 * live mirror buffer does not hold (docs/PLAN.md 5.2). Fetches once when the
 * range grows past the buffer, refreshes every 30 s, and steps aside (returns
 * nothing, no request) as soon as the buffer holds the whole range.
 */
export function useHistory(deviceId: string | undefined, metrics: MetricId[]): HistoryState {
  const windowMs = useRangeMs();
  const sessionId = useAppStore((s) => (deviceId ? s.devices[deviceId]?.session?.id : undefined));
  const startedAt = useAppStore((s) =>
    deviceId ? s.devices[deviceId]?.session?.started_at_ms : undefined,
  );
  const streamKind = useAppStore((s) => s.streamKind);
  const [state, setState] = useState<HistoryState>(IDLE);
  const metricKey = metrics.join(",");

  useEffect(() => {
    if (!deviceId || !sessionId || streamKind === "none") {
      setState(IDLE);
      return undefined;
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const ids = metricKey.split(",") as MetricId[];

    const run = async () => {
      const buffer = telemetry.metricsFor(deviceId);
      const live = buffer.window(ids[0]);
      const win = historyWindow({
        windowMs,
        bufferSize: buffer.size,
        oldestLiveMs: live.t[0],
        newestLiveMs: live.t[live.t.length - 1],
        startedAtMs: startedAt,
        nowMs: Date.now(),
      });
      if (!win) {
        setState(IDLE);
        return;
      }
      setState((s) => (s.loading ? s : { ...s, loading: true }));
      const step_ms = historyStep(win.fromMs, win.toMs);
      try {
        const pages = await Promise.all(
          ids.map((metric) =>
            commands.queryHistory({
              device_id: deviceId,
              session_id: null,
              metric,
              from_ms: win.fromMs,
              to_ms: win.toMs,
              step_ms,
            }),
          ),
        );
        if (cancelled) return;
        const before =
          telemetry.metricsFor(deviceId).window(ids[0]).t[0] ?? Number.POSITIVE_INFINITY;
        setState({ series: pages.map((p) => toInput(p ?? [], before)), loading: false });
      } catch {
        if (!cancelled) setState((s) => ({ series: s.series, loading: false }));
      }
    };

    const tick = () => {
      void run().finally(() => {
        if (!cancelled) timer = setTimeout(tick, HISTORY_REFRESH_MS);
      });
    };
    tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [deviceId, sessionId, startedAt, windowMs, metricKey, streamKind]);

  return state;
}
