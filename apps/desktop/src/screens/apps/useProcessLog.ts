import { useMemo } from "react";
import { useTelemetryTick } from "../../hooks/useDevice";
import { type FilteredLog, LogScanner } from "../../overview/log";
import { ALL_LEVELS, compileLogFilter } from "../../store/logFilter";
import { telemetry } from "../../store/telemetry";

/** Joins the names into one memo key; names may contain spaces, never NUL. */
const SEP = String.fromCharCode(0);

/**
 * The log lines of one process (every level), scanned incrementally like the
 * Overview card's `useFilteredLog` but with a fixed filter instead of the
 * per-device store: `names` are matched against `process` and `subsystem`,
 * so pass the process name and, when known, the app id (an iOS subsystem or
 * an Android package name).
 */
export function useProcessLog(deviceId: string, names: readonly string[]): FilteredLog {
  // React Compiler opt-out: `tick` is an invalidation signal for data outside React.
  "use no memo";
  const nameKey = names.join(SEP);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `nameKey` is the content key for `names`
  const scanner = useMemo(
    () =>
      new LogScanner(
        compileLogFilter({
          processes: nameKey === "" ? [] : nameKey.split(SEP),
          levels: [...ALL_LEVELS],
          query: "",
          followTail: true,
        }),
      ),
    [deviceId, nameKey],
  );
  const tick = useTelemetryTick(deviceId, ["log"]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `tick` invalidates data outside React
  return useMemo(() => scanner.update(telemetry.logsFor(deviceId)), [deviceId, scanner, tick]);
}
