import type { HTMLAttributes } from "react";
import { cx } from "./tokens";

export interface DashProps extends Omit<HTMLAttributes<HTMLSpanElement>, "title"> {
  /** Why the value is unavailable, e.g. "iOS does not report per-process network". */
  reason: string;
}

/** "—" for a value the platform cannot provide, with the reason as a tooltip (truth over polish). */
export function Dash({ reason, className, ...rest }: DashProps) {
  return (
    <span
      className={cx("sm-dash", "mono", className)}
      role="img"
      title={reason}
      aria-label={`Unavailable: ${reason}`}
      {...rest}
    >
      —
    </span>
  );
}
