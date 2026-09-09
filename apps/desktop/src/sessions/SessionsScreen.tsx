import "./sessions.css";
import { formatValue, type SessionDetail, type SessionSummary } from "@statasmon/protocol";
import { Card } from "@statasmon/ui";
import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, type StoreStats } from "../bridge/commands";
import { openExportDialog } from "../export/exportStore";
import { formatCount } from "../shell/format";
import { SessionDetailPanel } from "./SessionDetailPanel";
import { SessionRow } from "./SessionRow";
import { sortSessions } from "./sessionsModel";

/** The list re-reads the store this often (live rows keep growing). */
export const SESSIONS_REFRESH_MS = 10_000;

function describeError(e: unknown): string {
  if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return typeof e === "string" ? e : "Something went wrong";
}

/**
 * Sessions (docs/PLAN.md 5.5): every recorded session (live ones first among
 * the newest), reopen one read-only, delete, export, and compare two on one
 * metric. Store totals in the list footer.
 */
export function SessionsScreen() {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [stats, setStats] = useState<StoreStats | null>(null);
  const [selectedId, setSelectedId] = useState<string>();
  const [detail, setDetail] = useState<SessionDetail>();
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<string>();

  const refresh = useCallback(async () => {
    try {
      const [list, st] = await Promise.all([commands.listSessions(), commands.storeStats()]);
      setSessions(sortSessions(list ?? []));
      setStats(st ?? null);
      setError(undefined);
    } catch (e) {
      setError(describeError(e));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = () => {
      void refresh().finally(() => {
        if (!cancelled) timer = setTimeout(tick, SESSIONS_REFRESH_MS);
      });
    };
    tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [refresh]);

  const open = useCallback(async (id: string) => {
    setSelectedId(id);
    setOpening(true);
    try {
      const d = await commands.openSession(id);
      if (!d) throw new Error("The session could not be opened.");
      setDetail(d);
      setError(undefined);
    } catch (e) {
      setDetail(undefined);
      setError(describeError(e));
    } finally {
      setOpening(false);
    }
  }, []);

  const remove = useCallback(
    async (id: string) => {
      try {
        await commands.deleteSession(id);
        setSessions((list) => list.filter((s) => s.id !== id));
        if (selectedId === id) {
          setSelectedId(undefined);
          setDetail(undefined);
        }
        setError(undefined);
      } catch (e) {
        setError(describeError(e));
      }
      void refresh();
    },
    [refresh, selectedId],
  );

  const exportSession = useCallback((s: SessionSummary) => {
    openExportDialog({
      deviceId: s.device_id,
      deviceName: s.device_name,
      sessionId: s.live ? null : s.id,
      scope: "session",
    });
  }, []);

  const others = useMemo(
    () => sessions.filter((s) => s.id !== detail?.summary.id),
    [sessions, detail],
  );

  const footer = (
    <span className="mono" data-testid="store-stats">
      {stats
        ? `${formatCount(stats.session_count)} sessions · ${formatValue({ unit: "bytes" }, stats.total_bytes)} · ${stats.root}`
        : "Store totals unavailable"}
    </span>
  );

  return (
    <div className="sess" data-testid="sessions-screen">
      <Card
        title="Sessions"
        subtitle={`${formatCount(sessions.length)} recorded`}
        footer={footer}
        className="sess-list-card"
      >
        {error ? (
          <p role="alert" className="m-0 text-body text-red">
            {error}
          </p>
        ) : null}
        {loaded && sessions.length === 0 ? (
          <p className="sess-empty m-0">No sessions recorded yet.</p>
        ) : (
          <ul className="sess-list" aria-label="Sessions" data-count={sessions.length}>
            {sessions.map((s) => (
              <SessionRow
                key={s.id}
                session={s}
                selected={s.id === selectedId}
                onOpen={(id) => void open(id)}
                onExport={exportSession}
                onDelete={remove}
              />
            ))}
          </ul>
        )}
      </Card>
      <Card
        title="Session"
        subtitle={
          opening ? "opening…" : detail ? (detail.summary.live ? "recording" : "stored") : undefined
        }
        className="sess-detail-card"
      >
        {detail ? (
          <SessionDetailPanel detail={detail} others={others} onExport={exportSession} />
        ) : (
          <p className="sess-empty m-0">
            Open a session to see its device, marks, events and metrics.
          </p>
        )}
      </Card>
    </div>
  );
}
