//! The value generator: smooth, realistic, seeded series for every metric,
//! per-core samples, a process table, log lines and scenario events.

use std::f64::consts::TAU;

use statasmon_device::allowed_metrics;
use statasmon_protocol::{
    CapabilitySet, CoreKind, CoreSample, EventKind, LogEntry, LogLevel, Message, MetricId,
    MetricValue, Platform, ProcessRow, ProcessState,
};

use crate::config::Scenario;
use crate::profiles::Profile;
use crate::rng::Rng;

/// Pid of the sample app (iOS-like devices).
const FIELDKIT_PID_IOS: u32 = 1842;
/// Pid of the sample app (Android-like devices).
const FIELDKIT_PID_ANDROID: u32 = 4123;

/// Seconds after start at which the warm scenario raises thermal state.
pub const WARM_THERMAL_AT: f64 = 20.0;
/// Seconds after start at which the warm scenario sends a memory warning.
pub const WARM_MEMORY_WARNING_AT: f64 = 35.0;
/// Seconds after start at which the crash scenario crashes FieldKit.
pub const CRASH_AT: f64 = 15.0;
/// Seconds after the crash at which FieldKit relaunches.
pub const RELAUNCH_AFTER: f64 = 5.0;

#[derive(Debug, Clone)]
struct ProcTemplate {
    pid: u32,
    name: &'static str,
    app_id: Option<&'static str>,
    base_cpu: f64,
    mem_bytes: u64,
    threads: u32,
    net_bps: f64,
    state: ProcessState,
    energy: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct LogTemplate {
    process: &'static str,
    level: LogLevel,
    /// iOS subsystem or Android tag.
    scope: &'static str,
    category: Option<&'static str>,
    message: &'static str,
    weight: u32,
}

/// Low-pass filtered noise so values drift instead of jumping.
#[derive(Debug, Clone)]
struct Drift {
    value: f64,
    amplitude: f64,
}

impl Drift {
    fn new(amplitude: f64) -> Self {
        Self {
            value: 0.0,
            amplitude,
        }
    }

    fn step(&mut self, rng: &mut Rng) -> f64 {
        let target = rng.range(-self.amplitude, self.amplitude);
        self.value = 0.85 * self.value + 0.15 * target;
        self.value
    }
}

/// Deterministic generator for one device.
#[derive(Debug, Clone)]
pub struct Simulator {
    profile: Profile,
    scenario: Scenario,
    rng: Rng,
    allowed: Vec<MetricId>,
    cpu_drift: Drift,
    net_drift: Drift,
    lat_drift: Drift,
    temp_drift: Drift,
    rx_total: f64,
    tx_total: f64,
    retransmits: f64,
    dropped: f64,
    thermal_state: u8,
    pressure: u8,
    warm_thermal_fired: bool,
    warm_memory_fired: bool,
    crash_fired: bool,
    relaunch_fired: bool,
    fieldkit_alive: bool,
    log_seq: u64,
    last_cpu: f64,
    procs: Vec<ProcTemplate>,
    logs: Vec<LogTemplate>,
}

impl Simulator {
    /// A generator for `profile` following `scenario`, seeded by `seed`.
    pub fn new(profile: Profile, scenario: Scenario, seed: u64) -> Self {
        let caps: CapabilitySet = profile.capabilities();
        let allowed = allowed_metrics(&caps);
        let (procs, logs) = match profile.imitates {
            Platform::Android => (android_processes(), android_logs()),
            Platform::Ios | Platform::Fake => (ios_processes(), ios_logs()),
        };
        let pressure = if scenario == Scenario::Stress { 2 } else { 0 };
        Self {
            profile,
            scenario,
            rng: Rng::new(seed),
            allowed,
            cpu_drift: Drift::new(6.0),
            net_drift: Drift::new(3.0e6),
            lat_drift: Drift::new(3.0),
            temp_drift: Drift::new(0.3),
            rx_total: 1_284_000_000.0,
            tx_total: 226_000_000.0,
            retransmits: 0.0,
            dropped: 0.0,
            thermal_state: 0,
            pressure,
            warm_thermal_fired: false,
            warm_memory_fired: false,
            crash_fired: false,
            relaunch_fired: false,
            fieldkit_alive: true,
            log_seq: 0,
            last_cpu: 40.0,
            procs,
            logs,
        }
    }

    /// The profile being simulated.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Metrics the capability set allows, in catalog order.
    pub fn allowed(&self) -> &[MetricId] {
        &self.allowed
    }

    fn fieldkit_pid(&self) -> u32 {
        match self.profile.imitates {
            Platform::Android => FIELDKIT_PID_ANDROID,
            Platform::Ios | Platform::Fake => FIELDKIT_PID_IOS,
        }
    }

    fn fieldkit_name(&self) -> &'static str {
        match self.profile.imitates {
            Platform::Android => "com.example.fieldkit",
            Platform::Ios | Platform::Fake => "FieldKit",
        }
    }

