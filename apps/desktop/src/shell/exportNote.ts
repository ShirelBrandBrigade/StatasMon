import { useExportStore } from "../export/exportStore";

/**
 * Phase 1 showed an "Export arrives in Phase 5" note here. The same entry
 * points (TopBar button, ⌘/Ctrl+E in AppShell, the log card) now open the
 * Export dialog; these names stay so no caller changes.
 */
export function showExportNote(): void {
  useExportStore.getState().openDialog();
}

/** Test helper: closes the dialog. */
export function resetExportNote(): void {
  useExportStore.getState().close();
}

/** True while the Export dialog is open. */
export function useExportNote(): boolean {
  return useExportStore((s) => s.open);
}
