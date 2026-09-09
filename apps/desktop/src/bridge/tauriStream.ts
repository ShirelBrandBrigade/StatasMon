import type { Message } from "@statasmon/protocol";
import { Channel, invoke } from "@tauri-apps/api/core";
import type { DeviceStream } from "./stream";

/** Tauri-backed stream: one `subscribe` command carrying a Channel. */
export function createTauriStream(): DeviceStream {
  return {
    kind: "tauri",
    subscribe(onMessage) {
      const channel = new Channel<Message>();
      channel.onmessage = onMessage;
      let active = true;
      void invoke("subscribe", { channel }).catch((e) => {
        if (active) console.error("subscribe failed", e);
      });
      return () => {
        active = false;
        void invoke("unsubscribe").catch(() => undefined);
      };
    },
    invoke<T>(command: string, args?: Record<string, unknown>) {
      return invoke<T>(command, args);
    },
  };
}
