import type { LogLevel } from "@statasmon/protocol";
import type { HTMLAttributes, ReactNode } from "react";
import { cx, type Tone, toneColor, toneTint } from "./tokens";

export interface PillProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  size?: "sm" | "md";
  children: ReactNode;
}

/** Tinted pill: `rgba(<series>, .16)` fill with the series color as text. */
export function Pill({ tone = "neutral", size = "md", className, style, ...rest }: PillProps) {
  return (
    <span
      className={cx("sm-pill", size === "sm" && "is-sm", className)}
      data-tone={tone}
      style={{ background: toneTint(tone), color: toneColor(tone), ...style }}
      {...rest}
    />
  );
}

const LEVEL: Record<LogLevel, { tone: Tone; label: string }> = {
  debug: { tone: "neutral", label: "Debug" },
  info: { tone: "info", label: "Info" },
  notice: { tone: "note", label: "Notice" },
  warning: { tone: "warn", label: "Warning" },
  error: { tone: "warn", label: "Error" },
  fault: { tone: "bad", label: "Fault" },
};

/** Tone and label for a log level. */
export function levelMeta(level: LogLevel): { tone: Tone; label: string } {
  return LEVEL[level];
}

export interface LevelPillProps extends Omit<PillProps, "children" | "tone"> {
  level: LogLevel;
}

/** Log-level pill: 10.5px, mapped to a tone with a readable label. */
export function LevelPill({ level, size = "sm", className, ...rest }: LevelPillProps) {
  const { tone, label } = levelMeta(level);
  return (
    <Pill
      tone={tone}
      size={size}
      className={className}
      data-level={level}
      style={{ minWidth: 48, justifyContent: "center" }}
      {...rest}
    >
      {label}
    </Pill>
  );
}
