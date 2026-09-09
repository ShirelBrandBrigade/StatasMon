import type { Message } from "@statasmon/protocol";
import { useAppStore } from "../store/app";
import { telemetry } from "../store/telemetry";

export let dispatchedCount = 0;

/** Applies one core message to the React store and the mirror buffers. */
export function dispatch(m: Message): void {
  dispatchedCount += 1;
  const s = useAppStore.getState();
  switch (m.type) {
    case "device_attached":
      s.upsertDevice(m.device);
      break;
    case "device_detached":
      s.removeDevice(m.device_id);
      telemetry.forget(m.device_id);
      break;
    case "device_state":
      s.setState(m.device_id, m.state);
      break;
    case "capabilities":
      s.patchDevice(m.device_id, { capabilities: m.set });
      break;
    case "info":
      s.patchDevice(m.device_id, { info: m.info });
      break;
    case "headline":
      s.patchDevice(m.device_id, { headline: m.headline });
      break;
    case "session":
      s.patchDevice(m.device_id, { session: m.session });
      break;
    case "samples": {
      telemetry.metricsFor(m.device_id).push(m.ts_ms, m.values);
      const latest: Record<string, number> = {};
      for (const v of m.values) latest[v.id] = v.value;
      s.setLatest(m.device_id, m.ts_ms, latest);
      telemetry.notify(m.device_id, "samples");
      break;
    }
    case "cores":
      telemetry.cores.set(m.device_id, m.cores);
      telemetry.notify(m.device_id, "cores");
      break;
    case "processes":
      telemetry.processes.set(m.device_id, m.rows);
      telemetry.notify(m.device_id, "processes");
      break;
    case "log":
      telemetry.logsFor(m.device_id).push(m.entries);
      telemetry.notify(m.device_id, "log");
      break;
    case "event":
      s.pushEvent(m.device_id, {
        ts_ms: m.ts_ms,
        kind: m.kind,
        detail: m.detail,
        pid: m.pid,
        process: m.process,
      });
      break;
    case "error": {
      if (m.device_id) {
        const d = s.devices[m.device_id];
        if (d) s.patchDevice(m.device_id, { errorCount: d.errorCount + 1 });
      }
      s.pushNotice({
        code: m.code,
        message: m.message,
        hint: m.hint ?? undefined,
        deviceId: m.device_id ?? undefined,
      });
      console.warn(`[core] ${m.code}: ${m.message}${m.hint ? ` — ${m.hint}` : ""}`);
      break;
    }
    case "progress":
      s.setProgress(m.task_id, m.pct, m.message);
      break;
  }
}
