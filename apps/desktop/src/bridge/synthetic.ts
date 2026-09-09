import type { LogEntry, Message, MetricId, MetricValue } from "@statasmon/protocol";

/**
 * Minimal built-in session used only when no fixture can be loaded in the
 * browser. Deterministic (seeded), three devices, one thermal rise and one
 * memory warning. The Rust fake device is the real generator; this exists so
 * the UI never renders empty.
 */
export function synthesize(durationMs: number): { at: number; message: Message }[] {
  const out: { at: number; message: Message }[] = [];
  const rng = mulberry32(7);
  const devices = [
    { id: "fake:iphone", platform: "ios" as const, name: "iPhone 16 Pro", model: "iPhone 16 Pro" },
    {
      id: "fake:ipad",
      platform: "ios" as const,
      name: "Studio iPad",
      model: "iPad (10th generation)",
    },
    { id: "fake:android", platform: "android" as const, name: "Galaxy Tab S8", model: "SM-X700" },
  ];
  const t0 = 1_757_419_200_000; // 2026-09-09T12:00:00Z
  for (const d of devices) {
    out.push({
      at: 0,
      message: {
        type: "device_attached",
        device: {
          id: d.id,
          platform: d.platform,
          name: d.name,
          model_name: d.model,
          transport: d.platform === "android" ? "adb_tcp" : "usb",
          state: { state: "tier2" },
        },
      },
    });
    out.push({
      at: 10,
      message: {
        type: "session",
        device_id: d.id,
        session: {
          id: `synthetic-${d.id}`,
          status: "recording",
          started_at_ms: t0,
          elapsed_ms: 0,
          sample_count: 0,
          log_count: 0,
          marks: [],
        },
      },
    });
    out.push({
      at: 20,
      message: {
        type: "headline",
        device_id: d.id,
        headline: {
          sentence: `${d.model} looks normal`,
          cause: "Synthetic session (no fixture found).",
          severity: "nominal",
          pills: [{ label: "Synthetic", tone: "info" }],
          rule: "none",
        },
      },
    });
  }
  let seq = 0;
  for (let t = 0; t < durationMs; t += 1000) {
    for (const d of devices) {
      const phase = t / durationMs;
      const cpu = 30 + 20 * Math.sin(phase * Math.PI * 4) + rng() * 8;
      const values: MetricValue[] = [
        v("cpu.total", cpu),
        v("mem.total", 8 * 2 ** 30),
        v("mem.used", 5.4 * 2 ** 30 + rng() * 2 ** 27),
        v("mem.free", 2.6 * 2 ** 30),
        v("thermal.state", t > durationMs * 0.6 ? 1 : 0),
        v("bat.percent", 87),
        v("bat.voltage", 4.21),
        v("bat.current", 1.12),
        v("bat.temp", 33.8),
        v("disk.total", 256 * 2 ** 30),
        v("disk.used", 197 * 2 ** 30),
        v("disk.free", 59 * 2 ** 30),
        v("net.rx_bps", 15e6 + 10e6 * Math.sin(phase * Math.PI * 6) + rng() * 2e6),
        v("net.tx_bps", 2e6 + rng() * 1.5e6),
        v("net.rx_pps", 1500 + rng() * 500),
        v("net.tx_pps", 500 + rng() * 200),
        v("proc.count", 142),
      ];
      out.push({ at: t, message: { type: "samples", device_id: d.id, ts_ms: t0 + t, values } });
      out.push({
        at: t,
        message: {
          type: "cores",
          device_id: d.id,
          ts_ms: t0 + t,
          cores: [0, 1, 2, 3, 4, 5].map((i) => ({
            index: i,
            kind: i < 2 ? "performance" : "efficiency",
            usage_pct: Math.max(0, Math.min(100, cpu * (i < 2 ? 1.6 : 0.7) + rng() * 10)),
            freq_mhz: null,
            online: true,
          })),
        },
      });
      const entries: LogEntry[] = [];
      for (let k = 0; k < 3; k += 1) {
        seq += 1;
        entries.push({
          seq,
          ts_ms: t0 + t + k * 300,
          pid: 4412,
          process: k === 1 ? "SpringBoard" : "FieldKit",
          tag: null,
          subsystem: k === 1 ? null : "com.example.fieldkit",
          category: null,
          buffer: null,
          level: k === 2 && rng() > 0.85 ? "error" : "info",
          message: k === 2 ? "Sync: 18 pending records → POST /v2/sync" : "Heartbeat ok",
        });
      }
      out.push({ at: t + 50, message: { type: "log", device_id: d.id, entries } });
    }
  }
  return out;
}

function v(id: MetricId, value: number): MetricValue {
  return { id, value };
}

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
