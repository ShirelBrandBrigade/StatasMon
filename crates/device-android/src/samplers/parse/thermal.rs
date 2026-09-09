//! `dumpsys thermalservice`.

use super::{ParseError, parse_f64};

/// `android.hardware.thermal.TemperatureType`.
pub const TYPE_CPU: i32 = 0;
pub const TYPE_GPU: i32 = 1;
pub const TYPE_BATTERY: i32 = 2;
pub const TYPE_SKIN: i32 = 3;
pub const TYPE_DISPLAY: i32 = 11;

/// Android names for `Thermal Status` 0–6.
pub const STATUS_NAMES: [&str; 7] = [
    "None",
    "Light",
    "Moderate",
    "Severe",
    "Critical",
    "Emergency",
    "Shutdown",
];

pub fn status_name(raw: u32) -> &'static str {
    STATUS_NAMES.get(raw as usize).copied().unwrap_or("Unknown")
}

/// Common 0–3 severity for `thermal.state`: 0 → 0, 1–2 → 1, 3–4 → 2, 5–6 → 3.
pub fn common_state(raw: u32) -> f64 {
    match raw {
        0 => 0.0,
        1 | 2 => 1.0,
        3 | 4 => 2.0,
        _ => 3.0,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Temperature {
    pub value_c: f64,
    pub kind: i32,
    pub name: String,
    pub status: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Thermal {
    pub status: u32,
    /// From `Current temperatures from HAL:` when present, else the
    /// `Cached temperatures:` block. Entries reading 0.0 are dropped: the
    /// HAL uses 0.0 for sensors it does not report (SUBBAT, USB here).
    pub temps: Vec<Temperature>,
    pub from_cache: bool,
}

impl Thermal {
    /// Highest reading of one sensor type, if any.
    pub fn max_of(&self, kind: i32) -> Option<f64> {
        self.temps
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.value_c)
            .fold(None, |m, v| Some(m.map_or(v, |m: f64| m.max(v))))
    }
}

pub fn parse_thermalservice(text: &str) -> Result<Thermal, ParseError> {
    let mut status = None;
    let mut hal = Vec::new();
    let mut cached = Vec::new();
    let mut section = 0u8; // 1 cached, 2 hal, 0 other
    for line in text.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Thermal Status:") {
            status = Some(
                v.trim()
                    .parse::<u32>()
                    .map_err(|_| ParseError::invalid("Thermal Status", v.trim()))?,
            );
            section = 0;
        } else if t.starts_with("Cached temperatures:") {
            section = 1;
        } else if t.starts_with("Current temperatures from HAL:") {
            section = 2;
        } else if t.starts_with("Temperature{") {
            if let Some(temp) = parse_temperature(t)? {
                match section {
                    1 => cached.push(temp),
                    2 => hal.push(temp),
                    _ => {}
                }
            }
        } else if !t.starts_with("Temperature{") && t.ends_with(':') {
            section = 0;
        }
    }
    let status = status.ok_or(ParseError::Missing("Thermal Status"))?;
    let (temps, from_cache) = if hal.is_empty() {
        (cached, true)
    } else {
        (hal, false)
    };
    Ok(Thermal {
        status,
        temps,
        from_cache,
    })
}

/// `Temperature{mValue=30.2, mType=0, mName=AP, mStatus=0}`; `None` for
/// the 0.0 "not reported" placeholder.
fn parse_temperature(t: &str) -> Result<Option<Temperature>, ParseError> {
    let inner = t.trim_start_matches("Temperature{").trim_end_matches('}');
    let mut value = None;
    let mut kind = None;
    let mut name = None;
    let mut status = 0;
    for part in inner.split(',') {
        let Some((k, v)) = part.trim().split_once('=') else {
            continue;
        };
        match k {
            "mValue" => value = Some(parse_f64("mValue", v)?),
            "mType" => kind = v.parse().ok(),
            "mName" => name = Some(v.to_owned()),
            "mStatus" => status = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    let value = value.ok_or(ParseError::Missing("mValue"))?;
    let kind = kind.ok_or(ParseError::Missing("mType"))?;
    if value == 0.0 {
        return Ok(None);
    }
    Ok(Some(Temperature {
        value_c: value,
        kind,
        name: name.unwrap_or_default(),
        status,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn thermal_fixture() {
        let t = parse_thermalservice(&fixture("dumpsys_thermalservice")).unwrap();
        assert_eq!(t.status, 0);
        assert!(!t.from_cache);
        assert_eq!(t.temps.len(), 4, "zero readings dropped: {:?}", t.temps);
        assert_eq!(t.max_of(TYPE_CPU), Some(30.2));
        assert_eq!(t.max_of(TYPE_BATTERY), Some(32.8));
        assert_eq!(t.max_of(TYPE_SKIN), Some(33.9));
        assert_eq!(t.max_of(TYPE_DISPLAY), None);
        assert_eq!(t.max_of(TYPE_GPU), None);
    }

    #[test]
    fn falls_back_to_cache() {
        let text = "Thermal Status: 2\nCached temperatures:\n\tTemperature{mValue=41.0, mType=3, mName=SKIN, mStatus=0}\nHAL Ready: false\n";
        let t = parse_thermalservice(text).unwrap();
        assert!(t.from_cache);
        assert_eq!(t.max_of(TYPE_SKIN), Some(41.0));
        assert_eq!(common_state(t.status), 1.0);
        assert_eq!(status_name(t.status), "Moderate");
        assert_eq!(common_state(4), 2.0);
        assert_eq!(common_state(6), 3.0);
    }
}
