import { useEffect, useState } from "react";
import { getStream, startBridge } from "./bridge";
import { dispatchedCount } from "./bridge/dispatch";
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
import { AppShell } from "./shell/AppShell";
import { ComingSoon } from "./shell/ComingSoon";
import { EmptyState } from "./shell/EmptyState";
import { viewFromHash } from "./shell/routes";
import { useAppStore } from "./store/app";

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
        const ids = Object.keys(useAppStore.getState().devices);
        void st.invoke("ui_report", {
          message: `ui received ${dispatchedCount} messages; devices=${ids.join(",")}`,
        });
      });
    }, 4000);
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
      {!hasDevice ? (
        <EmptyState />
      ) : view === "overview" ? (
        <Overview slots={OVERVIEW_SLOTS} />
      ) : (
        <ComingSoon view={view} />
      )}
    </AppShell>
  );
}
