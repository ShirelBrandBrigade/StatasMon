import { useMemo } from "react";
import { useTelemetryTick } from "../../hooks/useDevice";
import { compileLogFilter, useLogFilter } from "../../store/logFilter";
import { telemetry } from "../../store/telemetry";
import { type FilteredLog, LogScanner } from "./logScan";

export type { FilteredLog, ProcessCount } from "./logScan";

/**
 * The visible log entries and footer counts for one device. Recomputes only
 * on log ticks (one per animation frame) or when the filter changes; the
 * scan is incremental (see `LogScanner`), so a 10,000 lines/min stream costs
 * a few matches per frame, not a 50k-entry refilter.
 */
export function useFilteredLog(deviceId: string): FilteredLog {
  // React Compiler opt-out: `tick` below is an invalidation signal for data
  // that lives outside React, and the compiler infers memo dependencies from
  // the callback body (ignoring the manual list), which would drop it.
  "use no memo";
  const filter = useLogFilter(deviceId);
  const { processes, levels, query } = filter;
  // A new scanner (full rescan) only when the device or a matching-relevant
  // field changes; `followTail` is not one of them.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `deviceId` keys the scanner to one ring
  const scanner = useMemo(
    () => new LogScanner(compileLogFilter({ processes, levels, query, followTail: true })),
    [deviceId, processes, levels, query],
  );
  const tick = useTelemetryTick(deviceId, ["log"]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `tick` invalidates data outside React
  return useMemo(() => scanner.update(telemetry.logsFor(deviceId)), [deviceId, scanner, tick]);
}
