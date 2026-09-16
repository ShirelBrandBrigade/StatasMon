import { type LogEntry, shortName } from "@statasmon/protocol";
import { cx } from "@statasmon/ui";
import { useMemo, useState } from "react";
import { Virtuoso } from "react-virtuoso";
import { formatClock, formatCount, isAppUnderTest, plural } from "./format";
import { buildStories, type Story } from "./logExplain";

export interface SimpleLogProps {
  entries: readonly LogEntry[];
  keys: readonly number[];
  followTail: boolean;
  initialItemCount?: number;
}

function timeSpan(s: Story): string {
  const a = formatClock(s.firstMs);
  const b = formatClock(s.lastMs);
  return a === b ? a : `${a} – ${b}`;
}

function who(s: Story): string {
  const names = s.processes.map(shortName);
  if (names.length <= 3) return names.join(", ");
  return `${names.slice(0, 3).join(", ")} and ${formatCount(names.length - 3)} more`;
}

/**
 * The log as plain sentences (ADR-020): one row per story, a count when
 * a line repeated, routine chatter folded into one grey row, and the raw
 * lines behind a "Raw" toggle on every row.
 */
export function SimpleLog({ entries, keys, followTail, initialItemCount }: SimpleLogProps) {
  const stories = useMemo(() => buildStories(entries, keys), [entries, keys]);
  const [open, setOpen] = useState<ReadonlySet<number>>(() => new Set());
  const toggle = (key: number) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const notable = stories.filter((s) => !s.routine).length;
  const routineLines = stories.filter((s) => s.routine).reduce((n, s) => n + s.count, 0);
  return (
    <div className="simple-log">
      <p className="simple-log-hint" data-testid="simple-hint">
        {notable === 0
          ? "Nothing notable so far. "
          : `${plural(notable, "thing")} worth knowing about; `}
        {formatCount(routineLines)} routine lines folded away.
        <span className="simple-log-legend">
          <i data-tone="ok" /> fine <i data-tone="info" /> information <i data-tone="warn" /> worth
          a look <i data-tone="bad" /> a problem
        </span>
      </p>
      <Virtuoso<Story>
        style={{ height: "100%" }}
        data={stories}
        computeItemKey={(i, s) => s?.key ?? `gap-${i}`}
        followOutput={followTail ? "auto" : false}
        increaseViewportBy={200}
        initialItemCount={initialItemCount}
        itemContent={(_i, s) =>
          s ? (
            <div
              className={cx("story", s.routine && "is-routine", open.has(s.key) && "is-open")}
              data-tone={s.tone}
              data-rule={s.rule}
              data-testid="story"
            >
              <i className="story-dot" aria-hidden />
              <div className="story-main">
                <div className="story-title">
                  {s.title}
                  {s.count > 1 ? <em className="mono">×{formatCount(s.count)}</em> : null}
                </div>
                <div className="story-meta mono">
                  {timeSpan(s)} · {who(s)}
                  {s.routine ? ` · ${plural(s.count, "line")} · nothing to worry about` : ""}
                </div>
                {s.detail ? <div className="story-detail">{s.detail}</div> : null}
                {open.has(s.key) ? (
                  <ul className="story-lines mono" aria-label="Raw lines">
                    {s.samples.map((e) => (
                      <li key={e.seq} data-me={isAppUnderTest(e) || undefined}>
                        <span className="story-line-time">{formatClock(e.ts_ms)}</span>
                        <span className="story-line-proc">{shortName(e.process)}</span>
                        <span className="story-line-msg">{e.message}</span>
                      </li>
                    ))}
                    {s.count > s.samples.length ? (
                      <li className="story-more">
                        … and {formatCount(s.count - s.samples.length)} more
                      </li>
                    ) : null}
                  </ul>
                ) : null}
              </div>
              <button
                type="button"
                className="story-raw"
                aria-expanded={open.has(s.key)}
                onClick={() => toggle(s.key)}
              >
                {open.has(s.key) ? "Hide raw" : "Raw"}
              </button>
            </div>
          ) : null
        }
      />
    </div>
  );
}
