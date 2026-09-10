//! `com.apple.instruments.server.services.networking` (step 4.4): the
//! device pushes interface-detection, connection-detection (pid, addresses,
//! interface) and connection-update (lifetime totals once, then per-interval rx/tx bytes and packets,
//! retransmits, RTT) events. [`NetAggregator`] folds them into per-pid
//! throughput, the number of active connections, retransmits per window
//! and the mean RTT the service reported (no probe of ours), flushed at
//! the medium cadence.
//!
//! RTT units: the update carries `min_rtt`/`avg_rtt` straight from the
//! kernel's TCP statistics, which are milliseconds; [`RTT_DIVISOR`] is the
//! one place to change if a build proves otherwise. The service reports no
//! RSSI or link speed, so `WifiDetails` stays unavailable.

use std::collections::HashMap;
use std::time::Duration;

use idevice::dvt::network_monitor::{NetworkEvent, NetworkMonitorClient};
use statasmon_device::{EventSink, now_ms};
use statasmon_protocol::{MetricId, MetricValue};
use tokio_util::sync::CancellationToken;

use super::{LoopEnd, SharedState};
use crate::tunnel::Tunnel;

/// The DVT channel identifier.
pub const SERVICE: &str = "com.apple.instruments.server.services.networking";
/// `avg_rtt` ÷ this = milliseconds.
pub const RTT_DIVISOR: f64 = 1.0;
/// A connection with no update for this long is no longer "active".
pub const ACTIVE_WINDOW_MS: i64 = 10_000;
/// Connections idle for this long are forgotten.
pub const FORGET_AFTER_MS: i64 = 120_000;
/// RTTs above this are treated as bogus.
pub const MAX_RTT_MS: f64 = 10_000.0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Conn {
    pid: u32,
    interface: u32,
    /// The first update after detection carries the connection's lifetime
    /// totals; every later one carries the bytes, packets and retransmits
    /// of the interval since the previous update (recorded from the iPad
    /// in `fixtures/ios/dvt/iPad13,18-17.4.1/networking_events.plist`:
    /// serial 296 reports 3 012 273 rx bytes, then 260, 63, 71, 71…).
    baselined: bool,
    last_update_ms: i64,
}

/// Interfaces that carry the device's own or our tunnel traffic, not the
/// user's network activity: loopback, utun (VPN and the developer tunnel),
/// anpi (Apple private), ipsec, awdl/llw (peer to peer), bridge/ap.
pub fn is_real_interface(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    !(n.starts_with("lo")
        || n.starts_with("utun")
        || n.starts_with("anpi")
        || n.starts_with("ipsec")
        || n.starts_with("awdl")
        || n.starts_with("llw")
        || n.starts_with("bridge")
        || n.starts_with("ap")
        || n.starts_with("stf")
        || n.starts_with("gif"))
}

/// One flushed window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetSummary {
    /// Bits per second (rx + tx) per pid over the window.
    pub pid_bps: HashMap<u32, f64>,
    /// Connections updated or detected in the last [`ACTIVE_WINDOW_MS`].
    pub connections: usize,
    /// Retransmitted segments in the window.
    pub retransmits: u64,
    /// Mean of the `avg_rtt` values reported in the window, when any.
    pub latency_ms: Option<f64>,
    /// Interface names seen so far (index → name).
    pub interfaces: HashMap<u32, String>,
    /// Device traffic over real interfaces in the window: bytes and
    /// packets received / sent, and totals since the loop started.
    /// `None` until at least one real interface is known.
    pub device: Option<DeviceTraffic>,
}

/// Per-window device traffic over real interfaces (not loopback/tunnels).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceTraffic {
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_pps: f64,
    pub tx_pps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
}

