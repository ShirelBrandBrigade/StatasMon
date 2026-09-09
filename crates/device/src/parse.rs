//! Helpers shared by provider parsers. Providers implement every parser as
//! a pure `parse_x(&str) -> DeviceResult<T>` with a fixture test
//! (docs/PLAN.md §4); the generic pieces live here.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time in milliseconds since the Unix epoch.
///
/// Every `ts_ms` on the bus uses this clock unless a provider gets a better
/// timestamp from the device.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Split a `key<sep>value` line into trimmed halves; `None` when the
/// separator is missing.
pub fn split_key_value(line: &str, sep: char) -> Option<(&str, &str)> {
    let (k, v) = line.split_once(sep)?;
    Some((k.trim(), v.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }

    #[test]
    fn splits_key_values() {
        assert_eq!(
            split_key_value("MemTotal:   8000 kB", ':'),
            Some(("MemTotal", "8000 kB"))
        );
        assert_eq!(split_key_value("no separator", '='), None);
    }
}
