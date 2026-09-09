import { formatValue, type SessionSummary } from "@statasmon/protocol";
import { Button, cx, Icon, Pill } from "@statasmon/ui";
import { useState } from "react";
import { formatCount, formatElapsed } from "../shell/format";
import { formatStart, platformIcon } from "./sessionsModel";

export interface SessionRowProps {
  session: SessionSummary;
  selected: boolean;
  onOpen: (id: string) => void;
  onExport: (session: SessionSummary) => void;
  onDelete: (id: string) => Promise<void> | void;
}

/**
 * One session: platform tile, device + model, start time, duration, live
 * badge, counts and size, then Open · Export · Delete (confirmed inline).
 */
export function SessionRow({ session, selected, onOpen, onExport, onDelete }: SessionRowProps) {
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const s = session;
  return (
    <li
      className={cx("sess-row", selected && "is-on")}
      data-testid="session-row"
      data-session-id={s.id}
      data-live={s.live ? "true" : "false"}
      aria-current={selected ? "true" : undefined}
    >
      <span className="sess-row-ic" aria-hidden>
        <Icon name={platformIcon(s.platform, s.model_name)} size={16} />
      </span>
      <div className="sess-row-body">
        <div className="sess-row-head">
          <b>{s.device_name}</b>
          <span>{s.model_name}</span>
          {s.live ? (
            <Pill tone="bad" size="sm">
              Live
            </Pill>
          ) : null}
        </div>
        <div className="sess-row-meta mono">
          <time dateTime={new Date(s.started_at_ms).toISOString()}>
            {formatStart(s.started_at_ms)}
          </time>
          <span>·</span>
          <span>{formatElapsed(s.elapsed_ms)}</span>
        </div>
        <div className="sess-row-meta mono">
          <span>{formatCount(s.sample_count)} samples</span>
          <span>·</span>
          <span>{formatCount(s.log_count)} logs</span>
          <span>·</span>
          <span>{formatCount(s.event_count)} events</span>
          <span>·</span>
          <span>{formatCount(s.mark_count)} marks</span>
          <span>·</span>
          <span>{formatValue({ unit: "bytes" }, s.size_bytes)}</span>
        </div>
        <div className="sess-row-actions">
          <Button size="sm" onClick={() => onOpen(s.id)} aria-pressed={selected}>
            Open
          </Button>
          <Button size="sm" icon="download" onClick={() => onExport(s)}>
            Export
          </Button>
          {confirming ? (
            <fieldset className="sess-confirm m-0 border-0 p-0" aria-label="Confirm delete">
              <span>Delete this session?</span>
              <Button
                size="sm"
                disabled={deleting}
                onClick={() => {
                  setDeleting(true);
                  void Promise.resolve(onDelete(s.id)).finally(() => {
                    setDeleting(false);
                    setConfirming(false);
                  });
                }}
              >
                {deleting ? "Deleting…" : "Yes, delete"}
              </Button>
              <Button size="sm" onClick={() => setConfirming(false)}>
                Keep
              </Button>
            </fieldset>
          ) : (
            <Button
              size="sm"
              disabled={s.live}
              title={s.live ? "Stop the recording before deleting it" : undefined}
              onClick={() => setConfirming(true)}
            >
              Delete
            </Button>
          )}
        </div>
      </div>
    </li>
  );
}
