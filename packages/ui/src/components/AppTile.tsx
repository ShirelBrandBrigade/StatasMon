import type { HTMLAttributes } from "react";
import { cx } from "./tokens";

export interface AppTileProps extends HTMLAttributes<HTMLSpanElement> {
  /** The app under test gets the cyan → violet gradient. */
  me?: boolean;
  /** App name, used as the tooltip and accessible name; without it the tile is decorative. */
  label?: string;
}

/** 20px rounded app-icon square (6px radius). */
export function AppTile({ me = false, label, className, ...rest }: AppTileProps) {
  const cls = cx("sm-tile", me && "is-me", className);
  const dataMe = me ? "true" : undefined;
  if (label) {
    return (
      <span
        className={cls}
        role="img"
        aria-label={label}
        title={label}
        data-me={dataMe}
        {...rest}
      />
    );
  }
  return <span className={cls} aria-hidden data-me={dataMe} {...rest} />;
}
