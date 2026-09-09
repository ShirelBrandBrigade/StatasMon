//! Cadence-driven samplers (steps 2.3–2.5). Each sampler owns its parsers
//! (pure functions with fixture tests) and its delta state.

use async_trait::async_trait;
use statasmon_device::EventSink;

use crate::probe::Probe;
use crate::shell::{Shell, ShellError};

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

/// Build the samplers this device supports (per the probe). The source
/// spawns one loop per cadence and calls `tick` sequentially within it.
pub fn build(probe: &Probe) -> Vec<Box<dyn Sampler>> {
    let _ = probe;
    // Filled in by the samplers agent (2.3–2.5).
    Vec::new()
}
