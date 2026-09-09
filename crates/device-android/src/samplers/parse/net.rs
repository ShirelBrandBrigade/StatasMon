//! `/proc/net/dev` and `/proc/net/tcp*` (established connection count).

use super::{ParseError, parse_u64};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iface {
    pub name: String,
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub rx_drop: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub tx_drop: u64,
}

pub fn parse_net_dev(text: &str) -> Result<Vec<Iface>, ParseError> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.contains(' ') {
            continue; // header lines
        }
        let cols: Vec<u64> = rest
            .split_whitespace()
            .map(|t| parse_u64("net/dev column", t))
            .collect::<Result<_, _>>()?;
        if cols.len() < 12 {
            return Err(ParseError::invalid("net/dev line", line));
        }
        out.push(Iface {
            name: name.to_owned(),
            rx_bytes: cols[0],
            rx_packets: cols[1],
            rx_drop: cols[3],
            tx_bytes: cols[8],
            tx_packets: cols[9],
            tx_drop: cols[11],
        });
    }
    if out.is_empty() {
        return Err(ParseError::Empty("net/dev"));
    }
    Ok(out)
}

/// Device-wide counters: every interface except loopback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetTotals {
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub dropped: u64,
}

pub fn totals(ifaces: &[Iface]) -> NetTotals {
    let mut t = NetTotals::default();
    for i in ifaces.iter().filter(|i| i.name != "lo") {
        t.rx_bytes += i.rx_bytes;
        t.rx_packets += i.rx_packets;
        t.tx_bytes += i.tx_bytes;
        t.tx_packets += i.tx_packets;
        t.dropped += i.rx_drop + i.tx_drop;
    }
    t
}

/// Count sockets in state `01` (ESTABLISHED) across concatenated
/// `/proc/net/tcp` and `/proc/net/tcp6` dumps. Header lines are skipped.
pub fn count_established(text: &str) -> Option<u32> {
    let mut saw_rows = false;
    let mut n = 0u32;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(sl) = it.next() else { continue };
        if !sl.ends_with(':') {
            continue;
        }
        saw_rows = true;
        let (_local, _remote, st) = (it.next(), it.next(), it.next());
        if st == Some("01") {
            n += 1;
        }
    }
    saw_rows.then_some(n)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn net_dev_totals_exclude_lo() {
        let a = parse_net_dev(&fixture("proc_net_dev_1")).unwrap();
        let wlan = a.iter().find(|i| i.name == "wlan0").unwrap();
        assert_eq!(wlan.rx_bytes, 536_689_966);
        assert_eq!(wlan.tx_packets, 98_490);
        let t = totals(&a);
        assert_eq!(t.rx_bytes, 536_689_966);
        assert_eq!(t.tx_bytes, 20_612_195 + 910);
        assert_eq!(t.dropped, 0);
        let b = parse_net_dev(&fixture("proc_net_dev_2")).unwrap();
        assert_eq!(totals(&b).rx_bytes, t.rx_bytes, "idle second: no delta");
    }

    #[test]
    fn established_count() {
        let text = "  sl  local_address rem_address   st tx_queue\n\
                    \x20  0: D532A8C0:B43E 2E4BFA8E:01BB 08 00000000:00000082 00:00000000\n\
                    \x20  1: D532A8C0:CE0E 139B6652:01BB 01 00000000:00000000 00:00000000\n\
                    \x20  sl  local_address rem_address   st\n\
                    \x20  0: 0000000000000000FFFF0000D532A8C0:CE0E 00000000000000000000FFFF139B6652:01BB 01 0\n";
        assert_eq!(count_established(text), Some(2));
        assert_eq!(count_established(""), None);
    }
}
