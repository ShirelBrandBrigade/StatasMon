import type { Settings } from "@statasmon/protocol";

/**
 * Bounds mirrored from src-tauri/src/settings.rs (`validate`). The core is
 * the authority; the UI repeats them so the hints and inline messages are
 * right before a round trip.
 */
export const LIMITS = {
  fast_ms: { min: 250 },
  medium_ms: { min: 1000 },
  slow_ms: { min: 5000 },
  heavy_ms: { min: 30_000 },
  max_session_hours: { min: 1, max: 48 },
  keep_days: { min: 1, max: 365 },
} as const;

/** What the inputs hold: strings, so a half-typed number is representable. */
export interface Draft {
  fast_ms: string;
  medium_ms: string;
  slow_ms: string;
  heavy_ms: string;
  max_session_hours: string;
  keep_days: string;
  /** Empty = no cap. */
  max_total_gb: string;
  adb_path: string;
  update_check: boolean;
}

export type DraftField = keyof Draft;

export type DraftErrors = Partial<Record<DraftField, string>>;

/** Mirrors `Settings::default()` in the core. */
export function defaultSettings(): Settings {
  return {
    sampling: { fast_ms: 1000, medium_ms: 3000, slow_ms: 15000, heavy_ms: 0 },
    retention: { max_session_hours: 4, keep_days: 30, max_total_gb: null },
    adb_path: null,
    update_check: false,
    theme: "dark",
    version: 1,
  };
}

export function toDraft(s: Settings): Draft {
  return {
    fast_ms: String(s.sampling.fast_ms),
    medium_ms: String(s.sampling.medium_ms),
    slow_ms: String(s.sampling.slow_ms),
    heavy_ms: String(s.sampling.heavy_ms),
    max_session_hours: String(s.retention.max_session_hours),
    keep_days: String(s.retention.keep_days),
    max_total_gb: s.retention.max_total_gb === null ? "" : String(s.retention.max_total_gb),
    adb_path: s.adb_path ?? "",
    update_check: s.update_check,
  };
}

/** Whole non-negative integer, or `undefined` when the text is not one. */
function intOf(text: string): number | undefined {
  const t = text.trim();
  if (!/^\d+$/.test(t)) return undefined;
  return Number(t);
}

/** Positive finite decimal, or `undefined`. */
function decimalOf(text: string): number | undefined {
  const t = text.trim();
  if (!/^\d+(\.\d+)?$/.test(t)) return undefined;
  const n = Number(t);
  return Number.isFinite(n) ? n : undefined;
}

/** Every problem in the draft, keyed by field; empty when it can be saved. */
export function validateDraft(d: Draft): DraftErrors {
  const errors: DraftErrors = {};
  const atLeast = (field: "fast_ms" | "medium_ms" | "slow_ms", label: string) => {
    const n = intOf(d[field]);
    if (n === undefined || n < LIMITS[field].min) {
      errors[field] = `${label} must be a whole number of at least ${LIMITS[field].min} ms.`;
    }
  };
  atLeast("fast_ms", "Fast");
  atLeast("medium_ms", "Medium");
  atLeast("slow_ms", "Slow");
  const heavy = intOf(d.heavy_ms);
  if (heavy === undefined || (heavy !== 0 && heavy < LIMITS.heavy_ms.min)) {
    errors.heavy_ms = `Heavy must be 0 (off) or at least ${LIMITS.heavy_ms.min} ms.`;
  }
  const hours = intOf(d.max_session_hours);
  if (
    hours === undefined ||
    hours < LIMITS.max_session_hours.min ||
    hours > LIMITS.max_session_hours.max
  ) {
    errors.max_session_hours = `Hours per session must be from ${LIMITS.max_session_hours.min} to ${LIMITS.max_session_hours.max}.`;
  }
  const days = intOf(d.keep_days);
  if (days === undefined || days < LIMITS.keep_days.min || days > LIMITS.keep_days.max) {
    errors.keep_days = `Days to keep must be from ${LIMITS.keep_days.min} to ${LIMITS.keep_days.max}.`;
  }
  if (d.max_total_gb.trim() !== "") {
    const gb = decimalOf(d.max_total_gb);
    if (gb === undefined || gb <= 0) {
      errors.max_total_gb = "Total size must be a positive number of GB, or empty for no cap.";
    }
  }
  return errors;
}

/** The settings to send, from a draft that passed `validateDraft`. */
export function fromDraft(d: Draft, base: Settings): Settings {
  return {
    ...base,
    sampling: {
      fast_ms: intOf(d.fast_ms) ?? base.sampling.fast_ms,
      medium_ms: intOf(d.medium_ms) ?? base.sampling.medium_ms,
      slow_ms: intOf(d.slow_ms) ?? base.sampling.slow_ms,
      heavy_ms: intOf(d.heavy_ms) ?? base.sampling.heavy_ms,
    },
    retention: {
      max_session_hours: intOf(d.max_session_hours) ?? base.retention.max_session_hours,
      keep_days: intOf(d.keep_days) ?? base.retention.keep_days,
      max_total_gb: d.max_total_gb.trim() === "" ? null : (decimalOf(d.max_total_gb) ?? null),
    },
    adb_path: d.adb_path.trim() === "" ? null : d.adb_path.trim(),
    update_check: d.update_check,
  };
}

/** True when the draft differs from what the core last confirmed. */
export function isDirty(d: Draft, saved: Settings): boolean {
  const s = toDraft(saved);
  return (Object.keys(s) as DraftField[]).some((k) => {
    const a = d[k];
    const b = s[k];
    if (typeof a === "string" && typeof b === "string") return a.trim() !== b.trim();
    return a !== b;
  });
}
