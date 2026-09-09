import { formatValue } from "@statasmon/protocol";
import type { SVGProps } from "react";
import { clamp, cx, type Series, seriesColor } from "./tokens";

export interface RingProps extends Omit<SVGProps<SVGSVGElement>, "values" | "stroke"> {
  /** Fill 0–100. `null` renders an empty ring with a dash label (metric unavailable). */
  value: number | null;
  /** Rendered size in px. */
  size?: number;
  /** Arc stroke width in px. */
  stroke?: number;
  series?: Series;
  /** Center label; defaults to the rounded percent. */
  label?: string;
  /** Accessible name, e.g. "CPU". Combined with the value in `aria-label`. */
  name: string;
}

const VIEW = 84;
const RADIUS = 34;
const CIRC = 2 * Math.PI * RADIUS;

/** Radial gauge, 78px / stroke 8 by default, drawn with a `stroke-dasharray` arc. */
export function Ring({
  value,
  size = 78,
  stroke = 8,
  series = "cyan",
  label,
  name,
  className,
  ...rest
}: RingProps) {
  const pct = value === null ? 0 : clamp(value, 0, 100);
  const arc = (pct / 100) * CIRC;
  const text = label ?? formatValue({ unit: "percent" }, value === null ? null : Math.round(pct));
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${VIEW} ${VIEW}`}
      role="img"
      aria-label={`${name} ${text}`}
      className={cx("sm-ring", className)}
      {...rest}
    >
      <circle
        cx={VIEW / 2}
        cy={VIEW / 2}
        r={RADIUS}
        fill="none"
        stroke="var(--track)"
        strokeWidth={stroke}
      />
      {value !== null ? (
        <circle
          cx={VIEW / 2}
          cy={VIEW / 2}
          r={RADIUS}
          fill="none"
          stroke={seriesColor(series)}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={`${arc.toFixed(2)} ${CIRC.toFixed(2)}`}
          transform={`rotate(-90 ${VIEW / 2} ${VIEW / 2})`}
          data-testid="ring-arc"
        />
      ) : null}
      <text
        x={VIEW / 2}
        y={VIEW / 2 + 5}
        textAnchor="middle"
        fontSize={15}
        className="sm-ring-label"
      >
        {text}
      </text>
    </svg>
  );
}
