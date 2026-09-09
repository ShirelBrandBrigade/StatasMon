import { formatValue, type MetricMeta } from "@statasmon/protocol";
import type { HTMLAttributes } from "react";
import { Dash } from "./Dash";
import { cx, type Series, seriesColor } from "./tokens";

export interface MetricFigureProps extends HTMLAttributes<HTMLDivElement> {
  /** Unit source; usually the metric's `MetricMeta`. */
  meta: Pick<MetricMeta, "unit">;
  value: number | null | undefined;
  /** Extra words after the unit, e.g. "down" → "Mb/s down". */
  suffix?: string;
  /** Small caption under the figure (mono, `--dim`). */
  caption?: string;
  /** Adds a colored mark beside the number (text stays `--fg`). */
  series?: Series;
  size?: "md" | "lg";
  /** Why the value is unavailable; shown as the dash tooltip. */
  unavailableReason?: string;
}

const SPLIT = /^([-+−]?[\d., ]*\d)\s?(.*)$/;

/** Splits a formatted value into its number and unit parts. */
export function splitFigure(text: string): { number: string; unit: string } | null {
  const m = SPLIT.exec(text);
  if (!m) return null;
  return { number: m[1], unit: m[2] };
}

/** Big mono number with a small unit and an optional caption, formatted via `formatValue`. */
export function MetricFigure({
  meta,
  value,
  suffix,
  caption,
  series,
  size = "md",
  unavailableReason = "Not reported by this device",
  className,
  ...rest
}: MetricFigureProps) {
  const text = formatValue(meta, value);
  const parts = text === "—" ? null : splitFigure(text);
  const unit = [parts?.unit, suffix].filter(Boolean).join(" ");
  return (
    <div className={cx("sm-figure", size === "lg" && "is-lg", className)} {...rest}>
      <div className="sm-figure-value mono">
        {series ? (
          <i className="sm-figure-mark" style={{ background: seriesColor(series) }} aria-hidden />
        ) : null}
        {text === "—" ? (
          <Dash reason={unavailableReason} />
        ) : (
          <span data-testid="figure-number">{parts ? parts.number : text}</span>
        )}
        {unit ? <small className="sm-figure-unit">{unit}</small> : null}
      </div>
      {caption ? <div className="sm-figure-caption mono">{caption}</div> : null}
    </div>
  );
}