    fn cpu_total(&mut self, t: f64) -> f64 {
        let noise = self.cpu_drift.step(&mut self.rng);
        let v = match self.scenario {
            Scenario::Quiet => 30.0,
            Scenario::Stress => {
                (90.0 + 5.0 * (TAU * t / 23.0).sin() + noise * 0.6).clamp(80.0, 100.0)
            }
            Scenario::Warm | Scenario::Crash => {
                let base = 45.0 + 15.0 * (TAU * t / 40.0).sin() + 6.0 * (TAU * t / 7.0 + 1.0).sin();
                (base + noise).clamp(20.0, 70.0)
            }
        };
        self.last_cpu = v;
        v
    }

    /// One fast tick of scalar metrics. `t` is scenario seconds since
    /// start, `dt` the seconds since the previous tick (for counters).
    pub fn samples(&mut self, t: f64, dt: f64) -> Vec<MetricValue> {
        use MetricId as M;
        let quiet = self.scenario == Scenario::Quiet;
        let wave = |period: f64, phase: f64| {
            if quiet {
                0.0
            } else {
                (TAU * t / period + phase).sin()
            }
        };

        let cpu = self.cpu_total(t);
        let cpu_user = cpu * 0.68;
        let cpu_system = cpu - cpu_user;
        let cores = f64::from(self.profile.cores());
        let load1 = cpu / 100.0 * cores * 0.9;
        let load5 = load1 * 0.92;

        let total = self.profile.ram_bytes as f64;
        let used_frac = match self.scenario {
            Scenario::Stress => 0.86 + 0.01 * wave(60.0, 0.0),
            _ => 0.63 + 0.02 * wave(90.0, 0.0),
        };
        let used = total * used_frac;
        let is_android = self.profile.imitates == Platform::Android;
        let cached = total * if is_android { 0.14 } else { 0.11 };
        let swap = total * if is_android { 0.09 } else { 0.02 };
        let app = used * 0.58;
        let wired = used * 0.26;
        let compressed = used - app - wired;
        let free = (total - used - cached).max(0.0);

        let thermal = f64::from(self.thermal_state);
        let thermal_raw = if is_android {
            match self.thermal_state {
                0 => 0.0,
                1 => 1.0,
                2 => 3.0,
                _ => 5.0,
            }
        } else {
            thermal
        };
        let temp_noise = if quiet {
            0.0
        } else {
            self.temp_drift.step(&mut self.rng)
        };
        let bat_temp = 33.8 + 0.4 * wave(120.0, 0.5) + thermal * 2.5 + temp_noise;
        let soc_temp = 41.0 + cpu * 0.15 + thermal * 4.0 + temp_noise;

        let bat_pct = (87.0 + t / 600.0).min(100.0);
        let bat_volt = 4.21 + 0.005 * wave(45.0, 0.0);
        let bat_cur = if bat_pct >= 100.0 {
            0.05
        } else {
            1.12 + 0.03 * wave(30.0, 1.0)
        };
        let design = f64::from(self.profile.battery_design_mah);
        let full = design * 0.96;

        let disk_total = self.profile.storage_bytes as f64;
        let disk_apps = disk_total * 0.1875;
        let disk_media = disk_total * 0.4375;
        let disk_system = disk_total * 0.046_875;
        let disk_other = disk_total * 0.097_656_25;
        let disk_used = disk_apps + disk_media + disk_system + disk_other;
        let disk_write = if quiet {
            800_000.0
        } else {
            1_400_000.0 + 900_000.0 * wave(11.0, 0.0)
        };

        let net_noise = if quiet {
            0.0
        } else {
            self.net_drift.step(&mut self.rng)
        };
        let rx_bps = (17.5e6 + 9.0e6 * wave(25.0, 0.0) + net_noise).clamp(5.0e6, 30.0e6);
        let tx_bps = (2.5e6 + 1.2e6 * wave(19.0, 2.0) + net_noise * 0.2).clamp(1.0e6, 4.0e6);
        let rx_pps = rx_bps / 8.0 / 700.0;
        let tx_pps = tx_bps / 8.0 / 700.0;
        self.rx_total += rx_bps / 8.0 * dt;
        self.tx_total += tx_bps / 8.0 * dt;
        if !quiet {
            if self.rng.chance(0.05 * dt.max(0.01) * 10.0) {
                self.retransmits += 1.0;
            }
            if self.rng.chance(0.02 * dt.max(0.01) * 10.0) {
                self.dropped += 1.0;
            }
        }
        let connections = 27.0
            + if quiet {
                0.0
            } else {
                (2.0 * wave(50.0, 0.0)).round()
            };
        let lat_noise = if quiet {
            0.0
        } else {
            self.lat_drift.step(&mut self.rng)
        };
        let latency = (18.0 + 4.0 * wave(33.0, 0.0) + lat_noise).max(3.0);
        let rssi = -58.0 + 2.0 * wave(70.0, 0.0);
        let proc_count = 187.0
            + if quiet {
                0.0
            } else {
                (3.0 * wave(80.0, 0.0)).round()
            };

        let value = |id: MetricId| -> f64 {
            match id {
                M::CpuTotal => cpu,
                M::CpuUser => cpu_user,
                M::CpuSystem => cpu_system,
                M::CpuLoad1 => load1,
                M::CpuLoad5 => load5,
                M::MemTotal => total,
                M::MemUsed => used,
                M::MemFree => free,
                M::MemApp => app,
                M::MemWired => wired,
                M::MemCompressed => compressed,
                M::MemCached => cached,
                M::MemSwap => swap,
                M::MemPressure => f64::from(self.pressure),
                M::ThermalState => thermal,
                M::ThermalRaw => thermal_raw,
                M::TempBattery | M::BatTemp => bat_temp,
                M::TempSoc => soc_temp,
                M::TempSkin => 34.5 + thermal * 1.5 + temp_noise,
                M::TempPmu => 38.0 + cpu * 0.05 + temp_noise,
                M::TempDisplay => 33.0 + temp_noise,
                M::BatPercent => bat_pct,
                M::BatVoltage => bat_volt,
                M::BatCurrent => bat_cur,
                M::BatPower => bat_volt * bat_cur,
                M::BatCycles => 214.0,
                M::BatHealth => 96.0,
                M::BatCharging => 1.0,
                M::BatDesignCapacity => design,
                M::BatFullCapacity => full,
                M::DiskTotal => disk_total,
                M::DiskUsed => disk_used,
                M::DiskFree => disk_total - disk_used,
                M::DiskApps => disk_apps,
                M::DiskMedia => disk_media,
                M::DiskSystem => disk_system,
                M::DiskOther => disk_other,
                M::DiskWriteBps => disk_write,
                M::NetRxBps => rx_bps,
                M::NetTxBps => tx_bps,
                M::NetRxPps => rx_pps,
                M::NetTxPps => tx_pps,
                M::NetRxTotal => self.rx_total,
                M::NetTxTotal => self.tx_total,
                M::NetRssi => rssi,
                M::NetLinkMbps => 866.0,
                M::NetConnections => connections,
                M::NetRetransmits => self.retransmits,
                M::NetDropped => self.dropped,
                M::NetLatencyMs => latency,
                M::ProcCount => proc_count,
            }
        };
        self.allowed
            .iter()
            .map(|id| MetricValue {
                id: *id,
                value: round3(value(*id)),
            })
            .collect()
    }

