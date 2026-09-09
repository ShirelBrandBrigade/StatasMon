//! Engine configuration.

use statasmon_protocol::SamplingConfig;

/// Bus capacity: messages a slow subscriber may fall behind before it
/// starts skipping.
pub const DEFAULT_BUS_CAPACITY: usize = 4096;
/// Samples kept per metric (one hour at 1 Hz).
pub const DEFAULT_RING_CAPACITY: usize = 3600;
/// Per-core ticks kept.
pub const DEFAULT_CORE_CAPACITY: usize = 600;
/// Log lines kept per device.
pub const DEFAULT_LOG_CAPACITY: usize = 50_000;
/// Window in which past events still shape the headline.
pub const DEFAULT_EVENT_WINDOW_MS: i64 = 60_000;

/// Everything the engine needs at construction.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Cadence handed to every source on start.
    pub sampling: SamplingConfig,
    /// Samples kept per metric per device.
    pub ring_capacity: usize,
    /// Per-core ticks kept per device.
    pub core_capacity: usize,
    /// Log lines kept per device.
    pub log_capacity: usize,
    /// Broadcast bus capacity (at least 4096).
    pub bus_capacity: usize,
    /// Start a recording session as soon as a device attaches.
    pub auto_record: bool,
    /// How long events keep influencing the headline.
    pub event_window_ms: i64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sampling: SamplingConfig::default(),
            ring_capacity: DEFAULT_RING_CAPACITY,
            core_capacity: DEFAULT_CORE_CAPACITY,
            log_capacity: DEFAULT_LOG_CAPACITY,
            bus_capacity: DEFAULT_BUS_CAPACITY,
            auto_record: true,
            event_window_ms: DEFAULT_EVENT_WINDOW_MS,
        }
    }
}

impl EngineConfig {
    /// Replace the sampling cadence.
    #[must_use]
    pub fn with_sampling(mut self, sampling: SamplingConfig) -> Self {
        self.sampling = sampling;
        self
    }

    /// Replace the auto-record flag.
    #[must_use]
    pub fn with_auto_record(mut self, auto_record: bool) -> Self {
        self.auto_record = auto_record;
        self
    }
}
