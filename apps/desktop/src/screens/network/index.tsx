import "./network.css";
import { useActiveDevice } from "../../hooks/useDevice";
import { TopBar } from "../../shell/TopBar";
import { CaptureCard } from "./CaptureCard";
import { InterfaceCard } from "./InterfaceCard";
import { PerProcessCard } from "./PerProcessCard";
import { ThroughputCard } from "./ThroughputCard";

/**
 * Network (docs/PLAN.md 6.2): throughput and packets history, interface /
 * Wi-Fi details, per-process traffic and packet capture for the active
 * device. `deviceId` overrides the active device (tests).
 */
export function NetworkScreen({ deviceId: override }: { deviceId?: string }) {
  const active = useActiveDevice();
  const deviceId = override ?? active?.summary.id;
  if (!deviceId) return null;
  return (
    <>
      <TopBar deviceId={deviceId} />
      <div className="net-grid" data-testid="network-screen">
        <ThroughputCard deviceId={deviceId} />
        <InterfaceCard deviceId={deviceId} />
        <PerProcessCard deviceId={deviceId} />
        <CaptureCard deviceId={deviceId} />
      </div>
    </>
  );
}
