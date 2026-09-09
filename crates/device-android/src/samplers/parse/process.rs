//! `ps -A -o PID,PPID,USER,%CPU,RSS,VSZ,NAME`, `cat /proc/[0-9]*/stat`,
//! the resumed activity line, and the Android USER → uid mapping.

use std::collections::HashMap;

use super::{ParseError, parse_u64};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsRow {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub uid: Option<u32>,
    /// KiB (toybox `ps` prints RSS in KiB, not pages: systemui shows
    /// 297472 → 290 MiB, which matches `dumpsys meminfo`).
    pub rss_kib: u64,
    pub vsz_kib: u64,
    pub name: String,
}

impl PsRow {
    /// Package when NAME looks like one (`com.foo.bar:service` → `com.foo.bar`).
    pub fn package(&self) -> Option<&str> {
        let base = self.name.split(':').next().unwrap_or(&self.name);
        (base.contains('.') && !base.contains('/') && !base.starts_with('[')).then_some(base)
    }
}

pub fn parse_ps(text: &str) -> Result<Vec<PsRow>, ParseError> {
    let mut lines = text.lines();
    let header = lines.next().ok_or(ParseError::Empty("ps"))?;
    let cols: Vec<&str> = header.split_whitespace().collect();
    let idx = |name: &'static str| -> Result<usize, ParseError> {
        cols.iter()
            .position(|c| *c == name)
            .ok_or(ParseError::Missing(name))
    };
    let (i_pid, i_ppid, i_user, i_rss, i_vsz, i_name) = (
        idx("PID")?,
        idx("PPID")?,
        idx("USER")?,
        idx("RSS")?,
        idx("VSZ")?,
        idx("NAME")?,
    );
    if i_name != cols.len() - 1 {
        return Err(ParseError::invalid("ps header", header));
    }
    let mut out = Vec::new();
    for line in lines {
        // NAME is the last column and may contain spaces: take the fixed
        // columns token by token and keep the remainder verbatim.
        let Some((fixed, name)) = split_fixed(line, cols.len() - 1) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let user = fixed[i_user].to_owned();
        out.push(PsRow {
            pid: parse_u64("PID", fixed[i_pid])? as u32,
            ppid: parse_u64("PPID", fixed[i_ppid])? as u32,
            uid: uid_from_user(&user),
            user,
            rss_kib: parse_u64("RSS", fixed[i_rss])?,
            vsz_kib: parse_u64("VSZ", fixed[i_vsz])?,
            name: name.to_owned(),
        });
    }
    if out.is_empty() {
        return Err(ParseError::Empty("ps rows"));
    }
    Ok(out)
}