impl NetSummary {
    /// `net.connections`, `net.retransmits`, `net.latency_ms` (when reported).
    pub fn metrics(&self) -> Vec<MetricValue> {
        let mut v = vec![
            MetricValue {
                id: MetricId::NetConnections,
                value: self.connections as f64,
            },
            MetricValue {
                id: MetricId::NetRetransmits,
                value: self.retransmits as f64,
            },
        ];
        if let Some(l) = self.latency_ms {
            v.push(MetricValue {
                id: MetricId::NetLatencyMs,
                value: l,
            });
        }
        if let Some(d) = &self.device {
            v.extend([
                MetricValue {
                    id: MetricId::NetRxBps,
                    value: d.rx_bps,
                },
                MetricValue {
                    id: MetricId::NetTxBps,
                    value: d.tx_bps,
                },
                MetricValue {
                    id: MetricId::NetRxPps,
                    value: d.rx_pps,
                },
                MetricValue {
                    id: MetricId::NetTxPps,
                    value: d.tx_pps,
                },
                MetricValue {
                    id: MetricId::NetRxTotal,
                    value: d.rx_total as f64,
                },
                MetricValue {
                    id: MetricId::NetTxTotal,
                    value: d.tx_total as f64,
                },
            ]);
        }
        v
    }
}

/// Folds networking events into windows.
#[derive(Debug)]
pub struct NetAggregator {
    conns: HashMap<u64, Conn>,
    interfaces: HashMap<u32, String>,
    window_start_ms: i64,
    pid_bytes: HashMap<u32, u64>,
    retx: u64,
    rtt_sum: f64,
    rtt_n: u32,
    /// Updates for serials never announced (counted, not attributed).
    pub orphans: u64,
    /// Window sums over real interfaces: rx bytes, tx bytes, rx packets, tx packets.
    dev_window: [u64; 4],
    /// Totals since start over real interfaces.
    dev_total: [u64; 2],
    /// At least one real interface has been announced.
    real_seen: bool,
}

impl NetAggregator {
    pub fn new(now_ms: i64) -> Self {
        Self {
            conns: HashMap::new(),
            interfaces: HashMap::new(),
            window_start_ms: now_ms,
            pid_bytes: HashMap::new(),
            retx: 0,
            rtt_sum: 0.0,
            rtt_n: 0,
            orphans: 0,
            dev_window: [0; 4],
            dev_total: [0; 2],
            real_seen: false,
        }
    }

    fn is_real(&self, index: u32) -> bool {
        // Unannounced interfaces count as real until told otherwise.
        self.interfaces
            .get(&index)
            .is_none_or(|n| is_real_interface(n))
    }

    /// Known connections (for tests and the capture example).
    pub fn connection_count(&self) -> usize {
        self.conns.len()
    }

    pub fn on_event(&mut self, ev: &NetworkEvent, now_ms: i64) {
        match ev {
            NetworkEvent::InterfaceDetection(i) => {
                if is_real_interface(&i.name) {
                    self.real_seen = true;
                }
                self.interfaces.insert(i.interface_index, i.name.clone());
            }
            NetworkEvent::ConnectionDetection(c) => {
                self.conns.insert(
                    c.serial_number,
                    Conn {
                        pid: c.pid,
                        interface: c.interface_index,
                        baselined: false,
                        last_update_ms: now_ms,
                    },
                );
            }
            NetworkEvent::ConnectionUpdate(u) => {
                let Some(conn) = self.conns.get_mut(&u.connection_serial) else {
                    self.orphans += 1;
                    return;
                };
                conn.last_update_ms = now_ms;
                if !conn.baselined {
                    // Lifetime totals of a connection that may predate the
                    // session: not this window's traffic.
                    conn.baselined = true;
                    let rtt = u.avg_rtt as f64 / RTT_DIVISOR;
                    if rtt > 0.0 && rtt < MAX_RTT_MS {
                        self.rtt_sum += rtt;
                        self.rtt_n += 1;
                    }
                    return;
                }
                let (d_rx, d_tx, d_rxp, d_txp, d_retx) = (
                    u.rx_bytes,
                    u.tx_bytes,
                    u.rx_packets,
                    u.tx_packets,
                    u.tx_retx,
                );
                let (pid, iface) = (conn.pid, conn.interface);
                *self.pid_bytes.entry(pid).or_insert(0) += d_rx + d_tx;
                if self.is_real(iface) {
                    self.dev_window[0] += d_rx;
                    self.dev_window[1] += d_tx;
                    self.dev_window[2] += d_rxp;
                    self.dev_window[3] += d_txp;
                    self.dev_total[0] += d_rx;
                    self.dev_total[1] += d_tx;
                }
                self.retx += d_retx;
                let rtt = u.avg_rtt as f64 / RTT_DIVISOR;
                if rtt > 0.0 && rtt < MAX_RTT_MS {
                    self.rtt_sum += rtt;
                    self.rtt_n += 1;
                }
            }
            NetworkEvent::Unknown(_) => {}
        }
    }

