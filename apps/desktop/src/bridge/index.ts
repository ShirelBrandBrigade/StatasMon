import { useAppStore } from "../store/app";
import { dispatch } from "./dispatch";
import { createReplayStream } from "./replayStream";
import { type DeviceStream, isTauri } from "./stream";

let current: DeviceStream | undefined;

/** The active stream (created on first use). */
export async function getStream(): Promise<DeviceStream> {
  if (current) return current;
  if (isTauri()) {
    const { createTauriStream } = await import("./tauriStream");
    current = createTauriStream();
  } else {
    const fixture = new URLSearchParams(window.location.search).get("fixture") ?? undefined;
    current = createReplayStream({ fixture });
  }
  useAppStore.getState().setStreamKind(current.kind);
  return current;
}

/** Connects the stream to the store; returns a disconnect function. */
export async function startBridge(): Promise<() => void> {
  const stream = await getStream();
  return stream.subscribe(dispatch);
}

export { dispatch } from "./dispatch";
export type { DeviceStream } from "./stream";
