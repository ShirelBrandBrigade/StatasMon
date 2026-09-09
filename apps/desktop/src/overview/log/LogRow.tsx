import type { LogEntry } from "@statasmon/protocol";
import { AppTile, cx, LevelPill } from "@statasmon/ui";
import { memo } from "react";
import { formatClock, isAppUnderTest } from "./format";

export interface LogRowProps {
  entry: LogEntry;
  /** Odd rows get the alternate fill. */
  alt: boolean;
  selected: boolean;
  onToggle(seq: number): void;
}

/** One log line: [app tile · time · process · level pill · mono message]. */
export const LogRow = memo(function LogRow({ entry, alt, selected, onToggle }: LogRowProps) {
  const fault = entry.level === "fault";
  return (
    <div
      className={cx("log-row", alt && "is-alt", fault && "is-fault", selected && "is-selected")}
      role="option"
      aria-selected={selected}
      tabIndex={-1}
      data-seq={entry.seq}
      data-level={entry.level}
      onClick={() => onToggle(entry.seq)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onToggle(entry.seq);
        }
      }}
    >
      <AppTile me={isAppUnderTest(entry)} />
      <span className="log-time mono">{formatClock(entry.ts_ms)}</span>
      <span className="log-proc" title={entry.subsystem ?? entry.tag ?? undefined}>
        {entry.process}
      </span>
      <LevelPill level={entry.level} />
      <span className="log-msg mono" title={entry.message}>
        {entry.message}
      </span>
    </div>
  );
});
