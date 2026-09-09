import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { useAppStore } from "../../store/app";
import { NetworkCard } from "./NetworkCard";

/**
 * Mounts a NetworkCard for the active (or given) device into an overlay on
 * the running page, so the card can be reviewed before the Overview screen
 * routes it. From the devtools console of `pnpm web:mock` / `pnpm dev:mock`:
 *
 *   const m = await import("/src/overview/network/devMount.ts"); m.mountNetworkCard();
 *
 * Returns an unmount function.
 */
export function mountNetworkCard(deviceId?: string, width = 560): () => void {
  const id = deviceId ?? useAppStore.getState().activeDeviceId;
  if (!id) throw new Error("no device in the store yet");
  const host = document.createElement("div");
  host.className = "app-mesh";
  host.style.cssText = `position:fixed;left:24px;top:24px;width:${width}px;padding:14px;z-index:9999;border-radius:var(--r-card)`;
  document.body.appendChild(host);
  const root: Root = createRoot(host);
  root.render(createElement(NetworkCard, { deviceId: id }));
  return () => {
    root.unmount();
    host.remove();
  };
}
