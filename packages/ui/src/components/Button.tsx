import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import { cx } from "./tokens";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost";
  size?: "sm" | "md";
  /** Leading icon from the fixed icon set. */
  icon?: IconName;
  children: ReactNode;
}

/** Primary (cyan fill, dark text, glow) or ghost (card fill + edge) button; hit target ≥ 32px. */
export function Button({
  variant = "ghost",
  size = "md",
  icon,
  className,
  type = "button",
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cx("sm-btn", `is-${variant}`, `is-${size}`, className)}
      {...rest}
    >
      {icon ? <Icon name={icon} size={13} strokeWidth={2.5} /> : null}
      {children}
    </button>
  );
}

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: IconName;
  /** Required accessible name (the button has no visible text). */
  label: string;
  size?: number;
}

/** 32×32 ghost button holding a single icon. */
export function IconButton({
  icon,
  label,
  size = 16,
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  return (
    <button
      type={type}
      className={cx("sm-icon-btn", className)}
      aria-label={label}
      title={label}
      {...rest}
    >
      <Icon name={icon} size={size} />
    </button>
  );
}
