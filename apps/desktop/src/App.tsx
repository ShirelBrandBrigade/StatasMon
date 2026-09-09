import { useEffect, useState } from "react";
import { getStream, startBridge } from "./bridge";
import { dispatchedCount } from "./bridge/dispatch";
import { ExportDialog } from "./export/ExportDialog";
import { Gallery } from "./gallery/Gallery";
import {
  BatteryCard,
  CpuCard,
  MemoryCard,
  StorageCard,
  ThermalCard,
  TopAppsCard,
} from "./overview/cards";
import { LogCard } from "./overview/log";
import { NetworkCard } from "./overview/network";
import { Overview, type OverviewSlots } from "./overview/Overview";
import { AppsScreen } from "./screens/apps";
import { DeviceScreen } from "./screens/device";
import { LogsScreen } from "./screens/logs";
import { NetworkScreen } from "./screens/network";
import { PerformanceScreen } from "./screens/performance";
import { SettingsScreen } from "./screens/settings";
import { SessionsScreen } from "./sessions/SessionsScreen";
import { AppShell } from "./shell/AppShell";
import { ComingSoon } from "./shell/ComingSoon";
import { EmptyState } from "./shell/EmptyState";
import { viewFromHash } from "./shell/routes";
import { useAppStore } from "./store/app";
import { telemetry } from "./store/telemetry";

/** Real Overview cards are dropped in here as Phase 1 steps land (see `OverviewSlots`). */
const OVERVIEW_SLOTS: OverviewSlots = {
  cpu: CpuCard,
  memory: MemoryCard,
  storage: StorageCard,
  battery: BatteryCard,
  thermal: ThermalCard,
  network: NetworkCard,
  apps: TopAppsCard,
  log: LogCard,
};

function useHash(): string {
  const [hash, setHash] = useState(() => window.location.hash);
  useEffect(() => {
    const onHash = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return hash;
}

/** Starts the core bridge once and, inside Tauri, reports UI health after 4 s. */
function useBridge() {
  useEffect(() => {
    let stop: (() => void) | undefined;
    void startBridge().then((s) => {
      stop = s;
    });
    const report = setTimeout(() => {
      void getStream().then((st) => {
        if (st.kind !== "tauri") return;
        const devices = useAppStore.getState().devices;
        const lines = Object.values(devices).map((d) => {
          const v = d.latest;
          return `${d.summary.id} [${d.summary.state.state}] "${d.headline?.sentence ?? "-"}" cpu=${v["cpu.total"]?.toFixed(0) ?? "-"} mem=${v["mem.used"] ? (v["mem.used"] / 2 ** 30).toFixed(2) : "-"}GB bat=${v["bat.percent"] ?? "-"} thermal=${v["thermal.raw"] ?? "-"} rx=${v["net.rx_bps"]?.toFixed(0) ?? "-"} logs=${telemetry.logsFor(d.summary.id).size}`;
        });
        void st.invoke("ui_report", {
          message: `ui received ${dispatchedCount} messages; ${lines.join(" || ")}`,
        });
      });
    }, 12_000);
    return () => {
      clearTimeout(report);
      stop?.();
    };
  }, []);
}

export function App() {
  const hash = useHash();
  useBridge();
  const hasDevice = useAppStore((s) => s.activeDeviceId !== undefined);
  if (hash === "#gallery") return <Gallery />;
  const view = viewFromHash(hash);
  return (
    <AppShell view={view}>
      {view === "sessions" ? (
        <SessionsScreen />
      ) : view === "settings" ? (
        <SettingsScreen />
      ) : !hasDevice ? (
        <EmptyState />
      ) : view === "overview" ? (
        <Overview slots={OVERVIEW_SLOTS} />
      ) : view === "performance" ? (
        <PerformanceScreen />
      ) : view === "network" ? (
        <NetworkScreen />
      ) : view === "apps" ? (
        <AppsScreen />
      ) : view === "logs" ? (
        <LogsScreen />
      ) : view === "device" ? (
        <DeviceScreen />
      ) : (
        <ComingSoon view={view} />
      )}
      <ExportDialog />
    </AppShell>
  );
}
