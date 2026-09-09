//! Unified log entry (docs/FEATURES.md §9).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Common level scale. iOS: Debug, Info, Notice(Default), Error, Fault.
/// Android: V/D → Debug, I → Info, W → Warning, E → Error, F/A → Fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Fault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogEntry {
    /// Monotonic per device, assigned by the provider.
    pub seq: u64,
    pub ts_ms: i64,
    pub pid: Option<u32>,
    pub process: String,
    /// Android tag.
    pub tag: Option<String>,
    /// iOS subsystem (com.example.app).
    pub subsystem: Option<String>,
    /// iOS category.
    pub category: Option<String>,
    /// Android buffer (main, system, crash, events, radio).
    pub buffer: Option<String>,
    pub level: LogLevel,
    pub message: String,
}
