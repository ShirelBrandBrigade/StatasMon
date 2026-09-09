//! User settings on disk (docs/PLAN.md 6.6, ADR-014): `settings.json` in
//! the app data folder, next to the session store. Missing or invalid
//! files fall back to defaults with a warning; writes go to a temp file
//! that is renamed into place so a crash never leaves a half-written file.

use std::path::{Path, PathBuf};

use statasmon_core::{EngineConfig, StoreConfig};
use statasmon_protocol::{SETTINGS_VERSION, Settings};

/// File name under the app data folder.
pub const SETTINGS_FILE: &str = "settings.json";

/// Lower bounds of the sampling cadences (ms) and the retention ranges.
pub const MIN_FAST_MS: u32 = 250;
pub const MIN_MEDIUM_MS: u32 = 1000;
pub const MIN_SLOW_MS: u32 = 5000;
pub const MIN_HEAVY_MS: u32 = 30_000;
pub const SESSION_HOURS: std::ops::RangeInclusive<u32> = 1..=48;
pub const KEEP_DAYS: std::ops::RangeInclusive<u32> = 1..=365;

/// Path of the settings file under `dir`.
pub fn path_in(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE)
}

/// Read the settings file under `dir`. Defaults when the file is missing;
/// defaults with a warning when it cannot be read or parsed. A file from a
/// newer format version is still read (unknown fields are dropped).
pub fn load(dir: &Path) -> Settings {
    let path = path_in(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(path = %path.display(), "no settings file; using defaults");
            return Settings::default();
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "settings unreadable; using defaults");
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(mut s) => {
            if let Err(reason) = validate(&s) {
                tracing::warn!(path = %path.display(), reason, "settings invalid; using defaults");
                return Settings::default();
            }
            if s.version > SETTINGS_VERSION {
                tracing::warn!(
                    path = %path.display(),
                    file_version = s.version,
                    supported = SETTINGS_VERSION,
                    "settings written by a newer version; unknown fields were ignored"
                );
            }
            s.version = SETTINGS_VERSION;
            s
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "settings unparsable; using defaults");
            Settings::default()
        }
    }
}

