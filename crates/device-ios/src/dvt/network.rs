//! `com.apple.instruments.server.services.networking` (step 4.4): the
//! device pushes interface-detection, connection-detection (pid, addresses,
//! interface) and connection-update (cumulative rx/tx bytes and packets,
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
    rx_bytes: Option<u64>,
    tx_bytes: Option<u64>,
    tx_retx: Option<u64>,
    last_update_ms: i64,
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
        }
    }

    /// Known connections (for tests and the capture example).
    pub fn connection_count(&self) -> usize {
        self.conns.len()
    }

    pub fn on_event(&mut self, ev: &NetworkEvent, now_ms: i64) {
        match ev {
            NetworkEvent::InterfaceDetection(i) => {
                self.interfaces.insert(i.interface_index, i.name.clone());
            }
            NetworkEvent::ConnectionDetection(c) => {
                self.conns.insert(
                    c.serial_number,
                    Conn {
                        pid: c.pid,
                        interface: c.interface_index,
                        rx_bytes: None,
                        tx_bytes: None,
                        tx_retx: None,
                        last_update_ms: now_ms,
                    },
                );
            }
            NetworkEvent::ConnectionUpdate(u) => {
                let Some(conn) = self.conns.get_mut(&u.connection_serial) else {
                    self.orphans += 1;
                    return;
                };
                let d_rx = conn.rx_bytes.map_or(0, |p| u.rx_bytes.saturating_sub(p));
                let d_tx = conn.tx_bytes.map_or(0, |p| u.tx_bytes.saturating_sub(p));
                let d_retx = conn.tx_retx.map_or(0, |p| u.tx_retx.saturating_sub(p));
                conn.rx_bytes = Some(u.rx_bytes);
                conn.tx_bytes = Some(u.tx_bytes);
                conn.tx_retx = Some(u.tx_retx);
                conn.last_update_ms = now_ms;
                *self.pid_bytes.entry(conn.pid).or_insert(0) += d_rx + d_tx;
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
        let summary = NetSummary {
            pid_bps,
            connections,
            retransmits: self.retx,
            latency_ms,
            interfaces: self.interfaces.clone(),
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
                shared.write().pid_net_bps = summary.pid_bps.clone();
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
        // First updates establish the baseline: no bytes attributed yet.
        agg.on_event(&update(1, 1000, 500, 2, 20), 100);
        agg.on_event(&update(2, 0, 0, 0, 0), 100);
        // Second updates carry deltas.
        agg.on_event(&update(1, 3000, 1500, 5, 30), 900);
        agg.on_event(&update(2, 4000, 0, 0, 10), 900);
        // Unknown serial: counted, not attributed.
        agg.on_event(&update(9, 10, 10, 1, 1), 900);
        let s = agg.flush(2000);
        assert_eq!(s.pid_bps.get(&100).copied(), Some(3000.0 * 8.0 / 2.0));
        assert_eq!(s.pid_bps.get(&200).copied(), Some(4000.0 * 8.0 / 2.0));
        assert_eq!(s.connections, 2);
        assert_eq!(s.retransmits, 3);
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
        assert_eq!(s2.metrics().len(), 2);
        assert_eq!(s2.connections, 2);
        // Idle connections leave the active count, then are forgotten.
        let s3 = agg.flush(900 + ACTIVE_WINDOW_MS + 1);
        assert_eq!(s3.connections, 0);
        assert_eq!(agg.connection_count(), 2);
        agg.flush(900 + FORGET_AFTER_MS + 1);
        assert_eq!(agg.connection_count(), 0);
    }
}
