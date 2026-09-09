import type { Capability, MetricId } from "@statasmon/protocol";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
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
 * render per animation frame. Returns a counter that changes on each update;
 * read the actual data from `telemetry` inside the component.
 */
export function useTelemetryTick(deviceId: string | undefined, kinds: TelemetryKind[]): number {
  const [tick, setTick] = useState(0);
  const pending = useRef(false);
  const kindKey = kinds.join(",");
  useEffect(() => {
    if (!deviceId) return undefined;
    const wanted = new Set(kindKey.split(","));
    return telemetry.subscribe((id, kind) => {
      if (id !== deviceId || !wanted.has(kind) || pending.current) return;
      pending.current = true;
      requestAnimationFrame(() => {
        pending.current = false;
        setTick((t) => t + 1);
      });
    });
  }, [deviceId, kindKey]);
  return tick;
}

/** Time series of one metric (oldest → newest) limited to the current range. */
export function useMetricSeries(
  deviceId: string | undefined,
  metric: MetricId,
): { t: number[]; v: number[] } {
  const tick = useTelemetryTick(deviceId, ["samples"]);
  const windowMs = useRangeMs();
  return useMemo(() => {
    if (!deviceId) return { t: [], v: [] };
    return telemetry.metricsFor(deviceId).window(metric, windowMs);
    // `tick` is the invalidation signal for data that lives outside React.
  }, [deviceId, metric, windowMs, tick]);
}

/** Newest per-core sample for a device. */
export function useCores(deviceId: string | undefined) {
  const tick = useTelemetryTick(deviceId, ["cores"]);
  return useMemo(() => (deviceId ? (telemetry.cores.get(deviceId) ?? []) : []), [deviceId, tick]);
}

/** Newest process table for a device. */
export function useProcesses(deviceId: string | undefined) {
  const tick = useTelemetryTick(deviceId, ["processes"]);
  return useMemo(
    () => (deviceId ? (telemetry.processes.get(deviceId) ?? []) : []),
    [deviceId, tick],
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