    /// Per-core usage derived from the last total.
    pub fn cores(&mut self, t: f64) -> Vec<CoreSample> {
        let quiet = self.scenario == Scenario::Quiet;
        let total = self.last_cpu;
        let is_android = self.profile.imitates == Platform::Android;
        let n = self.profile.cores();
        (0..n)
            .map(|i| {
                let perf = i < self.profile.performance_cores;
                let kind = if perf {
                    CoreKind::Performance
                } else {
                    CoreKind::Efficiency
                };
                let factor = if perf { 1.25 } else { 0.85 };
                let wobble = if quiet {
                    0.0
                } else {
                    8.0 * (TAU * t / (9.0 + f64::from(i)) + f64::from(i)).sin()
                        + self.rng.range(-3.0, 3.0)
                };
                let usage = (total * factor + wobble).clamp(0.0, 100.0);
                let freq_mhz = is_android.then(|| {
                    let max = if perf { 2995.0 } else { 1785.0 };
                    let min = if perf { 787.0 } else { 300.0 };
                    (min + (max - min) * usage / 100.0).round() as u32
                });
                CoreSample {
                    index: i,
                    kind,
                    usage_pct: round3(usage) as f32,
                    freq_mhz,
                    online: true,
                }
            })
            .collect()
    }

    /// The process table (about twelve rows).
    pub fn processes(&mut self) -> Vec<ProcessRow> {
        let quiet = self.scenario == Scenario::Quiet;
        let scale = self.last_cpu / 45.0;
        let fk_pid = self.fieldkit_pid();
        let alive = self.fieldkit_alive;
        let templates = self.procs.clone();
        templates
            .iter()
            .filter(|p| alive || p.pid != fk_pid)
            .map(|p| {
                let jitter = if quiet {
                    0.0
                } else {
                    self.rng.range(-1.5, 1.5)
                };
                let cpu = (p.base_cpu * scale + jitter).clamp(0.0, 100.0);
                let mem_jitter = if quiet {
                    0.0
                } else {
                    self.rng.range(-0.02, 0.02)
                };
                ProcessRow {
                    pid: p.pid,
                    name: p.name.to_owned(),
                    app_id: p.app_id.map(str::to_owned),
                    cpu_pct: Some(round3(cpu) as f32),
                    mem_bytes: Some((p.mem_bytes as f64 * (1.0 + mem_jitter)) as u64),
                    threads: Some(p.threads),
                    net_bps: Some(p.net_bps * scale),
                    state: p.state,
                    energy: p.energy.map(str::to_owned),
                }
            })
            .collect()
    }

