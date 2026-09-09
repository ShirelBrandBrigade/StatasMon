import type { MetricId } from "@statasmon/protocol";
import { useMemo, useRef } from "react";
import {
  alignSeries,
  type ChartData,
  prependHistory,
  type SeriesData,
  type SeriesInput,
  useHistory,
} from "../../charts";
import { useRangeMs, useTelemetryValue } from "../../hooks/useDevice";
import { telemetry } from "../../store/telemetry";

const EMPTY: SeriesInput[] = [];

/**
 * N metrics of one device as chart data, limited to the selected range —
 * the multi-series sibling of `useSeriesData` (which is fixed at two). One
 * telemetry subscription reads every metric from the mirror buffer; older
 * points come from the store through `useHistory` when the range reaches
 * past the buffer, and are merged in front of the live ones.
 */
export function useMultiSeriesData(deviceId: string | undefined, ids: MetricId[]): SeriesData {
  const windowMs = useRangeMs();
  const key = ids.join(",");
  const live = useTelemetryValue(
    deviceId,
    ["samples"],
    () => {
      if (!deviceId || key === "") return EMPTY;
      const buffer = telemetry.metricsFor(deviceId);
      return key.split(",").map((id) => buffer.window(id as MetricId, windowMs));
    },
    [key, windowMs],
  );
  const history = useHistory(deviceId, ids);
  const out = useRef<ChartData>({ t: [], v: [] });
  return useMemo(() => {
    const inputs = live.map((s, i) => prependHistory(history.series[i], s));
    const o = alignSeries(inputs, out.current);
    return { t: o.t, v: o.v, loadingHistory: history.loading };
  }, [live, history]);
}
