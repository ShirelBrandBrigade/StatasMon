import "./device.css";
import type { CapabilityEntry } from "@statasmon/protocol";
import { Button, Card, Dash, Icon, KeyValue, type KeyValueRow, Pill } from "@statasmon/ui";
import { useEffect, useState } from "react";
import { commands } from "../../bridge/commands";
import { useActiveDevice, useDevice } from "../../hooks/useDevice";
import { deviceIcon, transportLabel } from "../../shell/format";
import {
  capabilityLabel,
  connectionRows,
  hardwareRows,
  type InfoRow,
  identityRows,
  NOT_REPORTED,
  rawJson,
  softwareRows,
  sortedRaw,
  stateRows,
} from "./deviceModel";

/** How often the heartbeat age re-renders. */
const AGE_TICK_MS = 1000;
/** How long "Copied" / "Refreshed" stay visible. */
const STATUS_MS = 2000;

function useNow(tickMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), tickMs);
    return () => clearInterval(t);
  }, [tickMs]);
  return now;
}

function toRows(rows: InfoRow[]): KeyValueRow[] {
  return rows.map((r) => ({
    label: r.label,
    value: r.value === null ? <Dash reason={NOT_REPORTED} /> : r.value,
  }));
}

const PLATFORM_LABEL = { ios: "iOS", android: "Android", fake: "Fake" } as const;

function describeError(e: unknown): string {
  if (e && typeof e === "object" && typeof (e as { message?: unknown }).message === "string") {
    return (e as { message: string }).message;
  }
  return typeof e === "string" ? e : "Something went wrong";
}

/** One capability row: label left, `Available` pill or the honest reason right. */
function CapabilityRow({ entry }: { entry: CapabilityEntry }) {
  const a = entry.availability;
  return (
    <li
      className="dv-cap"
      data-capability={entry.capability}
      data-available={a.status === "available"}
    >
      <span className="dv-cap-name">{capabilityLabel(entry.capability)}</span>
      {a.status === "available" ? (
        <Pill tone="ok" size="sm">
          Available
        </Pill>
      ) : (
        <span className="dv-cap-reason">{a.reason}</span>
      )}
    </li>
  );
}

/**
 * Device sheet (docs/PLAN.md 6.5, docs/FEATURES.md §1): every DeviceInfo
 * field grouped, the capability list with reasons, and the raw platform
 * keys. Missing fields are dashes, never blanks or guesses.
 */
