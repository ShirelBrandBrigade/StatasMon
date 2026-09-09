import "./perf.css";
import { useActiveDevice } from "../../hooks/useDevice";
import { TopBar } from "../../shell/TopBar";
import { BatteryPanel } from "./BatteryPanel";
import { CpuPanel } from "./CpuPanel";
import { MemoryPanel } from "./MemoryPanel";
import { ThermalPanel } from "./ThermalPanel";

/**
 * Performance (docs/PLAN.md 6.1): CPU per core with frequencies, memory
 * breakdown history, thermal history and battery history for the active
 * device, on the global range. `deviceId` overrides the active device (tests).
 */
export function PerformanceScreen({ deviceId: override }: { deviceId?: string }) {
  const active = useActiveDevice();
  const deviceId = override ?? active?.summary.id;
  if (!deviceId) return null;
  return (
    <>
      <TopBar deviceId={deviceId} />
      <div className="perf-grid" data-testid="performance-screen">
        <CpuPanel deviceId={deviceId} />
        <MemoryPanel deviceId={deviceId} />
        <ThermalPanel deviceId={deviceId} />
        <BatteryPanel deviceId={deviceId} />
      </div>
    </>
  );
}
