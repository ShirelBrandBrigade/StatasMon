import type { DeviceSummary } from "@statasmon/protocol";
import { useSyncExternalStore } from "react";

/**
 * Devices the user unplugged this session through the fake device's
 * "Unplug" control (step 1.6). The core forgets a detached device entirely,
 * so the sidebar keeps this local list to offer "Reattach".
 */
let detached: ReadonlyArray<DeviceSummary> = [];
const listeners = new Set<() => void>();

function notify() {
  for (const l of listeners) l();
}

export function rememberDetached(summary: DeviceSummary): void {
  if (detached.some((d) => d.id === summary.id)) return;
  detached = [...detached, summary];
  notify();
}

export function forgetDetached(id: string): void {
  if (!detached.some((d) => d.id === id)) return;
  detached = detached.filter((d) => d.id !== id);
  notify();
}

/** Test helper. */
export function resetDetached(): void {
  detached = [];
  notify();
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function useDetached(): ReadonlyArray<DeviceSummary> {
  return useSyncExternalStore(subscribe, () => detached);
}
