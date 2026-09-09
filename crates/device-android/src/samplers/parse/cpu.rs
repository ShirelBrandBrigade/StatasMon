//! `/proc/stat`, `/sys/devices/system/cpu/online`, cpufreq and `/proc/loadavg`.

use std::collections::BTreeMap;

use super::{ParseError, parse_f64, parse_u64};

/// Jiffies from one `cpu`/`cpuN` line of `/proc/stat`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    /// Time spent doing work (everything but idle and iowait).
    pub fn busy(&self) -> u64 {
        self.user + self.nice + self.system + self.irq + self.softirq + self.steal
    }
    pub fn total(&self) -> u64 {
        self.busy() + self.idle + self.iowait
    }
    /// Ticks in user space (user + nice).
    pub fn user_all(&self) -> u64 {
        self.user + self.nice
    }
    /// Ticks in the kernel (system + irq + softirq).
    pub fn system_all(&self) -> u64 {
        self.system + self.irq + self.softirq
    }

    /// Per-field difference, saturating (counters never go backwards
    /// except across a reboot, which the caller treats as a reset).
    pub fn delta(&self, prev: &CpuTimes) -> CpuTimes {
        CpuTimes {
            user: self.user.saturating_sub(prev.user),
            nice: self.nice.saturating_sub(prev.nice),
            system: self.system.saturating_sub(prev.system),
            idle: self.idle.saturating_sub(prev.idle),
            iowait: self.iowait.saturating_sub(prev.iowait),
            irq: self.irq.saturating_sub(prev.irq),
            softirq: self.softirq.saturating_sub(prev.softirq),
            steal: self.steal.saturating_sub(prev.steal),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcStat {
    pub total: CpuTimes,
    /// Only the cores present in the file (offline cores vanish).
    pub cores: BTreeMap<u16, CpuTimes>,
}

pub fn parse_proc_stat(text: &str) -> Result<ProcStat, ParseError> {
    let mut out = ProcStat::default();
    let mut saw_total = false;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(label) = it.next() else { continue };
        if !label.starts_with("cpu") {
            continue;
        }
        let nums: Vec<u64> = it
            .map(|t| parse_u64("cpu jiffies", t))
            .collect::<Result<_, _>>()?;
        if nums.len() < 4 {
            return Err(ParseError::invalid("cpu line", line));
        }
        let get = |i: usize| nums.get(i).copied().unwrap_or(0);
        let t = CpuTimes {
            user: get(0),
            nice: get(1),
            system: get(2),
            idle: get(3),
            iowait: get(4),
            irq: get(5),
            softirq: get(6),
            steal: get(7),
        };
        if label == "cpu" {
            out.total = t;
            saw_total = true;
        } else if let Ok(idx) = label[3..].parse::<u16>() {
            out.cores.insert(idx, t);
        }
    }
    if !saw_total {
        return Err(ParseError::Missing("cpu total line"));
    }
    Ok(out)
}

/// `0-7`, `0-3,5,7` → the online core indices.
pub fn parse_cpu_online(text: &str) -> Result<Vec<u16>, ParseError> {
    let s = text.trim();
    if s.is_empty() {
        return Err(ParseError::Empty("cpu online"));
    }
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((a, b)) = part.split_once('-') {
            let a: u16 = a
                .trim()
                .parse()
                .map_err(|_| ParseError::invalid("cpu online", part))?;
            let b: u16 = b
                .trim()
                .parse()
                .map_err(|_| ParseError::invalid("cpu online", part))?;
            out.extend(a..=b);
        } else if !part.is_empty() {
            out.push(
                part.parse()
                    .map_err(|_| ParseError::invalid("cpu online", part))?,
            );
        }
    }
    Ok(out)
}

