import type { ExportFormat, ExportScope } from "@statasmon/protocol";
import { create } from "zustand";

/**
 * What the opener already knows: the TopBar opens for the active device, the
 * log card for its selection, a Sessions row for one stored session.
 */
export interface ExportPreset {
  deviceId?: string;
  /** Shown in the default file name when the device is not attached (stored sessions). */
  deviceName?: string;
  /** A stored session; `null`/undefined = the device's live session. */
  sessionId?: string | null;
  scope?: ExportScope["scope"];
  /** Log lines selected in the log card (`scope: "selection"`). */
  seqs?: number[];
  format?: ExportFormat;
}

interface ExportStore {
  open: boolean;
  preset: ExportPreset;
  openDialog(preset?: ExportPreset): void;
  close(): void;
}

export const useExportStore = create<ExportStore>((set) => ({
  open: false,
  preset: {},
  openDialog: (preset = {}) => set({ open: true, preset }),
  close: () => set({ open: false }),
}));

/** Opens the Export dialog (TopBar button, ⌘/Ctrl+E, the log card, a Sessions row). */
export function openExportDialog(preset?: ExportPreset): void {
  useExportStore.getState().openDialog(preset);
}

/**
 * DOM event other components can dispatch to open the dialog without importing
 * this module: `window.dispatchEvent(new CustomEvent(EXPORT_EVENT, { detail: preset }))`.
 */
export const EXPORT_EVENT = "statasmon:export";

/** Starts listening for `EXPORT_EVENT`; returns the cleanup. */
export function listenExportEvents(target: Window = window): () => void {
  const onEvent = (e: Event) => {
    const detail = (e as CustomEvent<ExportPreset | undefined>).detail;
    openExportDialog(detail && typeof detail === "object" ? detail : undefined);
  };
  target.addEventListener(EXPORT_EVENT, onEvent);
  return () => target.removeEventListener(EXPORT_EVENT, onEvent);
}
