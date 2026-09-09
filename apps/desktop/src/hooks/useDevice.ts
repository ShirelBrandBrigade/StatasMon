import type { Capability, CoreSample, MetricId, ProcessRow } from "@statasmon/protocol";
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useShallow } from "zustand/react/shallow";
import { type DeviceEntry, RANGE_MS, useAppStore } from "../store/app";
import { type TelemetryKind, telemetry } from "../store/telemetry";

/** The active device entry (undefined until a device attaches). */
export function useActiveDevice(): DeviceEntry | undefined {
  return useAppStore((s) => (s.activeDeviceId ? s.devices[s.activeDeviceId] : undefined));
}

export function useDevice(deviceId: string | undefined): DeviceEntry | undefined {
  return useAppStore((s) => (deviceId ? s.devices[deviceId] : undefined));
}

/** The latest value of one metric for a device (re-renders at most once per sample tick). */
export function useLatest(deviceId: string | undefined, metric: MetricId): number | undefined {
  return useAppStore((s) => (deviceId ? s.devices[deviceId]?.latest[metric] : undefined));
}

/**
 * Availability of a capability with its reason. `undefined` availability
 * means the capability set has not arrived yet.
 */
export function useCapability(
  deviceId: string | undefined,
  capability: Capability,
): { available: boolean | undefined; reason?: string } {
  return useAppStore(
    useShallow((s) => {
      const entry = deviceId ? s.devices[deviceId]?.capabilities?.entries : undefined;
      if (!entry) return { available: undefined };
      const e = entry.find((x) => x.capability === capability);
      if (!e) return { available: undefined };
      if (e.availability.status === "available") return { available: true };
      return { available: false, reason: e.availability.reason };
    }),
  );
}

/** Window in ms for the selected range (undefined = whole session). */
export function useRangeMs(): number | undefined {
  return useAppStore((s) => RANGE_MS[s.range]);
}

/**
 * Subscribes to telemetry updates of `kinds` for a device, throttled to one
 * render per animation frame. Returns a counter that changes on each update.
 * Prefer `useTelemetryValue` for reading data: the React Compiler memoizes
 * on the dependencies it can see, so a `useMemo` keyed on this counter alone
 * is NOT enough to recompute.
 */
export function useTelemetryTick(deviceId: string | undefined, kinds: TelemetryKind[]): number {
  const tick = useTelemetryValue(deviceId, kinds, () => Date.now());
  return tick;
}

/**
 * Reads a value derived from the mirror buffers and re-reads it (at most
 * once per animation frame) whenever telemetry of `kinds` arrives for the
 * device, or when `deps` change. `read` may close over anything; the latest
 * version is always used.
 */
export function useTelemetryValue<T>(
  deviceId: string | undefined,
  kinds: TelemetryKind[],
  read: () => T,
  deps: readonly unknown[] = [],
): T {
  const readRef = useRef(read);
  readRef.current = read;
  const [value, setValue] = useState<T>(read);
  const kindKey = kinds.join(",");
  useEffect(() => {
    setValue(() => readRef.current());
    if (!deviceId) return undefined;
    const wanted = new Set(kindKey.split(","));
    let pending = false;
    let cancelled = false;
    const unsubscribe = telemetry.subscribe((id, kind) => {
      if (id !== deviceId || !wanted.has(kind) || pending) return;
      pending = true;
      requestAnimationFrame(() => {
        pending = false;
        if (!cancelled) setValue(() => readRef.current());
      });
    });
    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [deviceId, kindKey, ...deps]);
  return value;
}

const EMPTY_SERIES = { t: [] as number[], v: [] as number[] };

/** Time series of one metric (oldest → newest) limited to the current range. */
export function useMetricSeries(
  deviceId: string | undefined,
  metric: MetricId,
): { t: number[]; v: number[] } {
  const windowMs = useRangeMs();
  return useTelemetryValue(
    deviceId,
    ["samples"],
    () => (deviceId ? telemetry.metricsFor(deviceId).window(metric, windowMs) : EMPTY_SERIES),
    [metric, windowMs],
  );
}

const EMPTY_CORES: CoreSample[] = [];
const EMPTY_PROCESSES: ProcessRow[] = [];

/** Newest per-core sample for a device. */
export function useCores(deviceId: string | undefined): CoreSample[] {
  return useTelemetryValue(deviceId, ["cores"], () =>
    deviceId ? (telemetry.cores.get(deviceId) ?? EMPTY_CORES) : EMPTY_CORES,
  );
}

/** Newest process table for a device. */
export function useProcesses(deviceId: string | undefined): ProcessRow[] {
  return useTelemetryValue(deviceId, ["processes"], () =>
    deviceId ? (telemetry.processes.get(deviceId) ?? EMPTY_PROCESSES) : EMPTY_PROCESSES,
  );
}

/** Total entries in the log mirror (changes on every log batch). */
export function useLogCount(deviceId: string | undefined): number {
  const subscribe = useCallback(
    (cb: () => void) =>
      telemetry.subscribe((id, kind) => id === deviceId && kind === "log" && cb()),
    [deviceId],
  );
  return useSyncExternalStore(subscribe, () => (deviceId ? telemetry.logsFor(deviceId).size : 0));
}
