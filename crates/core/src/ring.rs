//! Fixed-capacity ring buffers for the live UI: one typed pair of deques
//! per metric, plus a ring of per-core ticks.

use std::collections::{BTreeMap, VecDeque};

use statasmon_protocol::{CoreSample, HistoryPoint, MetricId, MetricValue};

/// Timestamps and values of one metric.
#[derive(Debug, Clone, Default)]
struct MetricRing {
    ts: VecDeque<i64>,
    values: VecDeque<f64>,
}

impl MetricRing {
    fn push(&mut self, capacity: usize, ts_ms: i64, value: f64) {
        while self.ts.len() >= capacity.max(1) {
            self.ts.pop_front();
            self.values.pop_front();
        }
        self.ts.push_back(ts_ms);
        self.values.push_back(value);
    }

    fn latest(&self) -> Option<(i64, f64)> {
        Some((*self.ts.back()?, *self.values.back()?))
    }
}

/// Per-device metric history with downsampling queries.
#[derive(Debug, Clone)]
pub struct RingBuffer {
    capacity: usize,
    metrics: BTreeMap<MetricId, MetricRing>,
    samples: u64,
}

impl RingBuffer {
    /// A buffer keeping `capacity` samples per metric.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            metrics: BTreeMap::new(),
            samples: 0,
        }
    }

    /// Samples kept per metric.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Ticks pushed since creation (not bounded by capacity).
    pub fn sample_count(&self) -> u64 {
        self.samples
    }

    /// Store one tick.
    pub fn push(&mut self, ts_ms: i64, values: &[MetricValue]) {
        for v in values {
            self.metrics
                .entry(v.id)
                .or_default()
                .push(self.capacity, ts_ms, v.value);
        }
        self.samples += 1;
    }

    /// Number of stored points for `metric`.
    pub fn len(&self, metric: MetricId) -> usize {
        self.metrics.get(&metric).map_or(0, |m| m.ts.len())
    }

    /// True when no metric has any point.
    pub fn is_empty(&self) -> bool {
        self.metrics.values().all(|m| m.ts.is_empty())
    }

    /// Metrics that have at least one point, in catalog order.
    pub fn metrics(&self) -> Vec<MetricId> {
        self.metrics
            .iter()
            .filter(|(_, m)| !m.ts.is_empty())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Newest value of every metric.
    pub fn latest(&self) -> Vec<MetricValue> {
        self.metrics
            .iter()
            .filter_map(|(id, m)| m.latest().map(|(_, value)| MetricValue { id: *id, value }))
            .collect()
    }

    /// Newest `(ts_ms, value)` of `metric`.
    pub fn latest_value(&self, metric: MetricId) -> Option<(i64, f64)> {
        self.metrics.get(&metric)?.latest()
    }

    /// Points in `[from_ms, to_ms]`. With `step_ms > 0` the points are
    /// averaged per bucket of `step_ms` starting at `from_ms`; each bucket
    /// is stamped with its start. `step_ms == 0` returns raw points.
    pub fn history(
        &self,
        metric: MetricId,
        from_ms: i64,
        to_ms: i64,
        step_ms: u32,
    ) -> Vec<HistoryPoint> {
        let Some(m) = self.metrics.get(&metric) else {
            return Vec::new();
        };
        let raw =
            m.ts.iter()
                .zip(m.values.iter())
                .filter(|(ts, _)| **ts >= from_ms && **ts <= to_ms);
        if step_ms == 0 {
            return raw
                .map(|(ts, v)| HistoryPoint {
                    ts_ms: *ts,
                    value: *v,
                })
                .collect();
        }
        let step = i64::from(step_ms);
        let mut out: Vec<HistoryPoint> = Vec::new();
        let mut bucket: Option<(i64, f64, u32)> = None;
        for (ts, v) in raw {
            let start = from_ms + (ts - from_ms).div_euclid(step) * step;
            match bucket {
                Some((b, sum, n)) if b == start => bucket = Some((b, sum + v, n + 1)),
                Some((b, sum, n)) => {
                    out.push(HistoryPoint {
                        ts_ms: b,
                        value: sum / f64::from(n),
                    });
                    bucket = Some((start, *v, 1));
                }
                None => bucket = Some((start, *v, 1)),
            }
        }
        if let Some((b, sum, n)) = bucket {
            out.push(HistoryPoint {
                ts_ms: b,
                value: sum / f64::from(n),
            });
        }
        out
    }

    /// Change of a counter over the last `window_ms` ending at `now_ms`
    /// (newest value minus the oldest value inside the window). `None`
    /// with fewer than two points in the window.
    pub fn delta_over(&self, metric: MetricId, window_ms: i64, now_ms: i64) -> Option<f64> {
        let m = self.metrics.get(&metric)?;
        let from = now_ms - window_ms;
        let mut first = None;
        let mut last = None;
        for (ts, v) in m.ts.iter().zip(m.values.iter()) {
            if *ts < from || *ts > now_ms {
                continue;
            }
            if first.is_none() {
                first = Some(*v);
            }
            last = Some(*v);
        }
        match (first, last) {
            (Some(a), Some(b))
                if m.ts.iter().filter(|t| **t >= from && **t <= now_ms).count() >= 2 =>
            {
                Some(b - a)
            }
            _ => None,
        }
    }
}

