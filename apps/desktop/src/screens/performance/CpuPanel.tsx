import type { CoreSample, DeviceHardware } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { Card, type CoreBar, coreLabel, Dash, Pill, seriesColor } from "@statasmon/ui";
import { useMemo } from "react";
import { useCapability, useCores, useDevice, useLatest } from "../../hooks/useDevice";
import {
  capabilityReason,
  DetailDash,
  LOAD,
  PERCENT,
  Subtitle,
  useMetric,
} from "../../overview/cards/shared";
import { CHART_HEIGHT, type ChartMetric, MetricChart, useMarkers } from "./shared";

/** "6 cores · 2P + 4E" from device info, else from the per-core sample; undefined when unknown. */
export function cpuSubtitle(
  hw: DeviceHardware | undefined,
  cores: CoreSample[],
): string | undefined {
  const total = hw?.cpu_cores ?? (cores.length > 0 ? cores.length : undefined);
  if (total === undefined) return undefined;
  let perf = hw?.performance_cores ?? undefined;
  let eff = hw?.efficiency_cores ?? undefined;
  if ((perf === undefined || eff === undefined) && cores.length > 0) {
    perf ??= cores.filter((c) => c.kind === "performance").length;
    eff ??= cores.filter((c) => c.kind === "efficiency").length;
  }
  const kinds =
    perf !== undefined && eff !== undefined && perf + eff > 0 ? ` · ${perf}P + ${eff}E` : "";
  return `${total} cores${kinds}`;
}

/** Core frequency in the mono face; the protocol has no MHz unit yet (proposal in the report). */
export function formatMhz(mhz: number): string {
  return mhz >= 1000 ? `${(mhz / 1000).toFixed(2)} GHz` : `${Math.round(mhz)} MHz`;
}

const formatPct = (v: number) => formatValue(PERCENT, v);

function toBar(c: CoreSample): CoreBar {
  return {
    index: c.index,
    kind: c.kind,
    usagePct: c.online ? c.usage_pct : null,
    online: c.online,
  };
}

/** CPU: total/user/system chart, one row per core with usage and frequency, load averages. */
export function CpuPanel({ deviceId }: { deviceId: string }) {
  const total = useMetric(deviceId, "cpu.total", "cpu_total");
  const user = useLatest(deviceId, "cpu.user");
  const system = useLatest(deviceId, "cpu.system");
  const load1 = useLatest(deviceId, "cpu.load1");
  const load5 = useLatest(deviceId, "cpu.load5");
  const loadCap = useCapability(deviceId, "load_average");
  const perCore = useCapability(deviceId, "cpu_per_core");
  const freqCap = useCapability(deviceId, "cpu_frequency");
  const totalCap = useCapability(deviceId, "cpu_total");
  const cores = useCores(deviceId);
  const hardware = useDevice(deviceId)?.info?.hardware;
  const markers = useMarkers(deviceId);

  const hasUser = user !== undefined;
  const hasSystem = system !== undefined;
  const metrics = useMemo<ChartMetric[]>(() => {
    const list: ChartMetric[] = [{ id: "cpu.total", label: "Total", fill: true, endpoint: true }];
    if (hasUser) list.push({ id: "cpu.user", label: "User" });
    if (hasSystem) list.push({ id: "cpu.system", label: "System" });
    return list;
  }, [hasUser, hasSystem]);

  const sorted = [...cores].sort((a, b) => a.index - b.index);
  const bars = sorted.map(toBar);
  const freqReason = capabilityReason(freqCap);
  const loadText =
    load1 !== undefined || load5 !== undefined
      ? [
          load1 !== undefined ? `1m ${formatValue(LOAD, load1)}` : undefined,
          load5 !== undefined ? `5m ${formatValue(LOAD, load5)}` : undefined,
        ]
          .filter(Boolean)
          .join(" · ")
      : undefined;

  return (
    <Card
      title="CPU"
      className="perf-card"
      data-testid="perf-cpu"
      subtitle={
        <Subtitle
          deviceId={deviceId}
          parts={[
            total.value !== undefined ? formatValue(PERCENT, total.value) : undefined,
            cpuSubtitle(hardware, cores),
          ]}
        />
      }
    >
      <MetricChart
        deviceId={deviceId}
        metrics={metrics}
        height={CHART_HEIGHT}
        yFormat={formatPct}
        name="CPU utilisation over time"
        unit="percent"
        markers={markers}
        offReason={totalCap.available === false ? total.reason : undefined}
        testId="cpu-chart"
      />
      <div className="section-label">Per core</div>
      {sorted.length > 0 ? (
        <ul className="perf-cores" aria-label="Per core">
          {sorted.map((c, i) => {
            const pct = c.online ? Math.max(0, Math.min(100, c.usage_pct)) : 0;
            return (
              <li
                key={c.index}
                data-testid="core-row"
                data-kind={c.kind}
                data-online={c.online ? "true" : "false"}
              >
                <span className="perf-core-label mono" title={`${c.kind} core ${c.index}`}>
                  {coreLabel(bars[i], bars)}
                </span>
                <div className="perf-core-bar" aria-hidden>
                  <i
                    style={{
                      width: `${pct.toFixed(2)}%`,
                      background: seriesColor(c.kind === "efficiency" ? "cyan-2" : "cyan"),
                    }}
                  />
                </div>
                <span className="perf-core-pct mono">
                  {c.online ? (
                    formatValue(PERCENT, c.usage_pct)
                  ) : (
                    <Pill tone="neutral" size="sm">
                      offline
                    </Pill>
                  )}
                </span>
                <span className="perf-core-freq mono">
                  {c.freq_mhz !== null && c.online ? (
                    formatMhz(c.freq_mhz)
                  ) : (
                    <Dash reason={c.online ? freqReason : "Core is offline"} />
                  )}
                </span>
              </li>
            );
          })}
        </ul>
      ) : (
        <DetailDash label="Per core" reason={capabilityReason(perCore)} />
      )}
      {loadText !== undefined ? (
        <div className="perf-load mono" data-testid="cpu-load">
          load {loadText}
        </div>
      ) : (
        <DetailDash label="Load average" reason={capabilityReason(loadCap)} />
      )}
    </Card>
  );
}
