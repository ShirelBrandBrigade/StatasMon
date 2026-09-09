import type { Capability, MetricId, Unit } from "@statasmon/protocol";
import { Dash, Ring, type Series } from "@statasmon/ui";
import { Children, type ReactNode, useEffect, useState } from "react";
import { useCapability, useLatest } from "../../hooks/useDevice";
import { useAppStore } from "../../store/app";

/** Reason shown while a capability is available (or unknown) but no value has arrived. */
export const NO_SAMPLE = "No sample yet";
/** Fallback when the core marks a capability unavailable without a reason. */
export const NOT_AVAILABLE = "Not available on this device";
/** The app under test (the mockups' FieldKit placeholder); its rows get the gradient tile. */
export const ME_APP_ID = "com.example.fieldkit";
/** A card shows the sample age once the newest sample is older than this. */
export const STALE_AFTER_S = 5;

export const PERCENT = { unit: "percent" } as const satisfies { unit: Unit };
export const BYTES = { unit: "bytes" } as const satisfies { unit: Unit };
export const VOLTS = { unit: "volts" } as const satisfies { unit: Unit };
export const AMPS = { unit: "amps" } as const satisfies { unit: Unit };
export const CELSIUS = { unit: "celsius" } as const satisfies { unit: Unit };
export const COUNT = { unit: "count" } as const satisfies { unit: Unit };
export const LOAD = { unit: "load" } as const satisfies { unit: Unit };

/** Turns a `useCapability` result into the reason a missing value carries. */
export function capabilityReason(cap: { available: boolean | undefined; reason?: string }): string {
  if (cap.available === false) return cap.reason ?? NOT_AVAILABLE;
  return NO_SAMPLE;
}

/** Latest value of a metric plus the reason to show when it is undefined. */
export function useMetric(
  deviceId: string,
  id: MetricId,
  capability: Capability,
): { value: number | undefined; reason: string } {
  const value = useLatest(deviceId, id);
  const cap = useCapability(deviceId, capability);
  return { value, reason: capabilityReason(cap) };
}

/**
 * Seconds since the newest sample reached the store, ticking once per second.
 * Measured from the moment `lastSampleMs` changed (not from the sample's own
 * timestamp) so replayed fixtures with old timestamps are not reported stale.
 */
export function useSampleAge(deviceId: string): number | undefined {
  const lastSampleMs = useAppStore((s) => s.devices[deviceId]?.lastSampleMs);
  const [arrival, setArrival] = useState<{ ts: number | undefined; at: number }>(() => ({
    ts: lastSampleMs,
    at: Date.now(),
  }));
  if (arrival.ts !== lastSampleMs) setArrival({ ts: lastSampleMs, at: Date.now() });
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  if (lastSampleMs === undefined) return undefined;
  const at = arrival.ts === lastSampleMs ? arrival.at : Date.now();
  return Math.max(0, Math.floor((now - at) / 1000));
}

/** "12 s ago", "3 min ago", "2 h ago". */
export function formatAge(seconds: number): string {
  if (seconds < 60) return `${seconds} s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  return `${Math.floor(seconds / 3600)} h ago`;
}

export interface SubtitleProps {
  deviceId: string;
  /** Subtitle fragments joined with " · "; empty entries are skipped. */
  parts: ReactNode[];
}

/** Card subtitle: the parts, then the sample age at the right end when stale. */
export function Subtitle({ deviceId, parts }: SubtitleProps) {
  const age = useSampleAge(deviceId);
  const items = parts.filter((p) => p !== undefined && p !== null && p !== "");
  const stale = age !== undefined && age > STALE_AFTER_S;
  const children: ReactNode[] = [];
  items.forEach((p, i) => {
    if (i > 0) children.push(" · ");
    children.push(p);
  });
  if (stale) {
    children.push(
      <span className="ov-age" data-testid="sample-age">
        {items.length > 0 ? " · " : ""}
        <span className="mono">{formatAge(age)}</span>
      </span>,
    );
  }
  if (children.length === 0) return null;
  return <>{Children.toArray(children)}</>;
}

export interface GaugeProps {
  value: number | undefined;
  /** Why the ring is empty when `value` is undefined (tooltip on the ring). */
  reason: string;
  name: string;
  series: Series;
}

/** 78px ring; an undefined value draws the empty ring with the reason as its tooltip. */
export function Gauge({ value, reason, name, series }: GaugeProps) {
  const unavailable = value === undefined;
  return (
    <span
      className="ov-ring-wrap"
      title={unavailable ? reason : undefined}
      data-unavailable={unavailable ? "true" : undefined}
      data-testid={`gauge-${name.toLowerCase()}`}
    >
      <Ring value={unavailable ? null : value} name={name} series={series} />
    </span>
  );
}

/** Label + Dash row for a detail block the device cannot provide. */
export function DetailDash({ label, reason }: { label: string; reason: string }) {
  return (
    <div className="ov-empty">
      <span>{label}</span>
      <Dash reason={reason} />
    </div>
  );
}

/** Share of `part` in `total` as 0–100, or undefined when either side is missing. */
export function percentOf(part: number | undefined, total: number | undefined): number | undefined {
  if (part === undefined || total === undefined || total <= 0) return undefined;
  return (part / total) * 100;
}
