import type { HTMLAttributes } from "react";
import { cx, type Tone, toneColor } from "./tokens";

export interface StatusDotProps extends HTMLAttributes<HTMLSpanElement> {
  tone: Tone;
  /** Required: status is never color alone. Used as the accessible name and tooltip. */
  label: string;
  /** Soft glow (used for the recording dot). */
  glow?: boolean;
}

/** 7px status dot in a tone color, always carrying a label. */
export function StatusDot({
  tone,
  label,
  glow = false,
  className,
  style,
  ...rest
}: StatusDotProps) {
  return (
    <span
      className={cx("sm-dot", glow && "is-glow", className)}
      role="img"
      aria-label={label}
      title={label}
      data-tone={tone}
      style={{ background: toneColor(tone), color: toneColor(tone), ...style }}
      {...rest}
    />
  );
}
