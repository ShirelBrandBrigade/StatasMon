import type { SVGProps } from "react";
import { cx } from "./tokens";

/** Every icon is a stroke drawing on a 24px grid at stroke 2. */
export type IconName =
  | "phone"
  | "tablet"
  | "android"
  | "overview"
  | "performance"
  | "network"
  | "apps"
  | "logs"
  | "sessions"
  | "device"
  | "settings"
  | "download"
  | "pause"
  | "play"
  | "record"
  | "mark"
  | "search"
  | "close"
  | "plus"
  | "chevron-down"
  | "warning"
  | "check";

export const ICON_NAMES: readonly IconName[] = [
  "phone",
  "tablet",
  "android",
  "overview",
  "performance",
  "network",
  "apps",
  "logs",
  "sessions",
  "device",
  "settings",
  "download",
  "pause",
  "play",
  "record",
  "mark",
  "search",
  "close",
  "plus",
  "chevron-down",
  "warning",
  "check",
];

type Shape =
  | { t: "path"; d: string; filled?: boolean }
  | { t: "rect"; x: number; y: number; w: number; h: number; r?: number }
  | { t: "circle"; cx: number; cy: number; r: number; filled?: boolean };

const SHAPES: Record<IconName, Shape[]> = {
  phone: [
    { t: "rect", x: 6, y: 2, w: 12, h: 20, r: 3 },
    { t: "path", d: "M11 18h2" },
  ],
  tablet: [
    { t: "rect", x: 3, y: 4, w: 18, h: 16, r: 3 },
    { t: "path", d: "M11 17h2" },
  ],
  android: [
    { t: "path", d: "M5 9a7 7 0 0 1 14 0v7H5z" },
    { t: "path", d: "M8 5l-1-2M16 5l1-2M9 9h.01M15 9h.01" },
  ],
  overview: [
    { t: "rect", x: 3, y: 3, w: 8, h: 8, r: 2 },
    { t: "rect", x: 13, y: 3, w: 8, h: 8, r: 2 },
    { t: "rect", x: 3, y: 13, w: 8, h: 8, r: 2 },
    { t: "rect", x: 13, y: 13, w: 8, h: 8, r: 2 },
  ],
  performance: [{ t: "path", d: "M4 18l5-7 4 4 7-9" }],
  network: [{ t: "path", d: "M2 9a15 15 0 0 1 20 0M5.5 12.5a10 10 0 0 1 13 0M9 16a5 5 0 0 1 6 0" }],
  apps: [{ t: "path", d: "M4 6h16M4 12h16M4 18h10" }],
  logs: [
    { t: "path", d: "M5 3h10l4 4v14H5z" },
    { t: "path", d: "M8 12h8M8 16h8" },
  ],
  sessions: [
    { t: "rect", x: 3, y: 5, w: 18, h: 14, r: 2 },
    { t: "path", d: "M8 3v4M16 3v4" },
  ],
  device: [
    { t: "rect", x: 7, y: 7, w: 10, h: 10, r: 2 },
    { t: "path", d: "M9 3v4M15 3v4M9 17v4M15 17v4M3 9h4M3 15h4M17 9h4M17 15h4" },
  ],
  settings: [
    { t: "path", d: "M4 7h16M4 12h16M4 17h16" },
    { t: "circle", cx: 9, cy: 7, r: 2, filled: true },
    { t: "circle", cx: 15, cy: 12, r: 2, filled: true },
    { t: "circle", cx: 8, cy: 17, r: 2, filled: true },
  ],
  download: [{ t: "path", d: "M12 3v12m0 0l-4-4m4 4l4-4M4 17v3h16v-3" }],
  pause: [{ t: "path", d: "M8 5v14M16 5v14" }],
  play: [{ t: "path", d: "M7 4l12 8-12 8z", filled: true }],
  record: [{ t: "circle", cx: 12, cy: 12, r: 6, filled: true }],
  mark: [{ t: "path", d: "M6 21V4h11l-2 4 2 4H6" }],
  search: [
    { t: "circle", cx: 11, cy: 11, r: 7 },
    { t: "path", d: "M16 16l5 5" },
  ],
  close: [{ t: "path", d: "M6 6l12 12M18 6L6 18" }],
  plus: [{ t: "path", d: "M12 5v14M5 12h14" }],
  "chevron-down": [{ t: "path", d: "M6 9l6 6 6-6" }],
  warning: [
    { t: "path", d: "M12 3l10 18H2z" },
    { t: "path", d: "M12 10v4M12 17.5h.01" },
  ],
  check: [{ t: "path", d: "M5 12l5 5 9-10" }],
};

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "name"> {
  name: IconName;
  /** Rendered size in px (the drawing grid is always 24). */
  size?: number;
  strokeWidth?: number;
  /** Accessible name; when omitted the icon is decorative (`aria-hidden`). */
  label?: string;
}

/** A stroke icon from the fixed set, drawn on a 24px grid, colored by `currentColor`. */
export function Icon({ name, size = 16, strokeWidth = 2, label, className, ...rest }: IconProps) {
  const shapes = SHAPES[name];
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cx("sm-icon", className)}
      data-icon={name}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      {...rest}
    >
      {shapes.map((s, i) => {
        const key = `${name}-${i}`;
        switch (s.t) {
          case "rect":
            return <rect key={key} x={s.x} y={s.y} width={s.w} height={s.h} rx={s.r} />;
          case "circle":
            return (
              <circle
                key={key}
                cx={s.cx}
                cy={s.cy}
                r={s.r}
                fill={s.filled ? "currentColor" : undefined}
                stroke={s.filled ? "none" : undefined}
              />
            );
          default:
            return <path key={key} d={s.d} fill={s.filled ? "currentColor" : undefined} />;
        }
      })}
    </svg>
  );
}
