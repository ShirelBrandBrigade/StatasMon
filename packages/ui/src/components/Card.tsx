import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./tokens";

export interface CardTitleProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  /** Card title, 12.5px / 700. */
  title: ReactNode;
  /** Right-aligned subtitle, 11.5px / 500 in `--dim`. May be a Pill. */
  subtitle?: ReactNode;
}

/** Title row of a card: title left, subtitle right, baseline aligned. */
export function CardTitle({ title, subtitle, className, ...rest }: CardTitleProps) {
  return (
    <div className={cx("sm-card-title", className)} {...rest}>
      <strong style={{ fontWeight: 700 }}>{title}</strong>
      {subtitle !== undefined && subtitle !== null ? <span>{subtitle}</span> : null}
    </div>
  );
}

export interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  title?: ReactNode;
  subtitle?: ReactNode;
  children?: ReactNode;
  /** Optional footer, separated by a 1px `--edge` rule. */
  footer?: ReactNode;
}

/** Glass card: card fill, edge border, 18px radius, blur and shadow. */
export function Card({ title, subtitle, children, footer, className, ...rest }: CardProps) {
  return (
    <section className={cx("glass", "sm-card", className)} {...rest}>
      {title !== undefined ? <CardTitle title={title} subtitle={subtitle} /> : null}
      {children}
      {footer !== undefined ? <div className="sm-card-footer">{footer}</div> : null}
    </section>
  );
}