    /// `n` log lines ending at `ts_ms`, spread over `span_ms`.
    pub fn log_lines(&mut self, ts_ms: i64, span_ms: i64, n: usize) -> Vec<LogEntry> {
        let fk = self.fieldkit_name();
        let alive = self.fieldkit_alive;
        let total_weight: u32 = self
            .logs
            .iter()
            .filter(|l| alive || l.process != fk)
            .map(|l| l.weight)
            .sum();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let mut pick = self.rng.below(total_weight.max(1) as usize) as u32;
            let mut chosen = None;
            for tpl in self.logs.iter().filter(|l| alive || l.process != fk) {
                if pick < tpl.weight {
                    chosen = Some(tpl.clone());
                    break;
                }
                pick -= tpl.weight;
            }
            let Some(tpl) = chosen else { break };
            let offset = span_ms * (n - i) as i64 / n.max(1) as i64;
            let entry = self.entry(ts_ms - offset, &tpl);
            out.push(entry);
        }
        out
    }

    fn pid_of(&self, process: &str) -> Option<u32> {
        self.procs.iter().find(|p| p.name == process).map(|p| p.pid)
    }

    fn entry(&mut self, ts_ms: i64, tpl: &LogTemplate) -> LogEntry {
        self.log_seq += 1;
        let pid = self.pid_of(tpl.process);
        match self.profile.imitates {
            Platform::Android => LogEntry {
                seq: self.log_seq,
                ts_ms,
                pid,
                process: tpl.process.to_owned(),
                tag: Some(tpl.scope.to_owned()),
                subsystem: None,
                category: None,
                buffer: Some(tpl.category.unwrap_or("main").to_owned()),
                level: tpl.level,
                message: tpl.message.to_owned(),
            },
            Platform::Ios | Platform::Fake => LogEntry {
                seq: self.log_seq,
                ts_ms,
                pid,
                process: tpl.process.to_owned(),
                tag: None,
                subsystem: Some(tpl.scope.to_owned()),
                category: tpl.category.map(str::to_owned),
                buffer: None,
                level: tpl.level,
                message: tpl.message.to_owned(),
            },
        }
    }

    fn custom_line(
        &mut self,
        ts_ms: i64,
        process: &'static str,
        level: LogLevel,
        scope: &'static str,
        category: Option<&'static str>,
        message: String,
    ) -> LogEntry {
        let tpl = LogTemplate {
            process,
            level,
            scope,
            category,
            message: "",
            weight: 0,
        };
        let mut entry = self.entry(ts_ms, &tpl);
        entry.message = message;
        entry
    }

    /// Scenario steps that fire at scenario time `t` (seconds since start).
    /// Returns the events and log lines to publish, in order.
    pub fn scenario_step(&mut self, t: f64, ts_ms: i64) -> Vec<Message> {
        let id = self.profile.id.clone();
        let is_android = self.profile.imitates == Platform::Android;
        let mut out = Vec::new();
        match self.scenario {
            Scenario::Warm => {
                if !self.warm_thermal_fired && t >= WARM_THERMAL_AT {
                    self.warm_thermal_fired = true;
                    self.thermal_state = 1;
                    let line = if is_android {
                        self.custom_line(
                            ts_ms,
                            "system_server",
                            LogLevel::Info,
                            "ThermalManagerService",
                            Some("system"),
                            "Thermal status changed: 0 (NONE) -> 1 (LIGHT)".into(),
                        )
                    } else {
                        self.custom_line(
                            ts_ms,
                            "thermalmonitord",
                            LogLevel::Notice,
                            "com.apple.thermalmonitor",
                            Some("state"),
                            "Thermal pressure state: 1 (Fair), previous 0 (Nominal)".into(),
                        )
                    };
                    out.push(Message::Event {
                        device_id: id.clone(),
                        ts_ms,
                        kind: EventKind::ThermalChange,
                        detail: "Nominal → Fair".into(),
                        pid: line.pid,
                        process: Some(line.process.clone()),
                    });
                    out.push(Message::Log {
                        device_id: id.clone(),
                        entries: vec![line],
                    });
                }
                if !self.warm_memory_fired && t >= WARM_MEMORY_WARNING_AT {
                    self.warm_memory_fired = true;
                    self.pressure = 1;
                    let fk = self.fieldkit_name();
                    let pid = self.fieldkit_pid();
                    let line = if is_android {
                        self.custom_line(
                            ts_ms,
                            "system_server",
                            LogLevel::Fault,
                            "ActivityManager",
                            Some("system"),
                            format!("Low on memory: trimming {fk} (pid {pid}) to TRIM_MEMORY_RUNNING_LOW"),
                        )
                    } else {
                        self.custom_line(
                            ts_ms,
                            "FieldKit",
                            LogLevel::Fault,
                            "com.example.fieldkit",
                            Some("memory"),
                            format!("didReceiveMemoryWarning — footprint 612 MB, pid {pid}"),
                        )
                    };
                    out.push(Message::Event {
                        device_id: id.clone(),
                        ts_ms,
                        kind: EventKind::MemoryWarning,
                        detail: format!("Memory warning delivered to {fk}"),
                        pid: Some(pid),
                        process: Some(fk.to_owned()),
                    });
                    out.push(Message::Log {
                        device_id: id,
                        entries: vec![line],
                    });
                }
            }
            Scenario::Crash => {
                if !self.crash_fired && t >= CRASH_AT {
                    self.crash_fired = true;
                    self.fieldkit_alive = false;
                    let fk = self.fieldkit_name();
                    let pid = self.fieldkit_pid();
                    let (detail, line) = if is_android {
                        (
                            format!("{fk} crashed: java.lang.NullPointerException in SyncWorker.run"),
                            self.custom_line(
                                ts_ms,
                                "com.example.fieldkit",
                                LogLevel::Fault,
                                "AndroidRuntime",
                                Some("crash"),
                                format!("FATAL EXCEPTION: main\nProcess: {fk}, PID: {pid}\njava.lang.NullPointerException: Attempt to invoke virtual method 'int java.util.List.size()' on a null object reference\n\tat com.example.fieldkit.sync.SyncWorker.run(SyncWorker.kt:88)"),
                            ),
                        )
                    } else {
                        (
                            format!("{fk} crashed: EXC_BAD_ACCESS (SIGSEGV) at 0x0000000000000010"),
                            self.custom_line(
                                ts_ms,
                                "ReportCrash",
                                LogLevel::Fault,
                                "com.apple.ReportCrash",
                                Some("crash"),
                                format!("Saving crash report for {fk} [{pid}] version 3.2.1 (412) to FieldKit-2026-09-09-101512.ips"),
                            ),
                        )
                    };
                    out.push(Message::Event {
                        device_id: id.clone(),
                        ts_ms,
                        kind: EventKind::Crash,
                        detail,
                        pid: Some(pid),
                        process: Some(fk.to_owned()),
                    });
                    out.push(Message::Log {
                        device_id: id.clone(),
                        entries: vec![line],
                    });
                }
                if self.crash_fired && !self.relaunch_fired && t >= CRASH_AT + RELAUNCH_AFTER {
                    self.relaunch_fired = true;
                    self.fieldkit_alive = true;
                    let fk = self.fieldkit_name();
                    out.push(Message::Event {
                        device_id: id,
                        ts_ms,
                        kind: EventKind::AppLaunch,
                        detail: format!("{fk} launched"),
                        pid: Some(self.fieldkit_pid()),
                        process: Some(fk.to_owned()),
                    });
                }
            }
            Scenario::Quiet | Scenario::Stress => {}
        }
        out
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn ios_processes() -> Vec<ProcTemplate> {
    use ProcessState as S;
    let p =
        |pid, name, app_id, base_cpu, mem_mb: u64, threads, net_bps, state, energy| ProcTemplate {
            pid,
            name,
            app_id,
            base_cpu,
            mem_bytes: mem_mb * 1024 * 1024,
            threads,
            net_bps,
            state,
            energy,
        };
    vec![
        p(
            FIELDKIT_PID_IOS,
            "FieldKit",
            Some("com.example.fieldkit"),
            18.0,
            412,
            31,
            2.1e6,
            S::Foreground,
            Some("High"),
        ),
        p(
            1207,
            "Safari",
            Some("com.apple.mobilesafari"),
            4.5,
            268,
            22,
            0.6e6,
            S::Background,
            Some("Low"),
        ),
        p(
            0,
            "kernel_task",
            None,
            6.0,
            1180,
            240,
            0.0,
            S::Unknown,
            None,
        ),
        p(
            84,
            "mediaserverd",
            None,
            3.2,
            96,
            28,
            0.0,
            S::Background,
            Some("Low"),
        ),
        p(
            61,
            "SpringBoard",
            Some("com.apple.springboard"),
            2.8,
            210,
            40,
            0.0,
            S::Background,
            Some("Low"),
        ),
        p(
            58,
            "backboardd",
            None,
            2.1,
            48,
            18,
            0.0,
            S::Background,
            None,
        ),
        p(
            97,
            "locationd",
            None,
            1.2,
            36,
            14,
            0.05e6,
            S::Background,
            Some("Low"),
        ),
        p(110, "wifid", None, 0.8, 22, 9, 0.0, S::Background, None),
        p(
            103,
            "bluetoothd",
            None,
            0.6,
            26,
            12,
            0.0,
            S::Background,
            None,
        ),
        p(72, "powerd", None, 0.3, 14, 6, 0.0, S::Background, None),
        p(
            76,
            "thermalmonitord",
            None,
            0.2,
            9,
            4,
            0.0,
            S::Background,
            None,
        ),
        p(131, "dasd", None, 0.4, 18, 7, 0.0, S::Background, None),
    ]
}

fn android_processes() -> Vec<ProcTemplate> {
    use ProcessState as S;
    let p = |pid, name, app_id, base_cpu, mem_mb: u64, threads, net_bps, state| ProcTemplate {
        pid,
        name,
        app_id,
        base_cpu,
        mem_bytes: mem_mb * 1024 * 1024,
        threads,
        net_bps,
        state,
        energy: None,
    };
    vec![
        p(
            FIELDKIT_PID_ANDROID,
            "com.example.fieldkit",
            Some("com.example.fieldkit"),
            17.0,
            388,
            46,
            2.0e6,
            S::Foreground,
        ),
        p(
            1620,
            "com.android.systemui",
            Some("com.android.systemui"),
            3.4,
            246,
            38,
            0.0,
            S::Background,
        ),
        p(
            1188,
            "system_server",
            None,
            5.5,
            402,
            190,
            0.1e6,
            S::Background,
        ),
        p(
            724,
            "surfaceflinger",
            None,
            4.1,
            118,
            24,
            0.0,
            S::Background,
        ),
        p(
            2311,
            "com.sec.android.app.launcher",
            Some("com.sec.android.app.launcher"),
            1.9,
            172,
            35,
            0.0,
            S::Background,
        ),
        p(
            2899,
            "com.google.android.gms",
            Some("com.google.android.gms"),
            2.2,
            214,
            74,
            0.3e6,
            S::Background,
        ),
        p(
            702,
            "android.hardware.graphics.composer@2.4-service",
            None,
            1.1,
            24,
            6,
            0.0,
            S::Background,
        ),
        p(611, "logd", None, 0.9, 18, 8, 0.0, S::Background),
        p(1044, "adbd", None, 0.7, 12, 5, 0.02e6, S::Background),
        p(
            3120,
            "com.samsung.android.bixby.agent",
            Some("com.samsung.android.bixby.agent"),
            0.5,
            96,
            21,
            0.0,
            S::Background,
        ),
        p(690, "netd", None, 0.6, 16, 9, 0.0, S::Background),
        p(1002, "zygote64", None, 0.1, 64, 4, 0.0, S::Background),
    ]
}

fn ios_logs() -> Vec<LogTemplate> {
    use LogLevel as L;
    let t = |process, level, scope, category, message, weight| LogTemplate {
        process,
        level,
        scope,
        category: Some(category),
        message,
        weight,
    };
    vec![
        t(
            "FieldKit",
            L::Debug,
            "com.example.fieldkit",
            "sync",
            "Sync: 18 pending records → POST /v2/sync",
            14,
        ),
        t(
            "FieldKit",
            L::Info,
            "com.example.fieldkit",
            "sync",
            "Sync completed in 412 ms (18 records, 2 conflicts resolved)",
            8,
        ),
        t(
            "FieldKit",
            L::Debug,
            "com.example.fieldkit",
            "ui",
            "FormViewController viewDidAppear (form 'Site inspection')",
            10,
        ),
        t(
            "FieldKit",
            L::Notice,
            "com.example.fieldkit",
            "location",
            "Location update: accuracy 4.8 m, speed 0.3 m/s",
            6,
        ),
        t(
            "FieldKit",
            L::Error,
            "com.example.fieldkit",
            "network",
            "URLSession task failed: NSURLErrorTimedOut (-1001)",
            3,
        ),
        t(
            "FieldKit",
            L::Debug,
            "com.example.fieldkit",
            "db",
            "SQLite checkpoint: 3 pages, 2 ms",
            6,
        ),
        t(
            "FieldKit",
            L::Info,
            "com.example.fieldkit",
            "photos",
            "Compressed photo 4032×3024 → 1.9 MB (q=0.82)",
            4,
        ),
        t(
            "Safari",
            L::Debug,
            "com.apple.WebKit",
            "Loading",
            "Resource load completed: 200 text/html (38 KB)",
            6,
        ),
        t(
            "SpringBoard",
            L::Notice,
            "com.apple.SpringBoard",
            "Scene",
            "Scene activation: com.example.fieldkit → foreground",
            2,
        ),
        t(
            "locationd",
            L::Info,
            "com.apple.locationd",
            "Core",
            "Client com.example.fieldkit authorized WhenInUse",
            3,
        ),
        t(
            "wifid",
            L::Notice,
            "com.apple.wifi",
            "Roaming",
            "RSSI -58 dBm, link 866 Mb/s on channel 44",
            3,
        ),
        t(
            "bluetoothd",
            L::Debug,
            "com.apple.bluetooth",
            "LE",
            "Scan window: 0 advertisements",
            4,
        ),
        t(
            "mediaserverd",
            L::Debug,
            "com.apple.coreaudio",
            "AVAudioSession",
            "Route change: Speaker → Speaker (category change)",
            3,
        ),
        t(
            "powerd",
            L::Info,
            "com.apple.powerd",
            "Battery",
            "Charging: 87 %, 1.12 A, 4.21 V, 33.8 °C",
            2,
        ),
        t(
            "backboardd",
            L::Debug,
            "com.apple.backboardd",
            "Display",
            "Brightness 0.62 (auto)",
            3,
        ),
        t(
            "thermalmonitord",
            L::Debug,
            "com.apple.thermalmonitor",
            "state",
            "Thermal pressure check: Nominal",
            2,
        ),
        t(
            "dasd",
            L::Info,
            "com.apple.duetactivityscheduler",
            "Scoring",
            "Activity com.example.fieldkit.refresh scored 0.71",
            2,
        ),
        t(
            "kernel_task",
            L::Notice,
            "com.apple.xnu",
            "vm",
            "memorystatus: available pages 41 % (above threshold)",
            1,
        ),
    ]
}

fn android_logs() -> Vec<LogTemplate> {
    use LogLevel as L;
    let t = |process, level, scope, buffer, message, weight| LogTemplate {
        process,
        level,
        scope,
        category: Some(buffer),
        message,
        weight,
    };
    vec![
        t(
            "com.example.fieldkit",
            L::Debug,
            "FieldKit/Sync",
            "main",
            "Sync: 18 pending records → POST /v2/sync",
            14,
        ),
        t(
            "com.example.fieldkit",
            L::Info,
            "FieldKit/Sync",
            "main",
            "Sync completed in 388 ms (18 records)",
            8,
        ),
        t(
            "com.example.fieldkit",
            L::Debug,
            "FieldKit/UI",
            "main",
            "FormFragment onResume (form 'Site inspection')",
            10,
        ),
        t(
            "com.example.fieldkit",
            L::Warning,
            "OkHttp",
            "main",
            "Retrying request after SocketTimeoutException (timeout 10 s)",
            3,
        ),
        t(
            "com.example.fieldkit",
            L::Error,
            "FieldKit/Net",
            "main",
            "java.net.SocketTimeoutException: timeout — POST /v2/sync",
            3,
        ),
        t(
            "com.example.fieldkit",
            L::Debug,
            "Room",
            "main",
            "WAL checkpoint: 3 pages",
            5,
        ),
        t(
            "com.example.fieldkit",
            L::Info,
            "FieldKit/Photos",
            "main",
            "Compressed photo 4032×3024 → 1.8 MB (q=82)",
            4,
        ),
        t(
            "system_server",
            L::Info,
            "ActivityManager",
            "system",
            "Displayed com.example.fieldkit/.ui.FormActivity: +312ms",
            3,
        ),
        t(
            "system_server",
            L::Info,
            "WindowManager",
            "system",
            "Relayout window com.example.fieldkit/.ui.FormActivity: visible=true",
            3,
        ),
        t(
            "system_server",
            L::Debug,
            "ConnectivityService",
            "system",
            "NetworkAgentInfo [WIFI () - 100] validation status: VALID",
            3,
        ),
        t(
            "system_server",
            L::Info,
            "WifiService",
            "system",
            "RSSI -58 dBm, link speed 866 Mbps, band 5 GHz",
            2,
        ),
        t(
            "system_server",
            L::Debug,
            "BatteryService",
            "system",
            "level=87 status=CHARGING voltage=4210 temp=338",
            2,
        ),
        t(
            "surfaceflinger",
            L::Debug,
            "SurfaceFlinger",
            "main",
            "Frame missed: 0 of 60 in the last second",
            3,
        ),
        t(
            "com.android.systemui",
            L::Debug,
            "StatusBar",
            "main",
            "Notification posted: com.example.fieldkit (sync)",
            3,
        ),
        t(
            "com.google.android.gms",
            L::Info,
            "FusedLocation",
            "main",
            "Location delivered to com.example.fieldkit: accuracy 5.1 m",
            3,
        ),
        t(
            "logd",
            L::Warning,
            "logd",
            "system",
            "Lost 0 lines in main buffer",
            1,
        ),
        t(
            "netd",
            L::Debug,
            "Netd",
            "system",
            "bandwidth stats: iface wlan0 rx=1.28 GB tx=226 MB",
            2,
        ),
        t(
            "adbd",
            L::Debug,
            "adbd",
            "system",
            "shell service: /system/bin/sh -c 'cat /proc/stat'",
            2,
        ),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::FakeDevice;

    fn sim(kind: FakeDevice, scenario: Scenario) -> Simulator {
        Simulator::new(Profile::new(kind), scenario, 7)
    }

    #[test]
    fn iphone_emits_every_allowed_metric_in_range() {
        let mut s = sim(FakeDevice::Iphone, Scenario::Warm);
        for tick in 0..120 {
            let t = f64::from(tick);
            let v = s.samples(t, 1.0);
            assert_eq!(v.len(), s.allowed().len());
            let get = |id| v.iter().find(|m| m.id == id).unwrap().value;
            assert!((20.0..=70.0).contains(&get(MetricId::CpuTotal)));
            assert!((5.0e6..=30.0e6).contains(&get(MetricId::NetRxBps)));
            assert!((1.0e6..=4.0e6).contains(&get(MetricId::NetTxBps)));
            let total = get(MetricId::MemTotal);
            assert!(get(MetricId::MemUsed) < total);
            assert!(get(MetricId::MemFree) >= 0.0);
            assert_eq!(get(MetricId::BatCharging), 1.0);
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let mut a = sim(FakeDevice::Android, Scenario::Warm);
        let mut b = sim(FakeDevice::Android, Scenario::Warm);
        for tick in 0..50 {
            let t = f64::from(tick) * 0.05;
            assert_eq!(a.samples(t, 0.05), b.samples(t, 0.05));
            assert_eq!(a.cores(t), b.cores(t));
        }
        assert_eq!(a.processes(), b.processes());
        assert_eq!(a.log_lines(1000, 200, 4), b.log_lines(1000, 200, 4));
    }

    #[test]
    fn ipad_tier1_emits_no_cpu() {
        let mut s = sim(
            FakeDevice::Ipad {
                developer_mode: false,
            },
            Scenario::Quiet,
        );
        let v = s.samples(0.0, 1.0);
        assert!(v.iter().all(|m| m.id != MetricId::CpuTotal));
        assert!(v.iter().any(|m| m.id == MetricId::BatPercent));
    }

    #[test]
    fn warm_scenario_fires_thermal_then_memory_warning() {
        let mut s = sim(FakeDevice::Iphone, Scenario::Warm);
        assert!(s.scenario_step(19.9, 1).is_empty());
        let step = s.scenario_step(20.0, 2);
        assert!(matches!(
            step.first(),
            Some(Message::Event {
                kind: EventKind::ThermalChange,
                ..
            })
        ));
        assert!(matches!(step.get(1), Some(Message::Log { .. })));
        assert!(s.scenario_step(21.0, 3).is_empty());
        let v = s.samples(21.0, 1.0);
        assert_eq!(
            v.iter()
                .find(|m| m.id == MetricId::ThermalState)
                .unwrap()
                .value,
            1.0
        );
        let step = s.scenario_step(35.0, 4);
        assert!(matches!(
            step.first(),
            Some(Message::Event {
                kind: EventKind::MemoryWarning,
                process: Some(p),
                ..
            }) if p == "FieldKit"
        ));
        match step.get(1) {
            Some(Message::Log { entries, .. }) => assert_eq!(entries[0].level, LogLevel::Fault),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn crash_scenario_removes_and_relaunches_fieldkit() {
        let mut s = sim(FakeDevice::Android, Scenario::Crash);
        let step = s.scenario_step(15.0, 1);
        assert!(matches!(
            step.first(),
            Some(Message::Event {
                kind: EventKind::Crash,
                ..
            })
        ));
        assert!(
            s.processes()
                .iter()
                .all(|p| p.name != "com.example.fieldkit")
        );
        let step = s.scenario_step(20.0, 2);
        assert!(matches!(
            step.first(),
            Some(Message::Event {
                kind: EventKind::AppLaunch,
                ..
            })
        ));
        assert!(
            s.processes()
                .iter()
                .any(|p| p.name == "com.example.fieldkit")
        );
    }

    #[test]
    fn stress_is_hot_and_quiet_is_flat() {
        let mut s = sim(FakeDevice::Iphone, Scenario::Stress);
        for tick in 0..30 {
            let v = s.samples(f64::from(tick), 1.0);
            let cpu = v.iter().find(|m| m.id == MetricId::CpuTotal).unwrap().value;
            assert!((80.0..=100.0).contains(&cpu));
            assert_eq!(
                v.iter()
                    .find(|m| m.id == MetricId::MemPressure)
                    .unwrap()
                    .value,
                2.0
            );
        }
        let mut q = sim(FakeDevice::Iphone, Scenario::Quiet);
        let a = q.samples(0.0, 1.0);
        let b = q.samples(1.0, 1.0);
        let cpu = |v: &[MetricValue]| v.iter().find(|m| m.id == MetricId::CpuTotal).unwrap().value;
        assert_eq!(cpu(&a), cpu(&b));
    }

    #[test]
    fn logs_have_platform_shape_and_monotonic_seq() {
        let mut s = sim(FakeDevice::Android, Scenario::Quiet);
        let lines = s.log_lines(10_000, 200, 4);
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|l| l.tag.is_some() && l.buffer.is_some()));
        assert!(
            lines
                .windows(2)
                .all(|w| w[0].seq < w[1].seq && w[0].ts_ms <= w[1].ts_ms)
        );
        let mut i = sim(FakeDevice::Iphone, Scenario::Quiet);
        let lines = i.log_lines(10_000, 200, 4);
        assert!(
            lines
                .iter()
                .all(|l| l.subsystem.is_some() && l.tag.is_none())
        );
    }

    #[test]
    fn android_cores_report_frequencies() {
        let mut s = sim(FakeDevice::Android, Scenario::Warm);
        s.samples(0.0, 1.0);
        let cores = s.cores(0.0);
        assert_eq!(cores.len(), 8);
        assert!(cores.iter().all(|c| c.freq_mhz.is_some() && c.online));
        assert_eq!(
            cores
                .iter()
                .filter(|c| c.kind == CoreKind::Performance)
                .count(),
            4
        );
        let ios = sim(FakeDevice::Iphone, Scenario::Warm).cores(0.0);
        assert!(ios.iter().all(|c| c.freq_mhz.is_none()));
        assert_eq!(ios.len(), 6);
    }
}