/// `n` whitespace-separated tokens, then the rest of the line trimmed.
fn split_fixed(line: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut rest = line.trim_start();
    let mut fixed = Vec::with_capacity(n);
    for _ in 0..n {
        let end = rest.find(char::is_whitespace)?;
        fixed.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    Some((fixed, rest.trim_end()))
}

/// Android `USER` column → uid. `u0_a123` → 10123, `u0_i5` → 99005,
/// `u10_a7` → 1010007; well-known AIDs from android_filesystem_config.h;
/// bare numbers pass through. Unknown names → `None` (no netstats join).
pub fn uid_from_user(user: &str) -> Option<u32> {
    if let Ok(n) = user.parse::<u32>() {
        return Some(n);
    }
    if let Some(rest) = user.strip_prefix('u')
        && let Some((u, app)) = rest.split_once('_')
        && let Ok(u) = u.parse::<u32>()
    {
        let base = u * 100_000;
        if let Some(n) = app.strip_prefix('a').and_then(|n| n.parse::<u32>().ok()) {
            return Some(base + 10_000 + n);
        }
        if let Some(n) = app.strip_prefix('i').and_then(|n| n.parse::<u32>().ok()) {
            return Some(base + 99_000 + n);
        }
        if let Some(n) = app.strip_prefix("s").and_then(|n| n.parse::<u32>().ok()) {
            return Some(base + 20_000 + n);
        }
    }
    let aid = match user {
        "root" => 0,
        "system" => 1000,
        "radio" => 1001,
        "bluetooth" => 1002,
        "graphics" => 1003,
        "input" => 1004,
        "audio" => 1005,
        "camera" => 1006,
        "log" => 1007,
        "compass" => 1008,
        "mount" => 1009,
        "wifi" => 1010,
        "adb" => 1011,
        "install" => 1012,
        "media" => 1013,
        "dhcp" => 1014,
        "sdcard_rw" => 1015,
        "vpn" => 1016,
        "keystore" => 1017,
        "usb" => 1018,
        "drm" => 1019,
        "mdnsr" => 1020,
        "gps" => 1021,
        "media_rw" => 1023,
        "mtp" => 1024,
        "drmrpc" => 1026,
        "nfc" => 1027,
        "sdcard_r" => 1028,
        "clat" => 1029,
        "loop_radio" => 1030,
        "mediadrm" | "media_drm" => 1031,
        "package_info" => 1032,
        "sdcard_pics" => 1033,
        "sdcard_av" => 1034,
        "sdcard_all" => 1035,
        "logd" => 1036,
        "shared_relro" => 1037,
        "dbus" => 1038,
        "tlsdate" => 1039,
        "mediaex" | "media_ex" => 1040,
        "audioserver" => 1041,
        "metrics_coll" => 1042,
        "metricsd" => 1043,
        "webserv" => 1044,
        "debuggerd" => 1045,
        "mediacodec" => 1046,
        "cameraserver" => 1047,
        "firewall" => 1048,
        "trunks" => 1049,
        "nvram" => 1050,
        "dns" => 1051,
        "dns_tether" => 1052,
        "webview_zygote" => 1053,
        "vehicle_network" => 1054,
        "media_audio" => 1055,
        "media_video" => 1056,
        "media_image" => 1057,
        "tombstoned" => 1058,
        "media_obb" => 1059,
        "ese" => 1060,
        "ota_update" => 1061,
        "automotive_evs" => 1062,
        "lowpan" => 1063,
        "hsm" => 1064,
        "reserved_disk" => 1065,
        "statsd" => 1066,
        "incidentd" => 1067,
        "secure_element" => 1068,
        "lmkd" => 1069,
        "llkd" => 1070,
        "iorapd" => 1071,
        "gpu_service" => 1072,
        "network_stack" => 1073,
        "gsid" => 1074,
        "fsverity_cert" => 1075,
        "credstore" => 1076,
        "external_storage" => 1077,
        "ext_data_rw" => 1078,
        "ext_obb_rw" => 1079,
        "context_hub" => 1080,
        "virtualizationservice" => 1081,
        "artd" => 1082,
        "uwb" => 1083,
        "thread_network" => 1084,
        "diced" => 1085,
        "dmesgd" => 1086,
        "jc_weaver" => 1087,
        "jc_strongbox" => 1088,
        "jc_identitycred" => 1089,
        "sdk_sandbox" => 1090,
        "security_log_writer" => 1091,
        "prng_seeder" => 1092,
        "uprobestats" => 1093,
        "shell" => 2000,
        "cache" => 2001,
        "diag" => 2002,
        "nobody" => 9999,
        _ => return None,
    };
    Some(aid)
}

/// The fields we keep from `/proc/<pid>/stat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidStat {
    pub pid: u32,
    pub comm: String,
    pub state: char,
    /// utime + stime in clock ticks (CLK_TCK = 100 on Android).
    pub cpu_ticks: u64,
    pub num_threads: u32,
}

/// Clock ticks per second on every Android kernel (`sysconf(_SC_CLK_TCK)`).
pub const CLK_TCK: f64 = 100.0;

/// Parse the concatenation of many `/proc/<pid>/stat` lines.
pub fn parse_pid_stats(text: &str) -> HashMap<u32, PidStat> {
    let mut out = HashMap::new();
    for line in text.lines() {
        if let Some(s) = parse_pid_stat_line(line) {
            out.insert(s.pid, s);
        }
    }
    out
}

