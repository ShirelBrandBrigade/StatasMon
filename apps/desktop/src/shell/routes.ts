import type { IconName } from "@statasmon/ui";

/** Every view in the sidebar nav (docs/PLAN.md §3.12). Overview (Phase 1) and Sessions (Phase 5) are built; the rest arrive in Phase 6. */
export const VIEWS = [
  { id: "overview", label: "Overview", icon: "overview" },
  { id: "performance", label: "Performance", icon: "performance" },
  { id: "network", label: "Network", icon: "network" },
  { id: "apps", label: "Apps & processes", icon: "apps" },
  { id: "logs", label: "Logs", icon: "logs" },
  { id: "sessions", label: "Sessions", icon: "sessions" },
  { id: "device", label: "Device", icon: "device" },
  { id: "settings", label: "Settings", icon: "settings" },
] as const satisfies ReadonlyArray<{ id: string; label: string; icon: IconName }>;

export type ViewId = (typeof VIEWS)[number]["id"];

export const DEFAULT_VIEW: ViewId = "overview";

/** Maps a location hash (`#network`) to a view; unknown or empty hashes go to the Overview. */
export function viewFromHash(hash: string): ViewId {
  const id = hash.replace(/^#/, "");
  const hit = VIEWS.find((v) => v.id === id);
  return hit ? hit.id : DEFAULT_VIEW;
}

export function hashForView(view: ViewId): string {
  return `#${view}`;
}
