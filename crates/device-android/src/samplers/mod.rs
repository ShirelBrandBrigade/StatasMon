//! Cadence-driven samplers (steps 2.3–2.5). Each sampler owns its parsers
//! (pure functions with fixture tests) and its delta state.
//!
//! Every tick is ONE batched `sh` invocation with sentinel lines between
//! sections (see [`parse::split_sections`]); a sampler skips every section
//! whose [`Probe`] flag is false and never emits a metric it could not
//! measure. Rates are emitted from the second tick on.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use statasmon_device::{EventSink, SinkClosed};

use crate::probe::Probe;
use crate::shell::{Shell, ShellError};

pub mod fast;
pub mod medium;
pub mod parse;
pub mod slow;

pub use fast::FastSampler;
pub use medium::MediumSampler;
pub use slow::SlowSampler;

/// Which timer drives a sampler (`SamplingConfig` fast/medium/slow/heavy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Fast,
    Medium,
    Slow,
    Heavy,
}

/// One sampler: on every tick it runs its commands through the shell,
/// parses, and emits `Message`s via the sink.
#[async_trait]
pub trait Sampler: Send + std::fmt::Debug {
    fn name(&self) -> &'static str;
    fn cadence(&self) -> Cadence;
    async fn tick(
        &mut self,
        shell: &dyn Shell,
        sink: &EventSink,
        device_id: &str,
        now_ms: i64,
    ) -> Result<(), ShellError>;
}

/// pid → process name, refreshed by the medium sampler from `ps` and read
/// by the logcat stream to fill `LogEntry.process`.
pub type PidTable = Arc<RwLock<HashMap<u32, String>>>;

/// uid → current network rate in bits/s, written by the slow sampler from
/// `dumpsys netstats` deltas and joined onto `ProcessRow.net_bps` by the
/// medium sampler.
pub type UidNetTable = Arc<RwLock<HashMap<u32, f64>>>;

/// State shared between samplers (and the logcat stream).
#[derive(Debug, Clone, Default)]
pub struct Shared {
    pub pids: PidTable,
    pub uid_net: UidNetTable,
}

/// Build the samplers this device supports (per the probe). The source
/// spawns one loop per cadence and calls `tick` sequentially within it.
pub fn build(probe: &Probe) -> Vec<Box<dyn Sampler>> {
    build_shared(probe, &Shared::default())
}

/// Like [`build`], with explicit shared tables so the caller can hand the
/// same [`PidTable`] to `logcat::run` via `LogcatConfig::pids`.
pub fn build_shared(probe: &Probe, shared: &Shared) -> Vec<Box<dyn Sampler>> {
    let mut out: Vec<Box<dyn Sampler>> = Vec::new();
    if let Some(s) = FastSampler::new(probe) {
        out.push(Box::new(s));
    }
    if let Some(s) = MediumSampler::new(probe, shared.clone()) {
        out.push(Box::new(s));
    }
    if let Some(s) = SlowSampler::new(probe, shared.clone()) {
        out.push(Box::new(s));
    }
    out
}

/// The engine dropped its receiver: surface it as a shell error so the
/// cadence loop stops (there is nobody left to sample for).
pub(crate) fn sink_closed(_: SinkClosed) -> ShellError {
    ShellError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "event sink closed",
    ))
}

/// Builds the batched command for one tick.
#[derive(Debug, Default, Clone)]
pub(crate) struct Batch {
    cmd: String,
}

impl Batch {
    pub(crate) fn section(&mut self, name: &str, cmd: &str) -> &mut Self {
        if !self.cmd.is_empty() {
            self.cmd.push_str("; ");
        }
        self.cmd.push_str(&parse::sentinel(name));
        self.cmd.push_str("; ");
        self.cmd.push_str(cmd);
        self
    }

    pub(crate) fn into_command(self) -> String {
        self.cmd
    }
}

/// `delta / dt`, or `None` when the interval is not usable.
pub(crate) fn per_second(delta: f64, dt_secs: f64) -> Option<f64> {
    (dt_secs > 0.05 && delta.is_finite()).then(|| delta / dt_secs)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn batch_command_shape() {
        let mut b = Batch::default();
        b.section("a", "cat /a").section("b", "cat /b 2>/dev/null");
        assert_eq!(
            b.into_command(),
            "echo '==[a]=='; cat /a; echo '==[b]=='; cat /b 2>/dev/null"
        );
    }

    #[test]
    fn build_matches_probe() {
        let none = Probe::default();
        assert!(build(&none).is_empty(), "nothing readable → no samplers");
        let all = Probe {
            proc_stat: true,
            battery_dumpsys: true,
            thermalservice: true,
            ..Probe::default()
        };
        let names: Vec<&str> = build(&all).iter().map(|s| s.name()).collect();
        assert_eq!(names, ["android-fast", "android-medium", "android-slow"]);
    }

    #[test]
    fn per_second_guards_tiny_intervals() {
        assert_eq!(per_second(100.0, 1.0), Some(100.0));
        assert_eq!(per_second(100.0, 0.0), None);
        assert_eq!(per_second(f64::NAN, 1.0), None);
    }
}
