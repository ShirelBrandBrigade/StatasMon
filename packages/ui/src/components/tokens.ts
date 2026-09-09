/**
 * Maps semantic props to CSS custom properties. Components never carry literal
 * colors; the only place a color value exists is tokens.css.
 */

/** Series colors are assigned in fixed order cyan → violet → mint → amber; a fifth series is "other". */
export type Series = "cyan" | "cyan-2" | "vio" | "mint" | "amber" | "red" | "other";

/** Status tone. Always paired with a label or icon in the UI, never color alone. */
export type Tone = "ok" | "warn" | "bad" | "info" | "note" | "neutral";

const SERIES_VAR: Record<Series, string> = {
  cyan: "var(--cyan)",
  "cyan-2": "var(--cyan-2)",
  vio: "var(--vio)",
  mint: "var(--mint)",
  amber: "var(--amber)",
  red: "var(--red)",
  other: "var(--other)",
};

const TONE_VAR: Record<Tone, string> = {
  ok: "var(--mint)",
  warn: "var(--amber)",
  bad: "var(--red)",
  info: "var(--cyan)",
  note: "var(--vio)",
  neutral: "var(--mut)",
};

const TONE_TINT_VAR: Record<Tone, string> = {
  ok: "var(--mint-tint)",
  warn: "var(--amber-tint)",
  bad: "var(--red-tint)",
  info: "var(--cyan-tint)",
  note: "var(--vio-tint)",
  neutral: "var(--neutral-tint)",
};

/** CSS value (a `var(--…)` reference) for a series color. */
export function seriesColor(series: Series): string {
  return SERIES_VAR[series];
}

/** CSS value for a tone's foreground color. */
export function toneColor(tone: Tone): string {
  return TONE_VAR[tone];
}

/** CSS value for a tone's tinted fill (`rgba(<series>, .16)`). */
export function toneTint(tone: Tone): string {
  return TONE_TINT_VAR[tone];
}

/** Joins class names, dropping falsy entries. */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

/** Clamps a number to a range. */
export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
