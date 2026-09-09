import { formatValue, type ProcessRow, type Unit } from "@statasmon/protocol";
import { AppTile, Card } from "@statasmon/ui";
import { useCapability, useProcesses } from "../../hooks/useDevice";
import { DetailDash, ME_APP_ID, Subtitle } from "../../overview/cards/shared";
import { why } from "./why";

const BPS = { unit: "bits_per_sec" } as const satisfies { unit: Unit };

/** Rows with a network rate, busiest first; ties keep the process-table order. */
export function byTraffic(rows: readonly ProcessRow[]): Array<ProcessRow & { net_bps: number }> {
  return rows
    .filter((r): r is ProcessRow & { net_bps: number } => r.net_bps !== null)
    .sort((a, b) => b.net_bps - a.net_bps);
}

/** Per-process traffic: one row per process with a rate (app tile, name, pid, bar, mono rate). */
export function PerProcessCard({ deviceId }: { deviceId: string }) {
  const rows = useProcesses(deviceId);
  const capability = useCapability(deviceId, "network_per_process");
  const traffic = byTraffic(rows);
  const max = traffic.reduce((m, r) => Math.max(m, r.net_bps), 0);

  return (
    <Card
      title="Per-process traffic"
      className="net-card net-procs"
      data-testid="net-processes"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[traffic.length > 0 ? `${traffic.length} processes` : undefined]}
        />
      }
    >
      {traffic.length === 0 ? (
        <DetailDash label="Per process" reason={why(capability)} />
      ) : (
        <ul className="net-proc-list" aria-label="Per-process traffic">
          {traffic.map((r) => {
            const share = max > 0 ? (Math.max(0, r.net_bps) / max) * 100 : 0;
            return (
              <li key={r.pid} data-pid={r.pid} data-testid="net-proc-row">
                <AppTile me={r.app_id === ME_APP_ID} label={r.app_id ?? undefined} />
                <b title={r.app_id ?? r.name}>{r.name}</b>
                <span className="net-proc-pid mono">{r.pid}</span>
                <div className="net-proc-bar" aria-hidden>
                  <i style={{ width: `${share.toFixed(2)}%` }} />
                </div>
                <em className="mono">{formatValue(BPS, r.net_bps)}</em>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}
