//! `/proc/meminfo`, `/proc/pressure/memory` (PSI) and `/proc/vmstat`.

use std::collections::HashMap;

use super::{ParseError, parse_f64, parse_u64};

/// `/proc/meminfo` as bytes, keyed by the field name without the colon
/// (`MemTotal`, `zram0`, …). Values without a `kB` suffix are taken as-is.
pub fn parse_meminfo(text: &str) -> Result<HashMap<String, u64>, ParseError> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let mut it = rest.split_whitespace();
        let Some(num) = it.next() else { continue };
        let Ok(v) = parse_u64("meminfo", num) else {
            continue;
        };
        let bytes = match it.next() {
            Some("kB") => v * 1024,
            Some("MB") => v * 1024 * 1024,
            _ => v,
        };
        out.insert(key.trim().to_owned(), bytes);
    }
    if !out.contains_key("MemTotal") {
        return Err(ParseError::Missing("MemTotal"));
    }
    Ok(out)
}

/// The breakdown the catalog asks for, derived from meminfo. Documented
/// mapping (Android has no "wired"/"compressed" in the iOS sense):
/// - `used` = MemTotal − MemAvailable (what `dumpsys meminfo` calls used)
/// - `free` = MemFree
/// - `cached` = Cached + Buffers (page cache; reclaimable)
/// - `app` = AnonPages (anonymous memory of user processes)
/// - `wired` = SUnreclaim + KernelStack + PageTables (kernel, not reclaimable)
/// - `swap` = SwapTotal − SwapFree (swap in use; ZRAM-backed on Android)
/// - `compressed` = the `zram0` line when the kernel exposes it (Samsung
///   does): physical bytes the compressed pages occupy. Absent otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemBreakdown {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub cached: u64,
    pub app: Option<u64>,
    pub wired: Option<u64>,
    pub swap: Option<u64>,
    pub compressed: Option<u64>,
}

pub fn breakdown(m: &HashMap<String, u64>) -> Result<MemBreakdown, ParseError> {
    let g = |k: &str| m.get(k).copied();
    let total = g("MemTotal").ok_or(ParseError::Missing("MemTotal"))?;
    let free = g("MemFree").ok_or(ParseError::Missing("MemFree"))?;
    let available = g("MemAvailable").unwrap_or(free);
    let cached = g("Cached").unwrap_or(0) + g("Buffers").unwrap_or(0);
    let wired = match (g("SUnreclaim"), g("KernelStack"), g("PageTables")) {
        (Some(a), b, c) => Some(a + b.unwrap_or(0) + c.unwrap_or(0)),
        _ => None,
    };
    let swap = match (g("SwapTotal"), g("SwapFree")) {
        (Some(t), Some(f)) => Some(t.saturating_sub(f)),
        _ => None,
    };
    Ok(MemBreakdown {
        total,
        used: total.saturating_sub(available),
        free,
        cached,
        app: g("AnonPages"),
        wired,
        swap,
        compressed: g("zram0"),
    })
}

/// PSI `some`/`full` avg10 from `/proc/pressure/memory`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemPsi {
    pub some_avg10: f64,
    pub full_avg10: f64,
}

pub fn parse_psi_memory(text: &str) -> Result<MemPsi, ParseError> {
    let mut some = None;
    let mut full = None;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(kind) = it.next() else { continue };
        let avg10 = it
            .find_map(|t| t.strip_prefix("avg10="))
            .ok_or(ParseError::Missing("avg10"))?;
        let v = parse_f64("avg10", avg10)?;
        match kind {
            "some" => some = Some(v),
            "full" => full = Some(v),
            _ => {}
        }
    }
    Ok(MemPsi {
        some_avg10: some.ok_or(ParseError::Missing("some"))?,
        full_avg10: full.unwrap_or(0.0),
    })
}

/// `mem.pressure` 0–3 from PSI `some avg10` (%): heuristic thresholds
/// 1 / 5 / 20 % of wall time stalled on memory.
pub fn pressure_from_psi(psi: &MemPsi) -> f64 {
    match psi.some_avg10 {
        x if x < 1.0 => 0.0,
        x if x < 5.0 => 1.0,
        x if x < 20.0 => 2.0,
        _ => 3.0,
    }
}

/// Fallback when PSI is unreadable: major page faults per second from
/// `/proc/vmstat` `pgmajfault` deltas (> 50/s → 1, > 500/s → 2). Never
/// reaches 3: a fault rate cannot prove a critical state.
pub fn pressure_from_majfault_rate(faults_per_sec: f64) -> f64 {
    if faults_per_sec > 500.0 {
        2.0
    } else if faults_per_sec > 50.0 {
        1.0
    } else {
        0.0
    }
}

/// One counter from `/proc/vmstat` (`pgmajfault 628838`).
pub fn parse_vmstat_field(text: &str, field: &'static str) -> Result<u64, ParseError> {
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() == Some(field) {
            return parse_u64(field, it.next().ok_or(ParseError::Missing(field))?);
        }
    }
    Err(ParseError::Missing(field))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn meminfo_in_bytes() {
        let m = parse_meminfo(&fixture("proc_meminfo")).unwrap();
        assert_eq!(m["MemTotal"], 7_467_144 * 1024);
        assert_eq!(m["zram0"], 449_652 * 1024);
        let b = breakdown(&m).unwrap();
        assert_eq!(b.total, 7_467_144 * 1024);
        assert_eq!(b.used, (7_467_144 - 2_896_320) * 1024);
        assert_eq!(b.free, 315_276 * 1024);
        assert_eq!(b.cached, (2_728_640 + 2_356) * 1024);
        assert_eq!(b.app, Some(1_738_072 * 1024));
        assert_eq!(b.swap, Some((8_388_604 - 6_588_668) * 1024));
        assert_eq!(b.compressed, Some(449_652 * 1024));
        assert_eq!(b.wired, Some((370_108 + 80_000 + 162_296) * 1024));
        assert!(b.used < b.total);
    }

    #[test]
    fn psi_and_pressure() {
        let p = parse_psi_memory(&fixture("proc_pressure_memory")).unwrap();
        assert_eq!(p.some_avg10, 0.0);
        assert_eq!(pressure_from_psi(&p), 0.0);
        let hot = MemPsi {
            some_avg10: 7.5,
            full_avg10: 1.0,
        };
        assert_eq!(pressure_from_psi(&hot), 2.0);
        assert_eq!(pressure_from_majfault_rate(10.0), 0.0);
        assert_eq!(pressure_from_majfault_rate(80.0), 1.0);
        assert_eq!(pressure_from_majfault_rate(900.0), 2.0);
    }

    #[test]
    fn vmstat_field() {
        assert_eq!(
            parse_vmstat_field(&fixture("proc_vmstat"), "pgmajfault").unwrap(),
            628_838
        );
        assert!(parse_vmstat_field("x 1\n", "pgmajfault").is_err());
    }
}