/// Ring of per-core ticks.
#[derive(Debug, Clone)]
pub struct CoreRing {
    capacity: usize,
    ticks: VecDeque<(i64, Vec<CoreSample>)>,
}

impl CoreRing {
    /// A ring keeping `capacity` ticks.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            ticks: VecDeque::new(),
        }
    }

    /// Store one tick.
    pub fn push(&mut self, ts_ms: i64, cores: Vec<CoreSample>) {
        while self.ticks.len() >= self.capacity {
            self.ticks.pop_front();
        }
        self.ticks.push_back((ts_ms, cores));
    }

    /// Ticks stored.
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    /// The newest tick.
    pub fn latest(&self) -> Option<&(i64, Vec<CoreSample>)> {
        self.ticks.back()
    }

    /// Ticks in `[from_ms, to_ms]`.
    pub fn history(&self, from_ms: i64, to_ms: i64) -> Vec<(i64, Vec<CoreSample>)> {
        self.ticks
            .iter()
            .filter(|(ts, _)| *ts >= from_ms && *ts <= to_ms)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use statasmon_protocol::CoreKind;

    fn mv(id: MetricId, value: f64) -> MetricValue {
        MetricValue { id, value }
    }

    #[test]
    fn capacity_is_enforced_per_metric() {
        let mut r = RingBuffer::new(3);
        for i in 0..5 {
            r.push(i * 1000, &[mv(MetricId::CpuTotal, i as f64)]);
        }
        assert_eq!(r.len(MetricId::CpuTotal), 3);
        assert_eq!(r.sample_count(), 5);
        let pts = r.history(MetricId::CpuTotal, 0, 10_000, 0);
        assert_eq!(
            pts.iter().map(|p| p.value).collect::<Vec<_>>(),
            vec![2.0, 3.0, 4.0]
        );
        assert_eq!(r.latest_value(MetricId::CpuTotal), Some((4000, 4.0)));
        assert_eq!(r.latest().len(), 1);
        assert_eq!(r.len(MetricId::MemUsed), 0);
    }

    #[test]
    fn history_downsamples_by_average_per_step() {
        let mut r = RingBuffer::new(100);
        for i in 0..10 {
            r.push(i * 1000, &[mv(MetricId::CpuTotal, i as f64)]);
        }
        let pts = r.history(MetricId::CpuTotal, 0, 9000, 4000);
        assert_eq!(pts.len(), 3);
        assert_eq!(
            pts[0],
            HistoryPoint {
                ts_ms: 0,
                value: 1.5
            }
        );
        assert_eq!(
            pts[1],
            HistoryPoint {
                ts_ms: 4000,
                value: 5.5
            }
        );
        assert_eq!(
            pts[2],
            HistoryPoint {
                ts_ms: 8000,
                value: 8.5
            }
        );
        let window = r.history(MetricId::CpuTotal, 2000, 5000, 0);
        assert_eq!(window.len(), 4);
        assert!(r.history(MetricId::MemUsed, 0, 9000, 1000).is_empty());
    }

    #[test]
    fn delta_over_window_uses_first_and_last_point() {
        let mut r = RingBuffer::new(100);
        for i in 0..70 {
            r.push(i * 1000, &[mv(MetricId::NetRetransmits, (i * 2) as f64)]);
        }
        assert_eq!(
            r.delta_over(MetricId::NetRetransmits, 60_000, 69_000),
            Some(120.0)
        );
        assert_eq!(r.delta_over(MetricId::NetRetransmits, 500, 69_000), None);
        assert_eq!(r.delta_over(MetricId::NetDropped, 60_000, 69_000), None);
    }

    #[test]
    fn core_ring_keeps_last_n() {
        let mut c = CoreRing::new(2);
        let sample = |i| {
            vec![CoreSample {
                index: 0,
                kind: CoreKind::Performance,
                usage_pct: i as f32,
                freq_mhz: None,
                online: true,
            }]
        };
        c.push(1, sample(1));
        c.push(2, sample(2));
        c.push(3, sample(3));
        assert_eq!(c.len(), 2);
        assert_eq!(c.latest().unwrap().0, 3);
        assert_eq!(c.history(2, 3).len(), 2);
        assert_eq!(c.history(0, 1).len(), 0);
    }
}