/// Write `settings` under `dir` atomically (temp file + rename), creating
/// the folder when needed.
pub fn save(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = path_in(dir);
    let tmp = dir.join(format!("{SETTINGS_FILE}.{}.tmp", std::process::id()));
    let mut text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    text.push('\n');
    std::fs::write(&tmp, text)?;
    match std::fs::rename(&tmp, &path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Check every field the UI can edit; the message names the first problem.
pub fn validate(s: &Settings) -> Result<(), String> {
    let sm = &s.sampling;
    if sm.fast_ms < MIN_FAST_MS {
        return Err(format!("Fast sampling must be at least {MIN_FAST_MS} ms"));
    }
    if sm.medium_ms < MIN_MEDIUM_MS {
        return Err(format!(
            "Medium sampling must be at least {MIN_MEDIUM_MS} ms"
        ));
    }
    if sm.slow_ms < MIN_SLOW_MS {
        return Err(format!("Slow sampling must be at least {MIN_SLOW_MS} ms"));
    }
    if sm.heavy_ms != 0 && sm.heavy_ms < MIN_HEAVY_MS {
        return Err(format!(
            "Heavy sampling must be 0 (off) or at least {MIN_HEAVY_MS} ms"
        ));
    }
    let r = &s.retention;
    if !SESSION_HOURS.contains(&r.max_session_hours) {
        return Err(format!(
            "Hours per session must be from {} to {}",
            SESSION_HOURS.start(),
            SESSION_HOURS.end()
        ));
    }
    if !KEEP_DAYS.contains(&r.keep_days) {
        return Err(format!(
            "Days to keep must be from {} to {}",
            KEEP_DAYS.start(),
            KEEP_DAYS.end()
        ));
    }
    if let Some(gb) = r.max_total_gb
        && (!gb.is_finite() || gb <= 0.0)
    {
        return Err("Total size cap must be a positive number of GB".to_owned());
    }
    if let Some(p) = s.adb_path.as_deref() {
        if p.trim().is_empty() {
            return Err("adb path is empty; clear it to auto-detect".to_owned());
        }
        if !Path::new(p).is_file() {
            return Err(format!("adb was not found at {p}"));
        }
    }
    if s.theme != "dark" {
        return Err("Only the dark theme exists".to_owned());
    }
    Ok(())
}

/// Normalise what the UI sent: trim the adb path, empty means none, and
/// stamp the current format version.
pub fn normalise(mut s: Settings) -> Settings {
    s.adb_path = s
        .adb_path
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty());
    s.version = SETTINGS_VERSION;
    s
}

/// One GB as the UI formats bytes (base 1024).
const GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Store retention from the settings, over `root`.
pub fn store_config(root: &Path, s: &Settings) -> StoreConfig {
    let r = &s.retention;
    StoreConfig::new(root)
        .with_max_session_ms(i64::from(r.max_session_hours) * 3_600_000)
        .with_keep_days(r.keep_days)
        .with_max_total_bytes(r.max_total_gb.map(|gb| (gb * GB) as u64))
}

/// Engine configuration from the settings (sampling; store when `root` is known).
pub fn engine_config(root: Option<&Path>, s: &Settings) -> EngineConfig {
    let config = EngineConfig::default().with_sampling(s.sampling.clone());
    match root {
        Some(root) => config.with_store(store_config(root, s)),
        None => config,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "statasmon-settings-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = temp_dir("missing");
        assert_eq!(load(&dir), Settings::default());
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp_file() {
        let dir = temp_dir("roundtrip");
        let mut s = Settings::default();
        s.sampling.fast_ms = 500;
        s.retention.keep_days = 7;
        s.retention.max_total_gb = Some(2.5);
        s.update_check = true;
        save(&dir, &s).unwrap();
        assert_eq!(load(&dir), s);
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![SETTINGS_FILE.to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_and_invalid_files_give_defaults() {
        let dir = temp_dir("garbage");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path_in(&dir), "{not json").unwrap();
        assert_eq!(load(&dir), Settings::default());
        let mut bad = Settings::default();
        bad.sampling.fast_ms = 10;
        std::fs::write(path_in(&dir), serde_json::to_string(&bad).unwrap()).unwrap();
        assert_eq!(load(&dir), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_version_is_read_and_restamped() {
        let dir = temp_dir("newer");
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Settings::default();
        s.version = SETTINGS_VERSION + 5;
        s.retention.keep_days = 12;
        let mut json: serde_json::Value = serde_json::to_value(&s).unwrap();
        json["future_field"] = serde_json::json!(true);
        std::fs::write(path_in(&dir), json.to_string()).unwrap();
        let loaded = load(&dir);
        assert_eq!(loaded.version, SETTINGS_VERSION);
        assert_eq!(loaded.retention.keep_days, 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation_bounds() {
        let ok = Settings::default();
        assert_eq!(validate(&ok), Ok(()));
        let mut s = ok.clone();
        s.sampling.fast_ms = 249;
        assert!(validate(&s).unwrap_err().contains("Fast"));
        s = ok.clone();
        s.sampling.medium_ms = 999;
        assert!(validate(&s).unwrap_err().contains("Medium"));
        s = ok.clone();
        s.sampling.slow_ms = 4999;
        assert!(validate(&s).unwrap_err().contains("Slow"));
        s = ok.clone();
        s.sampling.heavy_ms = 29_999;
        assert!(validate(&s).unwrap_err().contains("Heavy"));
        s.sampling.heavy_ms = 0;
        assert_eq!(validate(&s), Ok(()));
        s.sampling.heavy_ms = 30_000;
        assert_eq!(validate(&s), Ok(()));
        s = ok.clone();
        s.retention.max_session_hours = 0;
        assert!(validate(&s).unwrap_err().contains("Hours"));
        s.retention.max_session_hours = 49;
        assert!(validate(&s).unwrap_err().contains("Hours"));
        s = ok.clone();
        s.retention.keep_days = 366;
        assert!(validate(&s).unwrap_err().contains("Days"));
        s = ok.clone();
        s.retention.max_total_gb = Some(0.0);
        assert!(validate(&s).unwrap_err().contains("Total size"));
        s = ok.clone();
        s.adb_path = Some("/definitely/not/here/adb".to_owned());
        assert!(validate(&s).unwrap_err().contains("not found"));
        s = ok.clone();
        s.theme = "light".to_owned();
        assert!(validate(&s).unwrap_err().contains("dark"));
    }

    #[test]
    fn adb_path_must_be_a_file_and_is_trimmed() {
        let dir = temp_dir("adb");
        std::fs::create_dir_all(&dir).unwrap();
        let adb = dir.join("adb");
        std::fs::write(&adb, "#!/bin/sh\n").unwrap();
        let mut s = Settings::default();
        s.adb_path = Some(format!("  {}  ", adb.display()));
        let s = normalise(s);
        assert_eq!(s.adb_path.as_deref(), Some(adb.to_str().unwrap()));
        assert_eq!(validate(&s), Ok(()));
        let mut d = Settings::default();
        d.adb_path = Some(dir.to_string_lossy().into_owned());
        assert!(validate(&d).is_err(), "a directory is not adb");
        let e = normalise(Settings {
            adb_path: Some("   ".to_owned()),
            ..Settings::default()
        });
        assert_eq!(e.adb_path, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_config_carries_sampling_and_retention() {
        let mut s = Settings::default();
        s.sampling.fast_ms = 750;
        s.retention.max_session_hours = 2;
        s.retention.keep_days = 9;
        s.retention.max_total_gb = Some(1.5);
        let c = engine_config(Some(Path::new("/tmp/store")), &s);
        assert_eq!(c.sampling.fast_ms, 750);
        let store = c.store.expect("store configured");
        assert_eq!(store.max_session_ms, 2 * 3_600_000);
        assert_eq!(store.keep_days, 9);
        assert_eq!(store.max_total_bytes, Some(1_610_612_736));
        assert!(engine_config(None, &s).store.is_none());
    }
}
