import { type FieldsetHTMLAttributes, useId } from "react";
import { cx } from "./tokens";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
}

export interface SegmentProps<T extends string>
  extends Omit<FieldsetHTMLAttributes<HTMLFieldSetElement>, "onChange"> {
  options: ReadonlyArray<SegmentOption<T>>;
  value: T;
  onChange: (value: T) => void;
  /** Accessible name for the group. */
  name: string;
}

/**
 * Segmented control built on native radio inputs (arrow keys and focus come for free).
 * The selected option inverts to `--fg` fill with `--bg` text.
 */
export function Segment<T extends string>({
  options,
  value,
  onChange,
  name,
  className,
  ...rest
}: SegmentProps<T>) {
  const group = useId();
  return (
    <fieldset className={cx("sm-seg", className)} aria-label={name} {...rest}>
      {options.map((o) => {
        const checked = o.value === value;
        return (
          <label key={o.value} data-checked={checked ? "true" : "false"}>
            <input
              type="radio"
              name={group}
              value={o.value}
              checked={checked}
              onChange={() => onChange(o.value)}
            />
            <span>{o.label}</span>
          </label>
        );
      })}
    </fieldset>
  );
}