    /// Close the window: rates over its length, active connections now.
    pub fn flush(&mut self, now_ms: i64) -> NetSummary {
        let dt = ((now_ms - self.window_start_ms) as f64 / 1000.0).max(0.001);
        let pid_bps = self
            .pid_bytes
            .drain()
            .map(|(pid, bytes)| (pid, bytes as f64 * 8.0 / dt))
            .collect();
        self.conns
            .retain(|_, c| now_ms - c.last_update_ms <= FORGET_AFTER_MS);
        let connections = self
            .conns
            .values()
            .filter(|c| now_ms - c.last_update_ms <= ACTIVE_WINDOW_MS)
            .count();
        let latency_ms = (self.rtt_n > 0).then(|| self.rtt_sum / f64::from(self.rtt_n));
        let device = self.real_seen.then(|| DeviceTraffic {
            rx_bps: self.dev_window[0] as f64 * 8.0 / dt,
            tx_bps: self.dev_window[1] as f64 * 8.0 / dt,
            rx_pps: self.dev_window[2] as f64 / dt,
            tx_pps: self.dev_window[3] as f64 / dt,
            rx_total: self.dev_total[0],
            tx_total: self.dev_total[1],
        });
        self.dev_window = [0; 4];
        let summary = NetSummary {
            pid_bps,
            connections,
            retransmits: self.retx,
            latency_ms,
            interfaces: self.interfaces.clone(),
            device,
        };
        self.retx = 0;
        self.rtt_sum = 0.0;
        self.rtt_n = 0;
        self.window_start_ms = now_ms;
        summary
    }
}

