import "../../overview/log/LogCard.css";
import "./apps.css";
import type { ProcessRow } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { Card, cx, Dash } from "@statasmon/ui";
import { useCallback, useMemo, useRef, useState } from "react";
import { useCapability, useLatest, useProcesses } from "../../hooks/useDevice";
import { TopBar } from "../../shell/TopBar";
import { useAppStore } from "../../store/app";
import { AppDetail } from "./AppDetail";
import { ProcessTable } from "./ProcessTable";
import { arrangeRows, DEFAULT_SORT, nextSort, type SortKey, type SortSpec } from "./processModel";

const COUNT = { unit: "count" } as const;

export interface AppsBodyProps {
  deviceId: string;
  /** Test hook passed to the detail's log list. */
  initialItemCount?: number;
}

/** Table + detail for one device; keyed by device so sort, search, pins and selection reset. */
export function AppsBody({ deviceId, initialItemCount }: AppsBodyProps) {
  const rows = useProcesses(deviceId);
  const count = useLatest(deviceId, "proc.count");
  const capability = useCapability(deviceId, "processes");
  const [sort, setSort] = useState<SortSpec>(DEFAULT_SORT);
  const [query, setQuery] = useState("");
  const [pinned, setPinned] = useState<ReadonlySet<number>>(() => new Set());
  const [selectedPid, setSelectedPid] = useState<number>();
  /** The last table row seen for the selection, so the panel survives the process exiting. */
  const lastSelected = useRef<ProcessRow>(undefined);

  const arranged = useMemo(
    () => arrangeRows(rows, { sort, query, pinned }),
    [rows, sort, query, pinned],
  );
  const withEnergy = useMemo(() => rows.some((r) => r.energy !== null), [rows]);

  const onSort = useCallback((key: SortKey) => setSort((s) => nextSort(s, key)), []);
  const togglePin = useCallback((pid: number) => {
    setPinned((prev) => {
      const next = new Set(prev);
      if (next.has(pid)) next.delete(pid);
      else next.add(pid);
      return next;
    });
  }, []);
  const select = useCallback(
    (pid: number) => setSelectedPid((cur) => (cur === pid ? undefined : pid)),
    [],
  );

  const live = selectedPid === undefined ? undefined : rows.find((r) => r.pid === selectedPid);
  if (live) lastSelected.current = live;
  const detailRow = selectedPid === undefined ? undefined : (live ?? lastSelected.current);
  const unavailable = capability.available === false;
  const reason = unavailable
    ? (capability.reason ?? "Not available on this device")
    : "No process table yet";

  return (
    <div className={cx("apps", !detailRow && "is-solo")} data-testid="apps-screen">
      <Card
        className="apps-card"
        title="Apps & processes"
        subtitle={
          count !== undefined ? `${formatValue(COUNT, count)} processes on the device` : undefined
        }
      >
        <div className="apps-tools" role="toolbar" aria-label="Process filters">
          <input
            className="apps-search"
            type="search"
            aria-label="Search processes"
            placeholder="Search name or app id"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setQuery("");
            }}
          />
          <span className="apps-count">
            <span className="mono">{formatValue(COUNT, arranged.length)}</span>
            {arranged.length !== rows.length ? (
              <>
                {" of "}
                <span className="mono">{formatValue(COUNT, rows.length)}</span>
              </>
            ) : null}{" "}
            rows{pinned.size > 0 ? ` · ${pinned.size} pinned` : ""}
          </span>
        </div>
        {rows.length === 0 ? (
          <div className="apps-empty">
            <span>Processes</span>
            <Dash reason={reason} />
            <span>{unavailable ? reason : "Waiting for the first process table"}</span>
          </div>
        ) : (
          <ProcessTable
            rows={arranged}
            sort={sort}
            onSort={onSort}
            pinned={pinned}
            onTogglePin={togglePin}
            selectedPid={selectedPid}
            onSelect={select}
            withEnergy={withEnergy}
          />
        )}
      </Card>
      {detailRow ? (
        <Card className="apps-card">
          <AppDetail
            key={selectedPid ?? "none"}
            deviceId={deviceId}
            row={detailRow}
            exited={!live}
            onClose={() => setSelectedPid(undefined)}
            initialItemCount={initialItemCount}
          />
        </Card>
      ) : null}
    </div>
  );
}

/** Apps & processes (docs/PLAN.md 6.3): the active device's process table with a per-app drill-in. */
export function AppsScreen({ deviceId: given }: { deviceId?: string }) {
  const active = useAppStore((s) => s.activeDeviceId);
  const deviceId = given ?? active;
  if (!deviceId) return null;
  return (
    <>
      <TopBar deviceId={deviceId} />
      <AppsBody key={deviceId} deviceId={deviceId} />
    </>
  );
}
