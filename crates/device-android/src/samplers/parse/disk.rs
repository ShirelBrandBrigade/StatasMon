//! `dumpsys diskstats` and `df -k /data`.

use super::{ParseError, parse_u64};

/// Category sizes in bytes. `dumpsys diskstats` prints them straight from
/// the last `StorageStatsService` run, which is a periodic job: the values
/// can be hours old. They are emitted anyway; the UI shows sample age.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiskStats {
    pub app: Option<u64>,
    pub app_data: Option<u64>,
    pub app_cache: Option<u64>,
    pub photos: Option<u64>,
    pub videos: Option<u64>,
    pub audio: Option<u64>,
    pub downloads: Option<u64>,
    pub system: Option<u64>,
    pub other: Option<u64>,
    /// `Data-Free: <free>K / <total>K` → bytes, a fallback for `df`.
    pub data_free: Option<u64>,
    pub data_total: Option<u64>,
}

impl DiskStats {
    /// `disk.apps` = App + App Data + App Cache (only when all three exist).
    pub fn apps(&self) -> Option<u64> {
        Some(self.app? + self.app_data? + self.app_cache?)
    }
    /// `disk.media` = Photos + Videos + Audio + Downloads.
    pub fn media(&self) -> Option<u64> {
        Some(self.photos? + self.videos? + self.audio? + self.downloads?)
    }
}

pub fn parse_diskstats(text: &str) -> Result<DiskStats, ParseError> {
    let mut d = DiskStats::default();
    let mut any = false;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        let size = || v.parse::<u64>().ok();
        match k.trim() {
            "App Size" => d.app = size(),
            "App Data Size" => d.app_data = size(),
            "App Cache Size" => d.app_cache = size(),
            "Photos Size" => d.photos = size(),
            "Videos Size" => d.videos = size(),
            "Audio Size" => d.audio = size(),
            "Downloads Size" => d.downloads = size(),
            "System Size" => d.system = size(),
            "Other Size" => d.other = size(),
            "Data-Free" => {
                // `81151464K / 110594012K total = 73% free`
                let mut it = v.split_whitespace();
                d.data_free = it.next().and_then(kib);
                let _slash = it.next();
                d.data_total = it.next().and_then(kib);
            }
            _ => continue,
        }
        any = true;
    }
    if !any {
        return Err(ParseError::Empty("diskstats"));
    }
    Ok(d)
}

fn kib(s: &str) -> Option<u64> {
    s.strip_suffix('K')
        .and_then(|n| n.parse::<u64>().ok())
        .map(|k| k * 1024)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DfRow {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

/// The row for `mount` (e.g. `/data`) from `df -k` output; sizes in bytes.
pub fn parse_df(text: &str, mount: &str) -> Result<DfRow, ParseError> {
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 || cols[5] != mount {
            continue;
        }
        return Ok(DfRow {
            total: parse_u64("df 1K-blocks", cols[1])? * 1024,
            used: parse_u64("df Used", cols[2])? * 1024,
            free: parse_u64("df Available", cols[3])? * 1024,
        });
    }
    Err(ParseError::Missing("df mount row"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn diskstats_fixture() {
        let d = parse_diskstats(&fixture("dumpsys_diskstats")).unwrap();
        assert_eq!(
            d.apps(),
            Some(13_009_826_304 + 9_661_345_814 + 4_793_561_600)
        );
        assert_eq!(d.media(), Some(26_058_752 + 1_033_244_672 + 6_225_920));
        assert_eq!(d.system, Some(24_190_685_184));
        assert_eq!(d.other, Some(5_600_534_528));
        assert_eq!(d.data_free, Some(81_151_464 * 1024));
        assert_eq!(d.data_total, Some(110_594_012 * 1024));
    }

    #[test]
    fn df_data_row() {
        let r = parse_df(&fixture("df"), "/data").unwrap();
        assert_eq!(r.total, 110_594_012 * 1024);
        assert_eq!(r.used, 29_311_476 * 1024);
        assert_eq!(r.free, 81_151_464 * 1024);
        assert!(parse_df(&fixture("df"), "/nope").is_err());
    }
}