fn parse_pid_stat_line(line: &str) -> Option<PidStat> {
    // `pid (comm) state ppid …` — comm may contain spaces and parentheses,
    // so split on the *last* ')'.
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    let pid: u32 = line[..open].trim().parse().ok()?;
    let comm = line[open + 1..close].to_owned();
    let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();
    // rest[0] = state (field 3); utime = field 14 → rest[11]; stime → rest[12];
    // num_threads = field 20 → rest[17].
    if rest.len() < 18 {
        return None;
    }
    let state = rest[0].chars().next()?;
    let utime: u64 = rest[11].parse().ok()?;
    let stime: u64 = rest[12].parse().ok()?;
    let num_threads: u32 = rest[17].parse().ok()?;
    Some(PidStat {
        pid,
        comm,
        state,
        cpu_ticks: utime + stime,
        num_threads,
    })
}

/// Package of the resumed (foreground) activity from
/// `dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity'`:
/// `topResumedActivity=ActivityRecord{… u0 com.android.settings/.SubSettings t178}`.
pub fn parse_resumed_package(text: &str) -> Option<String> {
    let mut fallback = None;
    for line in text.lines() {
        let Some(start) = line.find("ActivityRecord{") else {
            continue;
        };
        let body = &line[start..];
        // tokens: ActivityRecord{<hash> u<user> <pkg>/<activity> t<task>}
        let pkg = body
            .split_whitespace()
            .find(|t| t.contains('/'))
            .and_then(|t| t.split('/').next())
            .map(str::to_owned);
        if pkg.is_none() {
            continue;
        }
        if line.contains("topResumedActivity") {
            return pkg;
        }
        fallback = fallback.or(pkg);
    }
    fallback
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn ps_rows_and_packages() {
        let rows = parse_ps(&fixture("ps")).unwrap();
        assert!(rows.len() > 500, "{} rows", rows.len());
        let init = &rows[0];
        assert_eq!((init.pid, init.ppid, init.user.as_str()), (1, 0, "root"));
        assert_eq!(init.uid, Some(0));
        assert_eq!(init.rss_kib, 5652);
        assert_eq!(init.name, "init");
        assert_eq!(init.package(), None);
        let sysui = rows
            .iter()
            .find(|r| r.name == "com.android.systemui")
            .unwrap();
        assert_eq!(sysui.pid, 3133);
        assert_eq!(sysui.uid, Some(10055));
        assert_eq!(sysui.rss_kib, 297_472);
        assert_eq!(sysui.package(), Some("com.android.systemui"));
        let gsb = rows
            .iter()
            .find(|r| r.name == "com.google.android.googlequicksearchbox:interactor")
            .unwrap();
        assert_eq!(
            gsb.package(),
            Some("com.google.android.googlequicksearchbox")
        );
        let kthread = rows.iter().find(|r| r.name == "[kthreadd]").unwrap();
        assert_eq!(kthread.package(), None);
    }

    #[test]
    fn uid_mapping() {
        assert_eq!(uid_from_user("u0_a219"), Some(10219));
        assert_eq!(uid_from_user("u0_i12"), Some(99012));
        assert_eq!(uid_from_user("u10_a7"), Some(1_010_007));
        assert_eq!(uid_from_user("system"), Some(1000));
        assert_eq!(uid_from_user("shell"), Some(2000));
        assert_eq!(uid_from_user("1073"), Some(1073));
        assert_eq!(uid_from_user("weird_vendor"), None);
    }

    #[test]
    fn pid_stat_fields() {
        let m = parse_pid_stats(&fixture("proc_pid_stat_sample"));
        let init = &m[&1];
        assert_eq!(init.comm, "init");
        assert_eq!(init.state, 'S');
        assert_eq!(init.cpu_ticks, 163 + 325);
        assert_eq!(init.num_threads, 3);
        let s = parse_pid_stat_line("42 (a (b) c) R 1 1 1 0 -1 0 0 0 0 0 7 8 0 0 20 0 5 0 0 0 0")
            .unwrap();
        assert_eq!(s.comm, "a (b) c");
        assert_eq!(s.cpu_ticks, 15);
        assert_eq!(s.num_threads, 5);
    }

    #[test]
    fn resumed_package() {
        assert_eq!(
            parse_resumed_package(&fixture("dumpsys_activity_resumed")).as_deref(),
            Some("com.android.settings")
        );
        assert_eq!(parse_resumed_package("  mResumedActivity: null\n"), None);
    }
}
