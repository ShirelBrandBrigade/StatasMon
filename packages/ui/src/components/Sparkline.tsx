import { type SVGProps, useId } from "react";
import { cx, type Series, seriesColor } from "./tokens";

export interface SparklineProps
  extends Omit<SVGProps<SVGSVGElement>, "values" | "width" | "height" | "fill"> {
  values: number[];
  width?: number;
  height?: number;
  series?: Series;
  /** Draw a gradient area (.35 → 0 alpha) under the line. */
  fill?: boolean;
  /** Mark the last point with a dot outlined in `--bg`. */
  endpoint?: boolean;
  /** Fixed y-range; defaults to the data range (with a flat-line guard). */
  range?: [number, number];
  strokeWidth?: number;
  /** Accessible name. */
  name: string;
}

const PAD = 4;

/** Points in SVG coordinates for `values` scaled to the box. */
export function sparklinePoints(
  values: number[],
  width: number,
  height: number,
  range?: [number, number],
): Array<[number, number]> {
  if (values.length === 0) return [];
  let [min, max] = range ?? [Math.min(...values), Math.max(...values)];
  if (max - min < 1e-9) {
    min -= 1;
    max += 1;
  }
  const stepX = values.length > 1 ? width / (values.length - 1) : 0;
  const usable = height - PAD * 2;
  return values.map((v, i) => {
    const x = values.length > 1 ? i * stepX : width;
    const y = PAD + (1 - (v - min) / (max - min)) * usable;
    return [Number(x.toFixed(2)), Number(y.toFixed(2))];
  });
}

/** Pure-SVG line chart with an optional gradient fill and endpoint dot. */
export function Sparkline({
  values,
  width = 300,
  height = 50,
  series = "cyan",
  fill = false,
  endpoint = false,
  range,
  strokeWidth = 2,
  name,
  className,
  ...rest
}: SparklineProps) {
  const id = useId();
  const gradientId = `sm-spark-${id}`;
  const pts = sparklinePoints(values, width, height, range);
  const polyline = pts.map(([x, y]) => `${x},${y}`).join(" ");
  const last = pts.at(-1);
  const color = seriesColor(series);
  const area =
    pts.length > 1 && last
      ? `M${pts[0][0]} ${pts[0][1]} ${pts
          .slice(1)
          .map(([x, y]) => `L${x} ${y}`)
          .join(" ")} L${last[0]} ${height} L${pts[0][0]} ${height}Z`
      : "";
  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={name}
      className={cx("sm-spark", className)}
      style={{ display: "block", overflow: "visible" }}
      {...rest}
    >
      {fill && area ? (
        <>
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor={color} stopOpacity={0.35} />
              <stop offset="1" stopColor={color} stopOpacity={0} />
            </linearGradient>
          </defs>
          <path d={area} fill={`url(#${gradientId})`} data-testid="spark-area" />
        </>
      ) : null}
      {pts.length > 1 ? (
        <polyline
          points={polyline}
          fill="none"
          stroke={color}
          strokeWidth={strokeWidth}
          strokeLinejoin="round"
          strokeLinecap="round"
          data-testid="spark-line"
        />
      ) : null}
      {endpoint && last ? (
        <circle
          cx={last[0]}
          cy={last[1]}
          r={4}
          fill={color}
          stroke="var(--bg)"
          strokeWidth={2}
          data-testid="spark-endpoint"
        />
      ) : null}
    </svg>
  );
}
