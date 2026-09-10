import "./device.css";
import { type CapabilityEntry, formatValue } from "@statasmon/protocol";
import { Button, Card, Dash, Icon, KeyValue, type KeyValueRow, Pill } from "@statasmon/ui";
import { type ReactNode, useEffect, useState } from "react";
import { commands } from "../../bridge/commands";
import { useActiveDevice, useDevice } from "../../hooks/useDevice";
import { deviceIcon, transportLabel } from "../../shell/format";
import { TopBar } from "../../shell/TopBar";
import {
  capabilityLabel,
  connectionRows,
  formatUptime,
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
const BYTES = { unit: "bytes" } as const;

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

/** One figure of the spec strip: small label, mono value, optional detail line. */
function Spec({ label, value, sub }: { label: string; value: ReactNode; sub?: string | null }) {
  return (
    <div className="dv-spec">
      <span className="dv-spec-label">{label}</span>
      <span className="dv-spec-value mono">
        {value === null || value === undefined ? <Dash reason={NOT_REPORTED} /> : value}
      </span>
      {sub ? <span className="dv-spec-sub">{sub}</span> : null}
    </div>
  );
}

function coresLine(cores: number | null, p: number | null, e: number | null): string | null {
  if (cores === null) return null;
  return p !== null && e !== null ? `${cores} (${p} P + ${e} E)` : String(cores);
}

/** An available capability as a chip. */
function CapabilityChip({ entry }: { entry: CapabilityEntry }) {
  return (
    <li className="dv-chip" data-capability={entry.capability} data-available="true">
      <i aria-hidden />
      {capabilityLabel(entry.capability)}
    </li>
  );
}

/** A missing capability: its name and the honest reason. */
function CapabilityMissing({ entry }: { entry: CapabilityEntry }) {
  const reason = entry.availability.status === "available" ? "" : entry.availability.reason;
  return (
    <li className="dv-missing" data-capability={entry.capability} data-available="false">
      <span className="dv-missing-name">{capabilityLabel(entry.capability)}</span>
      <span className="dv-missing-reason">{reason}</span>
    </li>
  );
}

/**
 * Device sheet (docs/PLAN.md 6.5, docs/FEATURES.md §1): the shared header
 * (name, range, export), a hero card with the model and the headline specs,
 * grouped KeyValue cards, the capabilities as chips with reasons for the
 * missing ones, and the raw platform keys. Missing fields are dashes,
 * never blanks or guesses.
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
  const available = caps.filter((c) => c.availability.status === "available");
  const missing = caps.filter((c) => c.availability.status !== "available");
  const hw = info?.hardware;
  const sw = info?.software;

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
      <TopBar deviceId={id} />

      <Card className="dv-hero" data-testid="device-hero" aria-label="Device">
        <div className="dv-hero-head">
          <span className="dv-hero-ic" aria-hidden>
            <Icon name={deviceIcon(summary)} size={28} />
          </span>
          <div className="dv-hero-text">
            <h2>{summary.model_name}</h2>
            <p>
              {marketing && marketing !== summary.model_name ? <span>{marketing}</span> : null}
              <Pill tone="info" size="sm">
                {PLATFORM_LABEL[summary.platform]}
              </Pill>
              <span>{transportLabel(summary.transport)}</span>
              <code className="mono dv-id">{id}</code>
            </p>
          </div>
          <div className="dv-hero-tools">
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
        </div>
        {!info ? (
          <p className="dv-pending" role="status">
            Waiting for device information…
          </p>
        ) : null}
        <div className="dv-specs" data-testid="device-specs">
          <Spec
            label="System"
            value={sw?.os_version ? `${sw.os_name ? `${sw.os_name} ` : ""}${sw.os_version}` : null}
            sub={sw?.os_build ? `build ${sw.os_build}` : null}
          />
          <Spec label="Chip" value={hw?.chip_name ?? null} sub={hw?.architecture ?? null} />
          <Spec
            label="Cores"
            value={coresLine(
              hw?.cpu_cores ?? null,
              hw?.performance_cores ?? null,
              hw?.efficiency_cores ?? null,
            )}
          />
          <Spec
            label="Memory"
            value={hw?.ram_bytes != null ? formatValue(BYTES, hw.ram_bytes) : null}
          />
          <Spec
            label="Storage"
            value={hw?.storage_bytes != null ? formatValue(BYTES, hw.storage_bytes) : null}
          />
          <Spec
            label="Battery"
            value={
              hw?.battery_design_mah != null
                ? `${hw.battery_design_mah.toLocaleString("en-US").replace(/,/g, " ")} mAh`
                : null
            }
            sub={hw?.battery_design_mah != null ? "design capacity" : null}
          />
          <Spec
            label="Uptime"
            value={info?.state.uptime_secs != null ? formatUptime(info.state.uptime_secs) : null}
          />
        </div>
      </Card>

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
          subtitle={caps.length > 0 ? `${available.length} of ${caps.length} available` : undefined}
          aria-label="Capabilities"
          className="dv-caps-card"
          data-count={caps.length}
        >
          {caps.length === 0 ? (
            <p className="dv-pending m-0">Waiting for the capability list…</p>
          ) : (
            <>
              <ul className="dv-chips" aria-label="Available capabilities">
                {available.map((c) => (
                  <CapabilityChip key={c.capability} entry={c} />
                ))}
              </ul>
              {missing.length > 0 ? (
                <>
                  <p className="dv-caps-sub">Not on this device</p>
                  <ul className="dv-missing-list" aria-label="Unavailable capabilities">
                    {missing.map((c) => (
                      <CapabilityMissing key={c.capability} entry={c} />
                    ))}
                  </ul>
                </>
              ) : null}
            </>
          )}
        </Card>
      </div>
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
  );
}
