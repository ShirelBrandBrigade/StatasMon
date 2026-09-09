# Research: Android telemetry over ADB (Android 14–17, September 2026)

Compiled by a research agent from AOSP source and developer.android.com.
All commands are `adb shell …` as uid 2000 (`shell` SELinux domain). OEM
policies can add restrictions on top of AOSP; probe once at connect and mark
unreadable sources as unavailable.

## 1. Device information
| Item | Command / path | Caveat |
|---|---|---|
| Model / manufacturer / brand / codename | `getprop ro.product.model`, `ro.product.manufacturer`, `ro.product.brand`, `ro.product.device`, `ro.product.name` | stable |
| Android version / API / minor | `ro.build.version.release`, `ro.build.version.sdk`, `ro.build.version.sdk_minor` (Android 16+, feeds `SDK_INT_FULL`) | Android 17 = API 37 |
| Build id / fingerprint / patch | `ro.build.display.id`, `ro.build.id`, `ro.build.fingerprint`, `ro.build.version.security_patch` | |
| SoC | `ro.soc.manufacturer`, `ro.soc.model` (API 31+); fallbacks `ro.hardware`, `ro.board.platform`, `ro.hardware.chipname` | pre-12 lacks `ro.soc.*` |
| ABI / arch | `ro.product.cpu.abilist`, `abilist64`, `abilist32`; `uname -m` | |
| RAM | `/proc/meminfo` MemTotal | shell-readable |
| Serial | `getprop ro.serialno` or ADB transport serial | |
| Uptime / kernel | `/proc/uptime`, `/proc/version`, `uname -r` | |
| Dev options / debugging flags | `settings get global development_settings_enabled`, `adb_enabled`, `adb_wifi_enabled` | |
Extras: `ro.product.board`, `ro.bootloader`, `wm size`, `wm density`, `getenforce`.

## 2. CPU
| Item | Command / path | Caveat |
|---|---|---|
| Total / per-core utilization | `/proc/stat` deltas (`cpu`, `cpuN`) | offline cores vanish from the list; handle gaps |
| Per-core frequency | `/sys/devices/system/cpu/cpuN/cpufreq/scaling_cur_freq`, `cpuinfo_max_freq`, `stats/time_in_state`, `/sys/devices/system/cpu/online` | readable by every domain; `cpuinfo_cur_freq` often mode 0400, use `scaling_cur_freq`; read one core per `policyN` on big.LITTLE |
| Load average | `/proc/loadavg` | |
| Per-process | `/proc/<pid>/stat` | `/proc` is `hidepid=2,gid=readproc`; shell is in `readproc`, so all pids are visible |

## 3. Memory
| Item | Command / path | Caveat |
|---|---|---|
| System fields | `/proc/meminfo`, `/proc/vmstat`, `/proc/zoneinfo` | shell-readable |
| Totals and per-process PSS | `dumpsys meminfo` (Total RAM/Free/Used/Lost, PSS by process), `dumpsys meminfo -d <pkg|pid>` | walks every process's smaps: seconds on busy devices; cheap RSS via `ps -A -o PID,RSS,NAME` |
| PSI | `/proc/pressure/memory` | **not shell-readable** (`proc_pressure_mem` granted only to system_server/lmkd). Proxy: `/proc/vmstat` `workingset_refault_*`, `pgmajfault` |
| LMK kills | `logcat -b main -s lowmemorykiller` (`Kill '<proc>' (<pid>), uid …, oom_score_adj … to free …kB`); `logcat -b events -s am_kill,am_low_memory,killinfo` | |

## 4. Thermal
| Item | Command / path | Caveat |
|---|---|---|
| Status and per-sensor temps | `dumpsys thermalservice` → `Thermal Status: N` (0 NONE, 1 LIGHT, 2 MODERATE, 3 SEVERE, 4 CRITICAL, 5 EMERGENCY, 6 SHUTDOWN) and `Current temperatures from HAL:` `Temperature{mValue, mType, mName, mStatus}` | Android 14+ requires AIDL Thermal HAL; some OEMs expose only SKIN/BATTERY |
| Headroom | `cmd thermalservice headroom <secs>` | Android 11+ |
| Raw zones | `/sys/class/thermal/thermal_zone*/{type,temp}` | usually EACCES for shell on Pixel/vendor builds; best-effort probe |
| Battery temp | `dumpsys battery` `temperature:` (tenths of °C) | always |

## 5. Battery
| Item | Command / path | Caveat |
|---|---|---|
| Core fields | `dumpsys battery`: AC/USB/Wireless/Dock powered, Max charging current/voltage (µA/µV), Charge counter (µAh), status, health, present, level, scale, voltage (mV), temperature (0.1 °C), technology, charging policy, capacity level | no cycle count here |
| Current now | `/sys/class/power_supply/battery/current_now` (sign convention varies by vendor); newest builds `dumpsys battery get current_now` | |
| Cycle count / capacities | `/sys/class/power_supply/battery/{cycle_count,charge_full,charge_full_design,health}` (`sysfs_batteryinfo` shell-readable); `dumpsys android.hardware.health.IHealth/default` prints HealthInfo incl. cycle count | verify per device |

