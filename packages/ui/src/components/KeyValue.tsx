import type { HTMLAttributes, ReactNode } from "react";
import { StatusDot } from "./StatusDot";
import { cx, type Tone } from "./tokens";

export interface KeyValueRow {
  label: string;
  /** Pre-formatted value (string through `formatValue`, or a `Dash`/`Pill`). */
  value: ReactNode;
  /** Adds a colored mark beside the value; the text itself stays `--fg`. */
  tone?: Tone;
}

export interface KeyValueProps extends HTMLAttributes<HTMLDListElement> {
  rows: KeyValueRow[];
}

/** Label / value list, 12px, values in the mono face. */
export function KeyValue({ rows, className, ...rest }: KeyValueProps) {
  return (
    <dl className={cx("sm-kv", className)} style={{ margin: 0 }} {...rest}>
      {rows.map((r) => (
        <div key={r.label}>
          <dt>
            <span>{r.label}</span>
          </dt>
          <dd className="sm-kv-value mono" style={{ margin: 0 }}>
            {r.tone ? <StatusDot tone={r.tone} label={r.tone} /> : null}
            {r.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}