/// `grep -H . /sys/devices/system/cpu/cpu[0-9]*/cpufreq/<file>` output:
/// `/sys/devices/system/cpu/cpu4/cpufreq/scaling_cur_freq:768000` → core → kHz.
/// Also accepts the fixture's `== /sys/.../cpuN` + value blocks (first
/// value after each header) so the recorded capture can be replayed.
pub fn parse_cpufreq_khz(text: &str) -> Result<BTreeMap<u16, u64>, ParseError> {
    let mut out = BTreeMap::new();
    let mut pending: Option<u16> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("== ") {
            pending = core_index(rest);
            continue;
        }
        if let Some((path, value)) = line.rsplit_once(':')
            && path.contains("/cpu")
        {
            if let (Some(core), Ok(khz)) = (core_index(path), parse_u64("cpufreq", value)) {
                out.insert(core, khz);
            }
            continue;
        }
        if let Some(core) = pending.take()
            && let Ok(khz) = parse_u64("cpufreq", line)
        {
            out.insert(core, khz);
        }
    }
    if out.is_empty() {
        return Err(ParseError::Empty("cpufreq"));
    }
    Ok(out)
}

/// Extract `N` from a path containing `/cpuN/` or ending in `/cpuN`.
fn core_index(path: &str) -> Option<u16> {
    // `/cpu` followed by digits; `/cpufreq` does not count.
    path.rmatch_indices("/cpu").find_map(|(i, _)| {
        let digits: String = path[i + 4..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadAvg {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
    pub running: u32,
    pub total: u32,
}

/// `2.27 2.47 2.67 3/4991 5626`
pub fn parse_loadavg(text: &str) -> Result<LoadAvg, ParseError> {
    let mut it = text.split_whitespace();
    let one = parse_f64("load1", it.next().ok_or(ParseError::Missing("load1"))?)?;
    let five = parse_f64("load5", it.next().ok_or(ParseError::Missing("load5"))?)?;
    let fifteen = parse_f64("load15", it.next().ok_or(ParseError::Missing("load15"))?)?;
    let (running, total) = it
        .next()
        .and_then(|s| s.split_once('/'))
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .unwrap_or((0, 0));
    Ok(LoadAvg {
        one,
        five,
        fifteen,
        running,
        total,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn proc_stat_has_eight_cores_and_deltas() {
        let a = parse_proc_stat(&fixture("proc_stat_1")).unwrap();
        let b = parse_proc_stat(&fixture("proc_stat_2")).unwrap();
        assert_eq!(a.cores.len(), 8);
        assert_eq!(a.cores[&0].user, 35594);
        let d = b.total.delta(&a.total);
        // 1 s apart on 8 cores at 100 Hz ≈ 800 jiffies (plus timer slop).
        assert!(
            (700..=1000).contains(&d.total()),
            "total delta {}",
            d.total()
        );
        let busy = d.busy() as f64 / d.total() as f64 * 100.0;
        assert!(busy > 0.0 && busy < 20.0, "idle tablet busy {busy}%");
        let c7 = b.cores[&7].delta(&a.cores[&7]);
        assert_eq!(c7.busy(), 0);
    }

    #[test]
    fn online_ranges() {
        assert_eq!(
            parse_cpu_online(&fixture("cpu_online")).unwrap(),
            (0..8).collect::<Vec<u16>>()
        );
        assert_eq!(parse_cpu_online("0-2,5,7\n").unwrap(), vec![0, 1, 2, 5, 7]);
        assert!(parse_cpu_online("").is_err());
    }

    #[test]
    fn cpufreq_from_grep_and_from_fixture_blocks() {
        let grep = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq:614400\n\
                    /sys/devices/system/cpu/cpu7/cpufreq/scaling_cur_freq:806400\n";
        let m = parse_cpufreq_khz(grep).unwrap();
        assert_eq!(m[&0], 614_400);
        assert_eq!(m[&7], 806_400);
        // The recorded capture prints cur, max, min per core; first value is cur.
        let m = parse_cpufreq_khz(&fixture("cpufreq")).unwrap();
        assert_eq!(m.len(), 8);
        assert_eq!(m[&3], 1_785_600);
        assert_eq!(m[&6], 2_496_000);
    }

    #[test]
    fn loadavg() {
        let l = parse_loadavg(&fixture("proc_loadavg")).unwrap();
        assert_eq!(l.one, 2.27);
        assert_eq!(l.five, 2.47);
        assert_eq!(l.running, 3);
        assert_eq!(l.total, 4991);
    }
}
