import { type ReactNode, useEffect } from "react";
import { commands } from "../bridge/commands";
import { useAppStore } from "../store/app";
import { showExportNote } from "./exportNote";
import { Notices } from "./Notices";
import type { ViewId } from "./routes";
import { Sidebar } from "./Sidebar";

function inTextInput(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/**
 * Sidebar + scrolling main column on the mesh ground, with the Notices stack
 * floating over the main column. Global keys: `M` adds a mark to the active
 * session, ⌘/Ctrl+E raises the Export note. `Space` is reserved for the log
 * card and is deliberately not bound here.
 */
export function AppShell({ view, children }: { view: ViewId; children: ReactNode }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const meta = e.metaKey || e.ctrlKey;
      if (meta && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "e") {
        e.preventDefault();
        showExportNote();
        return;
      }
      if (inTextInput(e.target)) return;
      if (!meta && !e.altKey && e.key.toLowerCase() === "m") {
        const s = useAppStore.getState();
        const id = s.activeDeviceId;
        const session = id ? s.devices[id]?.session : undefined;
        if (!id || !session || (session.status !== "recording" && session.status !== "paused"))
          return;
        e.preventDefault();
        void commands.mark(id, `Mark ${session.marks.length + 1}`);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app app-mesh">
      <Sidebar view={view} />
      <main className="main">{children}</main>
      <Notices />
    </div>
  );
}
