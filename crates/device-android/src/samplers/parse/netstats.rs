//! `dumpsys netstats detail` — per-UID byte counters from the `UID stats:`
//! section (all interfaces, all sets, tag 0x0, every bucket since boot).

use std::collections::HashMap;

use super::ParseError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UidBytes {
    pub rx: u64,
    pub tx: u64,
}

/// Sum of `rb=`/`tb=` over every history bucket of every `uid=N … tag=0x0`
/// entry between `UID stats:` and `UID tag stats:`. Tagged entries live
/// in a later section and would double count, so they are not read.
pub fn parse_uid_bytes(text: &str) -> Result<HashMap<u32, UidBytes>, ParseError> {
    let mut out: HashMap<u32, UidBytes> = HashMap::new();
    let mut in_uid = false;
    let mut current: Option<u32> = None;
    let mut saw_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("UID stats:") {
            in_uid = true;
            saw_section = true;
            continue;
        }
        if !in_uid {
            continue;
        }
        if t.starts_with("UID tag stats:") || (t.ends_with(':') && !t.contains("ident=")) {
            if t.starts_with("UID tag stats:") {
                break;
            }
            // "Pending bytes:", "History since boot:" etc. stay inside the section.
            continue;
        }
        if t.contains("ident=") {
            current = None;
            let uid = field(t, "uid=").and_then(|v| v.parse::<i64>().ok());
            let tag = field(t, "tag=").unwrap_or("0x0");
            if let Some(uid) = uid
                && uid >= 0
                && tag == "0x0"
            {
                current = Some(uid as u32);
            }
            continue;
        }
        if let Some(uid) = current
            && t.starts_with("st=")
        {
            let rb = field(t, "rb=").and_then(|v| v.parse::<u64>().ok());
            let tb = field(t, "tb=").and_then(|v| v.parse::<u64>().ok());
            if let (Some(rb), Some(tb)) = (rb, tb) {
                let e = out.entry(uid).or_default();
                e.rx += rb;
                e.tx += tb;
            }
        }
    }
    if !saw_section {
        return Err(ParseError::Missing("UID stats"));
    }
    Ok(out)
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == ',')
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn uid_bytes_fixture() {
        let m = parse_uid_bytes(&fixture("dumpsys_netstats_detail")).unwrap();
        // uid 1000 (system) has three buckets on the default network plus
        // whatever the other idents add; the first three sum as below.
        let sys = m[&1000];
        assert!(sys.rx >= 705_971 + 84_616 + 117_284, "{sys:?}");
        assert!(sys.tx >= 46_496 + 9_057 + 39_865, "{sys:?}");
        assert!(m.contains_key(&1073));
        assert!(!m.contains_key(&u32::MAX), "negative uids are skipped");
        assert!(m.len() > 10);
        assert!(parse_uid_bytes("Dev stats:\n").is_err());
    }
}
