import type { HTMLAttributes } from "react";
import { cx, type Series, seriesColor } from "./tokens";

export interface LegendItem {
  label: string;
  /** Already formatted (through `formatValue`) so units stay consistent. */
  value: string;
  series: Series;
}

export interface LegendProps extends HTMLAttributes<HTMLDivElement> {
  items: LegendItem[];
}

/** Swatch + label + mono value, wrapping, 11px in `--mut`. */
export function Legend({ items, className, ...rest }: LegendProps) {
  return (
    <div className={cx("sm-legend", className)} {...rest}>
      {items.map((it) => (
        <span key={it.label}>
          <i style={{ background: seriesColor(it.series) }} aria-hidden />
          {it.label} <em className="mono">{it.value}</em>
        </span>
      ))}
    </div>
  );
}
