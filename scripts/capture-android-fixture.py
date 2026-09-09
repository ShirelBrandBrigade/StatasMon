#!/usr/bin/env python3
"""Capture raw adb outputs from a connected Android device into fixtures/.

Usage: capture-android-fixture.py <adb path> <serial> <out dir>
Writes one file per command plus manifest.json with duration and exit code,
so provider parsers have real inputs and the plan's cadence table has real
costs. Nothing here is secret-free by construction: review before committing.
"""
import json, pathlib, subprocess, sys, time

adb, serial, out = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
out.mkdir(parents=True, exist_ok=True)
CMDS = {
    "getprop": "getprop",
    "proc_stat_1": "cat /proc/stat",
    "proc_stat_2": "sleep 1; cat /proc/stat",
    "proc_meminfo": "cat /proc/meminfo",
    "proc_vmstat": "cat /proc/vmstat",
    "proc_loadavg": "cat /proc/loadavg",
    "proc_uptime": "cat /proc/uptime",
    "proc_version": "cat /proc/version",
    "proc_net_dev_1": "cat /proc/net/dev",
    "proc_net_dev_2": "sleep 1; cat /proc/net/dev",
    "proc_pressure_memory": "cat /proc/pressure/memory",
    "cpu_online": "cat /sys/devices/system/cpu/online",
    "cpufreq": "for c in /sys/devices/system/cpu/cpu[0-9]*; do echo \"== $c\"; cat $c/cpufreq/scaling_cur_freq 2>&1; cat $c/cpufreq/cpuinfo_max_freq 2>&1; cat $c/cpufreq/cpuinfo_min_freq 2>&1; done",
    "cpufreq_policies": "for p in /sys/devices/system/cpu/cpufreq/policy*; do echo \"== $p\"; cat $p/related_cpus 2>&1; cat $p/scaling_cur_freq 2>&1; cat $p/cpuinfo_max_freq 2>&1; done",
    "uname": "uname -a; uname -m",
    "dumpsys_battery": "dumpsys battery",
    "power_supply": "for f in /sys/class/power_supply/battery/*; do printf '%s=' \"$(basename $f)\"; cat $f 2>&1 | head -c 200; echo; done",
    "power_supply_list": "ls /sys/class/power_supply/",
    "health_hal": "dumpsys android.hardware.health.IHealth/default",
    "dumpsys_thermalservice": "dumpsys thermalservice",
    "thermal_headroom": "cmd thermalservice headroom 10",
    "thermal_zones": "for z in /sys/class/thermal/thermal_zone*; do printf '%s ' \"$z\"; cat $z/type 2>&1 | tr -d '\\n'; printf ' '; cat $z/temp 2>&1; done",
    "cmd_wifi_status": "cmd wifi status",
    "proc_net_route": "cat /proc/net/route",
    "ip_addr": "ip -o addr",
    "ip_s_link": "ip -s link",
    "dumpsys_netstats_detail": "dumpsys netstats detail",
    "dumpsys_netstats_poll": "dumpsys netstats --poll detail",
    "df": "df -k",
    "sm_volumes": "sm list-volumes",
    "dumpsys_diskstats": "dumpsys diskstats",
    "pm_packages": "pm list packages -U -f",
    "top": "top -b -n 1 -o PID,USER,%CPU,%MEM,RES,THREAD,NAME",
    "ps": "ps -A -o PID,PPID,USER,%CPU,RSS,VSZ,NAME",
    "proc_pid_stat_sample": "for p in $(ls /proc | grep -E '^[0-9]+$' | head -40); do cat /proc/$p/stat 2>/dev/null; done",
    "proc_1_cmdline": "cat /proc/1/cmdline | tr '\\0' ' '",
    "dumpsys_activity_resumed": "dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity|mResumedActivity'",
    "dumpsys_window_focus": "dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'",
    "settings_global": "for k in development_settings_enabled adb_enabled adb_wifi_enabled; do printf '%s=' $k; settings get global $k; done",
    "wm": "wm size; wm density",
    "getenforce": "getenforce",
    "logcat_dump": "logcat -d -v threadtime -v epoch -v uid -v usec -b main,system,crash,events -t 3000",
    "logcat_dump_default": "logcat -d -v threadtime -t 500",
    "dumpsys_dropbox_list": "dumpsys dropbox",
    "dumpsys_dropbox_crash": "dumpsys dropbox --print data_app_crash | head -c 200000",
    "dumpsys_dropbox_anr": "dumpsys dropbox --print data_app_anr | head -c 100000",
    "dumpsys_meminfo": "dumpsys meminfo",
    "dumpsys_meminfo_pkg": "dumpsys meminfo -d com.android.systemui",
    "pidof_systemui": "pidof com.android.systemui",
    "id": "id",
}
manifest = {}
for name, cmd in CMDS.items():
    t0 = time.time()
    p = subprocess.run([adb, "-s", serial, "shell", cmd], capture_output=True, timeout=120)
    dt = round((time.time() - t0) * 1000)
    data = p.stdout + (b"\n[stderr]\n" + p.stderr if p.stderr else b"")
    (out / f"{name}.txt").write_bytes(data)
    manifest[name] = {"cmd": cmd, "exit": p.returncode, "ms": dt, "bytes": len(p.stdout)}
    print(f"{name:28s} exit={p.returncode} {dt:5d} ms {len(p.stdout):8d} B")
(out / "manifest.json").write_text(json.dumps(manifest, indent=2))
