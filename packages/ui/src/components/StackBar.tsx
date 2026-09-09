import type { HTMLAttributes } from "react";
import { cx, type Series, seriesColor } from "./tokens";

export interface StackSegment {
  label: string;
  /** Share of the bar. Segments are normalised against `total` (default: their sum). */
  value: number;
  series: Series;
}

export interface StackBarProps extends HTMLAttributes<HTMLDivElement> {
  segments: StackSegment[];
  /** Total the segments are measured against; remaining space is left as track. */
  total?: number;
  /** Accessible name for the whole bar. */
  name: string;
}

/** Horizontal stacked bar, 8px tall, 2px gaps, rounded, over the grid track. */
export function StackBar({ segments, total, name, className, ...rest }: StackBarProps) {
  const sum = segments.reduce((acc, s) => acc + Math.max(0, s.value), 0);
  const denom = total !== undefined && total > 0 ? total : sum;
  return (
    <div className={cx("sm-stack", className)} role="img" aria-label={name} {...rest}>
      {segments.map((s) => {
        const width = denom > 0 ? (Math.max(0, s.value) / denom) * 100 : 0;
        return (
          <i
            key={s.label}
            title={s.label}
            data-segment={s.label}
            style={{ width: `${width.toFixed(2)}%`, background: seriesColor(s.series) }}
          />
        );
      })}
    </div>
  );
}
