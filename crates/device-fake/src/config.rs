//! Configuration of the fake discovery and its scenarios.

use std::fmt;
use std::str::FromStr;

/// Environment variable that selects the scenario (`warm|quiet|stress|crash`).
pub const SCENARIO_ENV: &str = "STATASMON_FAKE_SCENARIO";

/// Which scripted behaviour the fake devices follow. Time runs from
/// `start()` (scaled by [`FakeConfig::time_scale`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scenario {
    /// Thermal Nominal → Fair at t+20 s, memory warning to FieldKit at t+35 s.
    #[default]
    Warm,
    /// Flat values, no events.
    Quiet,
    /// CPU 80–100 % and memory pressure 2 throughout.
    Stress,
    /// FieldKit crashes at t+15 s.
    Crash,
}

impl Scenario {
    /// All scenarios, for help text and tests.
    pub const ALL: [Scenario; 4] = [
        Scenario::Warm,
        Scenario::Quiet,
        Scenario::Stress,
        Scenario::Crash,
    ];

    /// Lower-case name as used in the environment variable.
    pub fn as_str(self) -> &'static str {
        match self {
            Scenario::Warm => "warm",
            Scenario::Quiet => "quiet",
            Scenario::Stress => "stress",
            Scenario::Crash => "crash",
        }
    }

    /// Read the scenario from [`SCENARIO_ENV`]; unknown or unset → default.
    pub fn from_env() -> Scenario {
        std::env::var(SCENARIO_ENV)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_default()
    }
}

impl fmt::Display for Scenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error for an unknown scenario name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownScenario(pub String);

impl fmt::Display for UnknownScenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown scenario {:?} (expected warm|quiet|stress|crash)",
            self.0
        )
    }
}

impl std::error::Error for UnknownScenario {}

impl FromStr for Scenario {
    type Err = UnknownScenario;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "warm" => Ok(Scenario::Warm),
            "quiet" => Ok(Scenario::Quiet),
            "stress" => Ok(Scenario::Stress),
            "crash" => Ok(Scenario::Crash),
            other => Err(UnknownScenario(other.to_owned())),
        }
    }
}

/// One fake device to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeDevice {
    /// `fake:iphone` — iPhone 16 Pro, iOS 27.0, Developer Mode on (tier 2).
    Iphone,
    /// `fake:ipad` — iPad (10th generation), iPadOS 17.4.1. Tier 1 unless
    /// `developer_mode` is true.
    Ipad { developer_mode: bool },
    /// `fake:android` — Samsung Galaxy Tab S8 (SM-X700), Android 14.
    Android,
}

/// Everything the fake discovery needs.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeConfig {
    /// Devices to report, in attach order.
    pub devices: Vec<FakeDevice>,
    /// Seed for the value generators; each device derives its own from it.
    pub seed: u64,
    /// Scripted behaviour.
    pub scenario: Scenario,
    /// Multiplier on scenario time: 10.0 makes the warm rise happen at 2 s.
    pub time_scale: f64,
    /// Log lines per second per device.
    pub log_lines_per_sec: f64,
}

impl Default for FakeConfig {
    fn default() -> Self {
        Self {
            devices: vec![
                FakeDevice::Iphone,
                FakeDevice::Ipad {
                    developer_mode: false,
                },
                FakeDevice::Android,
            ],
            seed: 42,
            scenario: Scenario::Warm,
            time_scale: 1.0,
            log_lines_per_sec: 20.0,
        }
    }
}

impl FakeConfig {
    /// Defaults with the scenario taken from [`SCENARIO_ENV`].
    pub fn from_env() -> Self {
        Self {
            scenario: Scenario::from_env(),
            ..Self::default()
        }
    }

    /// Replace the scenario.
    #[must_use]
    pub fn with_scenario(mut self, scenario: Scenario) -> Self {
        self.scenario = scenario;
        self
    }

    /// Replace the device list.
    #[must_use]
    pub fn with_devices(mut self, devices: Vec<FakeDevice>) -> Self {
        self.devices = devices;
        self
    }

    /// Replace the seed.
    #[must_use]
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Replace the scenario time scale.
    #[must_use]
    pub fn with_time_scale(mut self, time_scale: f64) -> Self {
        self.time_scale = if time_scale > 0.0 { time_scale } else { 1.0 };
        self
    }

    /// Replace the log rate.
    #[must_use]
    pub fn with_log_rate(mut self, lines_per_sec: f64) -> Self {
        self.log_lines_per_sec = lines_per_sec.max(0.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_round_trips_through_names() {
        for s in Scenario::ALL {
            assert_eq!(s.as_str().parse::<Scenario>(), Ok(s));
        }
        assert_eq!(" CRASH ".parse::<Scenario>(), Ok(Scenario::Crash));
        assert!("hot".parse::<Scenario>().is_err());
    }

    #[test]
    fn default_has_three_devices() {
        let cfg = FakeConfig::default();
        assert_eq!(cfg.devices.len(), 3);
        assert_eq!(cfg.scenario, Scenario::Warm);
    }
}
