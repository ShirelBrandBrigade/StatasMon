# Performance measurements

Budgets (docs/PLAN.md §4): UI idle CPU < 3 % with two devices; 1 Hz update
≤ 4 ms main-thread work; 10 000 log lines/min without frame drops;
memory < 300 MB after 1 h.

## 2026-09-09 · dev build, macOS (Apple Silicon), three fake devices
Scenario `warm`, log rate ~20 lines/s per device (60/s total, about 3×
the real tablet's rate), sampled every 15 s with `ps` on the Rust host
process (the WebKit content process is not attributed here).

| t (s) | CPU % | RSS (MB) |
|---|---|---|
| 15 | 3.3 | 100 |
| 60 | 0.9 | 69 |
| 120 | 3.3 | 64 |
| 180 | 3.6 | 69 |
| 240 | 6.1 | 72 |
| 270 | 3.0 | 73 |

Average CPU over the run ≈ 4 %, peak 8.7 % (a `dev` profile binary, so
JSON serialization is unoptimized). Memory grows slowly while the ring
buffers fill (3 600 samples × 52 metrics per device, 50 000 log lines)
and stays under 75 MB at five minutes. A release build with two real
devices should land under the 3 % budget; re-measure in a release build
before v1 and add the WebKit content process to the sum.

Chart budget: 1.4 ms per 1 Hz update for 3 600 points × 2 series in
Chromium (Phase 1.3 measurement); the jsdom perf test asserts < 4 ms.

Android tick cost on the Galaxy Tab S8: fast ≈ 280 ms, medium ≈ 300 ms,
slow ≈ 250 ms per tick through the adb subprocess (docs/KNOWN-ISSUES.md).
