import type { ProcessRow } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { AppTile, Card, Dash } from "@statasmon/ui";
import { useCapability, useLatest, useProcesses } from "../../hooks/useDevice";
import { COUNT, capabilityReason, DetailDash, ME_APP_ID, PERCENT, Subtitle } from "./shared";

export const TOP_APPS = 6;

/** Rows sorted by CPU (unknown CPU last), limited to `TOP_APPS`. */
export function topByCpu(rows: readonly ProcessRow[], limit = TOP_APPS): ProcessRow[] {
  return [...rows]
    .sort(
      (a, b) => (b.cpu_pct ?? Number.NEGATIVE_INFINITY) - (a.cpu_pct ?? Number.NEGATIVE_INFINITY),
    )
    .slice(0, limit);
}

/** Top apps: six rows with an app tile, name, CPU bar and percent. */
export function TopAppsCard({ deviceId }: { deviceId: string }) {
  const rows = useProcesses(deviceId);
  const count = useLatest(deviceId, "proc.count");
  const capability = useCapability(deviceId, "processes");
  const top = topByCpu(rows);
  const max = top.reduce((m, r) => Math.max(m, r.cpu_pct ?? 0), 0);

  return (
    <Card
      title="Top apps"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[
            "by CPU",
            count !== undefined ? `${formatValue(COUNT, count)} processes` : undefined,
          ]}
        />
      }
    >
      {top.length === 0 ? (
        <DetailDash label="Processes" reason={capabilityReason(capability)} />
      ) : (
        <ul className="ov-proc">
          {top.map((r) => {
            const share = r.cpu_pct !== null && max > 0 ? (Math.max(0, r.cpu_pct) / max) * 100 : 0;
            return (
              <li key={r.pid} data-pid={r.pid}>
                <AppTile me={r.app_id === ME_APP_ID} label={r.app_id ?? undefined} />
                <b>{r.name}</b>
                <div className="ov-bar" aria-hidden>
                  <i style={{ width: `${share.toFixed(2)}%` }} />
                </div>
                <em className="mono">
                  {r.cpu_pct !== null ? (
                    formatValue(PERCENT, r.cpu_pct)
                  ) : (
                    <Dash reason="CPU is not reported for this process" />
                  )}
                </em>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}
