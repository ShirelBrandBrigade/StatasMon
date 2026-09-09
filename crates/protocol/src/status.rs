//! Derived status the core computes and the UI displays: the headline
//! sentence (docs/PLAN.md §3.11) and the per-device session state.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Severity {
    Nominal,
    Notice,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Tone {
    Ok,
    Info,
    Warn,
    Bad,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pill {
    pub label: String,
    pub tone: Tone,
}

/// "iPad (10th generation) is running a little warm" + the cause sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Headline {
    pub sentence: String,
    pub cause: String,
    pub severity: Severity,
    pub pills: Vec<Pill>,
    /// Which rule fired, for tests and debugging ("thermal_serious").
    pub rule: String,
}

impl Default for Headline {
    fn default() -> Self {
        Self {
            sentence: "Waiting for data".to_owned(),
            cause: String::new(),
            severity: Severity::Nominal,
            pills: Vec::new(),
            rule: "none".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SessionStatus {
    Idle,
    Recording,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Mark {
    pub ts_ms: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionState {
    pub id: Option<String>,
    pub status: SessionStatus,
    pub started_at_ms: Option<i64>,
    /// Recording time, excluding paused time.
    pub elapsed_ms: i64,
    pub sample_count: u64,
    pub log_count: u64,
    pub marks: Vec<Mark>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            id: None,
            status: SessionStatus::Idle,
            started_at_ms: None,
            elapsed_ms: 0,
            sample_count: 0,
            log_count: 0,
            marks: Vec::new(),
        }
    }
}
