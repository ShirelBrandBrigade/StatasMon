import type { CoreSample, DeviceHardware } from "@statasmon/protocol";
import { formatValue } from "@statasmon/protocol";
import { Card, type CoreBar, CoreBars } from "@statasmon/ui";
import { useCapability, useCores, useDevice, useLatest } from "../../hooks/useDevice";
import { capabilityReason, DetailDash, Gauge, LOAD, Subtitle, useMetric } from "./shared";

/** "6 cores · 2P + 4E" from device info, else from the per-core sample; undefined when unknown. */
function cpuSubtitle(hw: DeviceHardware | undefined, cores: CoreSample[]): string | undefined {
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

function toCoreBar(c: CoreSample): CoreBar {
  return {
    index: c.index,
    kind: c.kind,
    usagePct: c.online ? c.usage_pct : null,
    online: c.online,
  };
}

/** CPU: total utilisation ring, one bar per core, load average. */
export function CpuCard({ deviceId }: { deviceId: string }) {
  const total = useMetric(deviceId, "cpu.total", "cpu_total");
  const load1 = useLatest(deviceId, "cpu.load1");
  const perCore = useCapability(deviceId, "cpu_per_core");
  const cores = useCores(deviceId);
  const hardware = useDevice(deviceId)?.info?.hardware;
  const subtitle = cpuSubtitle(hardware, cores);

  return (
    <Card title="CPU" subtitle={<Subtitle deviceId={deviceId} parts={[subtitle]} />}>
      <div className="ov-ring-row">
        <Gauge value={total.value} reason={total.reason} name="CPU" series="cyan" />
        <div className="ov-detail">
          {cores.length > 0 ? (
            <CoreBars cores={cores.map(toCoreBar)} />
          ) : (
            <DetailDash label="Per core" reason={capabilityReason(perCore)} />
          )}
          {load1 !== undefined ? (
            <div className="ov-note mono" data-testid="cpu-load">
              load 1m {formatValue(LOAD, load1)}
            </div>
          ) : null}
        </div>
      </div>
    </Card>
  );
}