## 6. Storage
| Item | Command / path | Caveat |
|---|---|---|
| Volumes | `df -k /data /system /storage/emulated`; `sm list-volumes` | |
| Categories | `dumpsys diskstats`: Data/Cache/System free, App Size, App Data Size, App Cache Size, Photos/Videos/Audio/Downloads/System/Other Size, per-package arrays | values come from a periodic job; can be hours stale |
| Per app | `pm list packages -U -f`, APK size via `stat -c %s`; media via `content query --uri content://media/external/files --projection _size,mime_type` | private data sizes need root |

## 7. Network
| Item | Command / path | Caveat |
|---|---|---|
| Interface counters | `/proc/net/dev`; `ip -s link` | shell-readable |
| Per-UID traffic | `dumpsys netstats detail` (`--uid`, `--tag`, `--full`, `--poll` to force refresh) → rx/tx bytes and packets per uid | eBPF since Android 9; `/sys/fs/bpf` not readable; buckets refresh ~30 min unless polled |
| Wi-Fi | `cmd wifi status` → SSID, BSSID, RSSI, link speed, frequency, standard; `dumpsys wifi` verbose | SSID may be hidden when location is off |
| Capture | `tcpdump` not on user builds; needs root | **no packet capture on Android in v1**; mention PCAPdroid (VpnService) as a user-side option |

## 8. Processes
| Item | Command / path | Caveat |
|---|---|---|
| Snapshot | `top -b -n 1 -o PID,USER,%CPU,%MEM,RES,THREAD,NAME`; `ps -A -o PID,PPID,USER,%CPU,RSS,NAME` | `ps %CPU` is lifetime average; compute deltas from `/proc/<pid>/stat` |
| pid ↔ package | `pidof <pkg>`; `pm list packages -U`; `/proc/<pid>/cmdline` | multi-process apps (`:service`) |
| Foreground app | `dumpsys activity activities` (`topResumedActivity` / `ResumedActivity`); `dumpsys window` (`mCurrentFocus`, `mFocusedApp`) | strings changed in Android 14; parse both |

## 9. Logs
| Item | Command / path | Caveat |
|---|---|---|
| Structured stream | `logcat -v threadtime -v epoch -v uid -v usec -b main,system,crash,events -T 1` | shell has READ_LOGS |
| Filter | `--pid=<pid>`, `--uid=<uid>`, `-e <regex>`, `tag:prio *:S` | re-resolve pid on restart |
| Buffers | main, system, crash, events, radio, kernel, security, all | kernel/security empty for shell on user builds |
| Tombstones / ANR | dirs are 0775 system:system so shell is denied; use `adb bugreport out.zip` (30–120 s) or `dumpsys dropbox --print` (`data_app_crash`, `data_app_anr`) | |

## Sampling cadence and overhead
- Continuous streams: `logcat` with `-T` and `--pid`; one long-lived shell per stream.
- 1 s: `/proc/stat`, `/proc/loadavg`, `/proc/meminfo`, `/proc/net/dev`, cpufreq files, `/proc/<pid>/stat` for tracked pids. Batch into one `sh -c 'cat a; echo ---; cat b'` per tick.
- 2–5 s: `top -b -n 1` / `ps`, `dumpsys battery`, `cmd wifi status`.
- 10–30 s: `dumpsys thermalservice`, `dumpsys netstats detail`.
- 60 s+ or on demand: `dumpsys meminfo`, `dumpsys diskstats`, `dumpsys batterystats`; use `dumpsys -t 5` to bound hangs.
- Keep one persistent shell per cadence; `dumpsys` is a Binder round-trip into system_server holding service locks, never sub-second.

## Tooling and recommended approach
- **Licensing**: adb source is Apache-2.0, but the prebuilt platform-tools zip is under the Android SDK License, which forbids redistribution. Options: build adb from AOSP (redistributable), download platform-tools at first run after the user accepts the SDK terms (`https://dl.google.com/android/repository/platform-tools-latest-{windows,darwin}.zip`), or detect a user-installed adb (scrcpy does this). Latest adb 37.0.1 (Jul 2026).
- **Windows drivers**: Google USB Driver (WinUSB) covers Google devices only; other OEMs ship their own; also SDK-licensed.
- **Server**: all clients share the server on TCP 5037; reuse if present, else start our own with `-P <port>` / `ANDROID_ADB_SERVER_PORT` to avoid fighting Android Studio.
- **Wireless**: Android 11+: `adb pair ip:port` + code, then `adb connect`; discovery `adb mdns services`; Android 17 + adb 37 adds Wi-Fi 2.0 auto-reconnect; Android ≤10 only `adb tcpip 5555`.
- **Pure-language clients**: Rust `adb_client` 3.2.x (MIT, USB + TCP + server proxy + pairing + mDNS, most complete); `mozdevice` 0.5.x (server-only, Mozilla); `forensic-adb` (stale); Go `gadb`/`goadb` (stale).
- **Recommendation**: Rust server-protocol client (`adb_client` in server-proxy mode) talking to an adb binary we ship (built from AOSP or fetched on first run) and reusing a running server when found. Keep `adb_client` direct-USB mode as an optional fallback only.
- Dev machine note (2026-09-09): platform-tools 37.0.1 present at `~/Downloads/platform-tools`, daemon starts fine.
