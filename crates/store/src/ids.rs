//! Session ids and file-name sanitising.

use chrono::{Local, TimeZone, Utc};

/// Keep `[A-Za-z0-9._-]`; everything else becomes `_`. Never empty.
pub fn sanitize(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.is_empty() {
        out.push_str("device");
    }
    out
}

/// `<device_id sanitised>-<yyyyMMdd-HHmmss>` in local time.
pub fn session_id_for(device_id: &str, now_ms: i64) -> String {
    let stamp = Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|t| t.format("%Y%m%d-%H%M%S").to_string())
        .or_else(|| {
            Utc.timestamp_millis_opt(now_ms)
                .single()
                .map(|t| t.format("%Y%m%d-%H%M%S").to_string())
        })
        .unwrap_or_else(|| now_ms.to_string());
    format!("{}-{stamp}", sanitize(device_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitises_ids() {
        assert_eq!(sanitize("fake:iphone"), "fake_iphone");
        assert_eq!(sanitize("ios:00008030-001A/x"), "ios_00008030-001A_x");
        assert_eq!(sanitize(""), "device");
        assert_eq!(sanitize("..hidden"), "hidden");
    }

    #[test]
    fn session_id_has_device_and_stamp() {
        let id = session_id_for("android:R52X", 1_757_419_927_000);
        let stamp = id.strip_prefix("android_R52X-").unwrap_or_default();
        assert_eq!(stamp.len(), 15, "{id}");
        assert!(stamp[..8].chars().all(|c| c.is_ascii_digit()), "{id}");
        assert_eq!(&stamp[8..9], "-");
        assert!(stamp[9..].chars().all(|c| c.is_ascii_digit()), "{id}");
    }
}