export function DeviceScreen({ deviceId }: { deviceId?: string }) {
  const active = useActiveDevice();
  const chosen = useDevice(deviceId);
  const entry = deviceId ? chosen : active;
  const now = useNow(AGE_TICK_MS);
  const [refreshing, setRefreshing] = useState(false);
  const [status, setStatus] = useState<{ text: string; tone: "ok" | "bad" }>();
  const [rawOpen, setRawOpen] = useState(false);

  useEffect(() => {
    if (!status) return undefined;
    const t = setTimeout(() => setStatus(undefined), STATUS_MS);
    return () => clearTimeout(t);
  }, [status]);

  if (!entry) {
    return (
      <div className="empty" data-testid="device-screen">
        <Card className="empty-card" aria-labelledby="device-empty-title">
          <h1 id="device-empty-title">No device selected</h1>
          <p>Pick a device in the list on the left to see its information sheet.</p>
        </Card>
      </div>
    );
  }

  const { summary, info, capabilities } = entry;
  const id = summary.id;
  const marketing = info?.identity.marketing_name ?? null;
  const raw = sortedRaw(info?.raw);
  const caps = capabilities?.entries ?? [];

  const refresh = async () => {
    setRefreshing(true);
    try {
      await commands.deviceAction(id, { action: "refresh_info" });
      setStatus({ text: "Refreshed", tone: "ok" });
    } catch (e) {
      setStatus({ text: describeError(e), tone: "bad" });
    } finally {
      setRefreshing(false);
    }
  };

  const copyRaw = async () => {
    try {
      await navigator.clipboard.writeText(rawJson(info?.raw));
      setStatus({ text: "Copied", tone: "ok" });
    } catch (e) {
      setStatus({ text: `Copy failed: ${describeError(e)}`, tone: "bad" });
    }
  };

  return (
    <div className="dv" data-testid="device-screen" data-device-id={id}>
      <header className="dv-head">
        <span className="dv-head-ic" aria-hidden>
          <Icon name={deviceIcon(summary)} size={22} />
        </span>
        <div className="dv-head-text">
          <h1>{summary.model_name}</h1>
          <p>
            {marketing && marketing !== summary.model_name ? <span>{marketing}</span> : null}
            <Pill tone="info" size="sm">
              {PLATFORM_LABEL[summary.platform]}
            </Pill>
            <span>{transportLabel(summary.transport)}</span>
            <code className="mono dv-id">{id}</code>
          </p>
        </div>
        <div className="dv-head-tools">
          {status ? (
            <span
              role="status"
              className={status.tone === "bad" ? "dv-status is-bad" : "dv-status"}
            >
              {status.text}
            </span>
          ) : null}
          <Button onClick={() => void refresh()} disabled={refreshing} aria-busy={refreshing}>
            {refreshing ? "Refreshing…" : "Refresh"}
          </Button>
        </div>
      </header>

      {!info ? (
        <p className="dv-pending" role="status">
          Waiting for device information…
        </p>
      ) : null}

      <div className="dv-grid">
        <Card title="Identity" aria-label="Identity">
          <KeyValue rows={toRows(identityRows(info))} />
        </Card>
        <Card title="Software" aria-label="Software">
          <KeyValue rows={toRows(softwareRows(info))} />
        </Card>
        <Card title="Hardware" aria-label="Hardware">
          <KeyValue rows={toRows(hardwareRows(info))} />
        </Card>
        <Card title="State" aria-label="State">
          <KeyValue rows={toRows(stateRows(info))} />
        </Card>
        <Card title="Connection" aria-label="Connection">
          <KeyValue rows={toRows(connectionRows(info, summary, now))} />
        </Card>
        <Card
          title="Capabilities"
          subtitle={
            caps.length > 0
              ? `${caps.filter((c) => c.availability.status === "available").length} of ${caps.length} available`
              : undefined
          }
          aria-label="Capabilities"
          className="dv-caps-card"
        >
          {caps.length === 0 ? (
            <p className="dv-pending m-0">Waiting for the capability list…</p>
          ) : (
            <ul className="dv-caps" aria-label="Capability list" data-count={caps.length}>
              {caps.map((c) => (
                <CapabilityRow key={c.capability} entry={c} />
              ))}
            </ul>
          )}
        </Card>
        <Card
          title="Raw"
          subtitle={`${raw.length} keys`}
          aria-label="Raw"
          className="dv-raw-card"
          footer={
            <>
              <Button size="sm" onClick={() => setRawOpen((o) => !o)} aria-expanded={rawOpen}>
                {rawOpen ? "Hide keys" : "Show keys"}
              </Button>
              <Button size="sm" onClick={() => void copyRaw()} disabled={raw.length === 0}>
                Copy as JSON
              </Button>
            </>
          }
        >
          {rawOpen ? (
            raw.length === 0 ? (
              <p className="dv-pending m-0">No raw keys reported.</p>
            ) : (
              <table className="dv-raw mono" aria-label="Raw keys">
                <thead>
                  <tr>
                    <th scope="col">Key</th>
                    <th scope="col">Value</th>
                  </tr>
                </thead>
                <tbody>
                  {raw.map(([k, v]) => (
                    <tr key={k}>
                      <th scope="row">{k}</th>
                      <td>{v}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          ) : (
            <p className="dv-pending m-0">
              Platform keys exactly as the device reported them, sorted by name.
            </p>
          )}
        </Card>
      </div>
    </div>
  );
}
