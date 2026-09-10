import type { ProcessRow } from "@statasmon/protocol";
import { formatValue, shortName } from "@statasmon/protocol";
import { AppTile, cx, Dash, Pill } from "@statasmon/ui";
import { type CSSProperties, memo } from "react";
import { isAppUnderTest, maxCpu, type SortKey, type SortSpec } from "./processModel";

const PERCENT = { unit: "percent" } as const;
const BYTES = { unit: "bytes" } as const;
const BPS = { unit: "bytes_per_sec" } as const;
const COUNT = { unit: "count" } as const;

interface Column {
  key: SortKey;
  label: string;
  /** Right-aligned mono column. */
  numeric?: boolean;
}

const COLUMNS: readonly Column[] = [
  { key: "name", label: "App" },
  { key: "app_id", label: "App id" },
  { key: "pid", label: "PID", numeric: true },
  { key: "cpu_pct", label: "CPU", numeric: true },
  { key: "mem_bytes", label: "Memory", numeric: true },
  { key: "threads", label: "Threads", numeric: true },
  { key: "net_bps", label: "Network", numeric: true },
  { key: "state", label: "State" },
  { key: "energy", label: "Energy" },
];

export interface ProcessTableProps {
  rows: readonly ProcessRow[];
  sort: SortSpec;
  onSort(key: SortKey): void;
  pinned: ReadonlySet<number>;
  onTogglePin(pid: number): void;
  selectedPid: number | undefined;
  onSelect(pid: number): void;
  /** Show the energy column (any row reports it). */
  withEnergy: boolean;
}

function StateCell({ state }: { state: ProcessRow["state"] }) {
  if (state === "foreground") return <Pill tone="info">Foreground</Pill>;
  if (state === "background") return <Pill tone="neutral">Background</Pill>;
  return <Dash reason="State is not reported for this process" />;
}

interface RowProps {
  row: ProcessRow;
  max: number;
  pinned: boolean;
  selected: boolean;
  withEnergy: boolean;
  onTogglePin(pid: number): void;
  onSelect(pid: number): void;
}

const ProcessRowView = memo(function ProcessRowView({
  row,
  max,
  pinned,
  selected,
  withEnergy,
  onTogglePin,
  onSelect,
}: RowProps) {
  const me = isAppUnderTest(row);
  const share = row.cpu_pct !== null && max > 0 ? (Math.max(0, row.cpu_pct) / max) * 100 : 0;
  return (
    <tr
      className={cx("pt-row", me && "is-me", pinned && "is-pinned", selected && "is-selected")}
      aria-selected={selected}
      tabIndex={0}
      data-pid={row.pid}
      onClick={() => onSelect(row.pid)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onSelect(row.pid);
        }
      }}
    >
      <td className="pt-app">
        <AppTile me={me || pinned} label={row.app_id ?? undefined} />
        <b title={row.name}>{shortName(row.name)}</b>
      </td>
      <td className="pt-id mono">
        {row.app_id ?? <Dash reason="System process without an app id" />}
      </td>
      <td className="mono is-num">{row.pid}</td>
      <td className="pt-cpu mono is-num">
        <span className="pt-bar" aria-hidden>
          <i style={{ "--pt-share": `${share.toFixed(2)}%` } as CSSProperties} />
        </span>
        {row.cpu_pct !== null ? (
          formatValue(PERCENT, row.cpu_pct)
        ) : (
          <Dash reason="CPU is not reported for this process" />
        )}
      </td>
      <td className="mono is-num">
        {row.mem_bytes !== null ? (
          formatValue(BYTES, row.mem_bytes)
        ) : (
          <Dash reason="Memory is not reported for this process" />
        )}
      </td>
      <td className="mono is-num">
        {row.threads !== null ? (
          formatValue(COUNT, row.threads)
        ) : (
          <Dash reason="Thread count is not reported for this process" />
        )}
      </td>
      <td className="mono is-num">
        {row.net_bps !== null ? (
          formatValue(BPS, row.net_bps)
        ) : (
          <Dash reason="Per-process network is not reported on this platform" />
        )}
      </td>
      <td>
        <StateCell state={row.state} />
      </td>
      {withEnergy ? (
        <td>{row.energy ?? <Dash reason="Energy impact is not reported for this process" />}</td>
      ) : null}
      <td className="pt-pin">
        <button
          type="button"
          className="pt-pin-btn"
          aria-pressed={pinned}
          aria-label={pinned ? `Unpin ${row.name}` : `Pin ${row.name}`}
          title={pinned ? "Unpin from the top" : "Pin to the top"}
          onClick={(e) => {
            e.stopPropagation();
            onTogglePin(row.pid);
          }}
        >
          {pinned ? "Pinned" : "Pin"}
        </button>
      </td>
    </tr>
  );
});

/** Sortable process table; rows are selectable (click / Enter) and pinnable. */
export function ProcessTable({
  rows,
  sort,
  onSort,
  pinned,
  onTogglePin,
  selectedPid,
  onSelect,
  withEnergy,
}: ProcessTableProps) {
  const max = maxCpu(rows);
  const columns = withEnergy ? COLUMNS : COLUMNS.filter((c) => c.key !== "energy");
  return (
    <div className="pt-scroll">
      <table className="pt" aria-label="Processes" data-count={rows.length}>
        <thead>
          <tr>
            {columns.map((c) => {
              const active = sort.key === c.key;
              return (
                <th
                  key={c.key}
                  scope="col"
                  className={cx(c.numeric && "is-num")}
                  aria-sort={active ? (sort.dir === "asc" ? "ascending" : "descending") : "none"}
                >
                  <button
                    type="button"
                    className="pt-sort"
                    data-active={active ? "true" : undefined}
                    onClick={() => onSort(c.key)}
                  >
                    {c.label}
                    {active ? (
                      <span className="pt-sort-dir" aria-hidden>
                        {sort.dir === "asc" ? "↑" : "↓"}
                      </span>
                    ) : null}
                  </button>
                </th>
              );
            })}
            <th scope="col">
              <span className="pt-sort">Pin</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <ProcessRowView
              key={r.pid}
              row={r}
              max={max}
              pinned={pinned.has(r.pid)}
              selected={r.pid === selectedPid}
              withEnergy={withEnergy}
              onTogglePin={onTogglePin}
              onSelect={onSelect}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}