/// The networking loop: start monitoring, fold events, flush every
/// `medium` into the shared per-pid rates and `Samples`.
pub async fn run(
    tunnel: std::sync::Arc<Tunnel>,
    sink: EventSink,
    device_id: String,
    medium: Duration,
    shared: SharedState,
    cancel: CancellationToken,
) -> LoopEnd {
    let mut hub = match tunnel.dvt().await {
        Ok(h) => h,
        Err(e) => return LoopEnd::TunnelDead(e.to_string()),
    };
    let mut client = match NetworkMonitorClient::new(&mut hub).await {
        Ok(c) => c,
        Err(e) => return LoopEnd::from_error(&tunnel, "networking channel", &e),
    };
    if let Err(e) = client.start_monitoring().await {
        return LoopEnd::from_error(&tunnel, "networking startMonitoring", &e);
    }
    let mut agg = NetAggregator::new(now_ms());
    let period = medium.max(Duration::from_secs(1));
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = client.stop_monitoring().await;
                return LoopEnd::Cancelled;
            }
            _ = interval.tick() => {
                if sink.is_closed() {
                    return LoopEnd::SinkClosed;
                }
                let ts = now_ms();
                let summary = agg.flush(ts);
                {
                    let mut sh = shared.write();
                    sh.pid_net_bps = summary.pid_bps.clone();
                    if summary.device.is_some() {
                        sh.net_from_networking = true;
                    }
                }
                if sink.samples(&device_id, ts, summary.metrics()).await.is_err() {
                    return LoopEnd::SinkClosed;
                }
            }
            ev = client.next_event() => match ev {
                Ok(ev) => agg.on_event(&ev, now_ms()),
                Err(e) => return LoopEnd::from_error(&tunnel, "networking read", &e),
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use idevice::dvt::network_monitor::{
        ConnectionDetectionEvent, ConnectionUpdateEvent, InterfaceDetectionEvent,
    };

    fn detect(serial: u64, pid: u32) -> NetworkEvent {
        NetworkEvent::ConnectionDetection(ConnectionDetectionEvent {
            local_address: None,
            remote_address: None,
            interface_index: 1,
            pid,
            recv_buffer_size: 0,
            recv_buffer_used: 0,
            serial_number: serial,
            kind: 1,
        })
    }

    fn update(serial: u64, rx: u64, tx: u64, retx: u64, rtt: u64) -> NetworkEvent {
        NetworkEvent::ConnectionUpdate(ConnectionUpdateEvent {
            rx_packets: 0,
            rx_bytes: rx,
            tx_packets: 0,
            tx_bytes: tx,
            rx_dups: 0,
            rx_ooo: 0,
            tx_retx: retx,
            min_rtt: rtt,
            avg_rtt: rtt,
            connection_serial: serial,
            time: 0,
        })
    }

    #[test]
    fn per_pid_rates_retransmits_and_rtt() {
        let mut agg = NetAggregator::new(0);
        agg.on_event(
            &NetworkEvent::InterfaceDetection(InterfaceDetectionEvent {
                interface_index: 1,
                name: "en0".into(),
            }),
            0,
        );
        agg.on_event(&detect(1, 100), 0);
        agg.on_event(&detect(2, 200), 0);
        // First updates carry lifetime totals: nothing attributed yet.
        agg.on_event(&update(1, 1000, 500, 2, 20), 100);
        agg.on_event(&update(2, 0, 0, 0, 0), 100);
        // Later updates carry the interval's bytes as they are.
        agg.on_event(&update(1, 2000, 1000, 3, 30), 900);
        agg.on_event(&update(2, 4000, 0, 0, 10), 900);
        // Unknown serial: counted, not attributed.
        agg.on_event(&update(9, 10, 10, 1, 1), 900);
        let s = agg.flush(2000);
        assert_eq!(s.pid_bps.get(&100).copied(), Some(3000.0 * 8.0 / 2.0));
        assert_eq!(s.pid_bps.get(&200).copied(), Some(4000.0 * 8.0 / 2.0));
        assert_eq!(s.connections, 2);
        assert_eq!(s.retransmits, 3);
        let dev = s.device.as_ref().unwrap();
        assert_eq!(dev.rx_bps, 6000.0 * 8.0 / 2.0);
        assert_eq!(dev.rx_total, 6000);
        // RTTs reported: 20, 30, 10 (zeros ignored).
        assert_eq!(s.latency_ms, Some(20.0));
        assert_eq!(s.interfaces.get(&1).map(String::as_str), Some("en0"));
        assert_eq!(agg.orphans, 1);
        let m = s.metrics();
        assert!(
            m.iter()
                .any(|v| v.id == MetricId::NetConnections && v.value == 2.0)
        );
        assert!(m.iter().any(|v| v.id == MetricId::NetLatencyMs));

        // A fresh window: nothing attributed, retransmits reset, RTT absent.
        let s2 = agg.flush(4000);
        assert!(s2.pid_bps.is_empty());
        assert_eq!(s2.retransmits, 0);
        assert_eq!(s2.latency_ms, None);
        // Connections + retransmits, plus the six device-traffic values (all
        // zero rates now) because en0 is a real interface.
        assert_eq!(s2.metrics().len(), 8);
        assert!(
            s2.metrics()
                .iter()
                .any(|v| v.id == MetricId::NetRxBps && v.value == 0.0)
        );
        assert_eq!(s2.connections, 2);
        // Idle connections leave the active count, then are forgotten.
        let s3 = agg.flush(900 + ACTIVE_WINDOW_MS + 1);
        assert_eq!(s3.connections, 0);
        assert_eq!(agg.connection_count(), 2);
        agg.flush(900 + FORGET_AFTER_MS + 1);
        assert_eq!(agg.connection_count(), 0);
    }
}
