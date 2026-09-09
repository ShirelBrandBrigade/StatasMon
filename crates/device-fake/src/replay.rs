//! Fixture files: one protocol [`Message`] per line (JSONL), recorded from
//! a fake or a real provider and replayed with the original timing.

use std::path::Path;

use statasmon_device::{DeviceError, DeviceResult};
use statasmon_protocol::{CapabilitySet, DeviceInfo, DeviceSummary, Message};

/// The timestamp a message carries, if any (used for replay pacing).
pub fn message_ts(msg: &Message) -> Option<i64> {
    match msg {
        Message::Samples { ts_ms, .. }
        | Message::Cores { ts_ms, .. }
        | Message::Processes { ts_ms, .. }
        | Message::Event { ts_ms, .. } => Some(*ts_ms),
        Message::Log { entries, .. } => entries.iter().map(|e| e.ts_ms).max(),
        _ => None,
    }
}

/// A parsed fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    /// From the first `DeviceAttached` line.
    pub summary: DeviceSummary,
    /// From the first `Info` line, if any.
    pub info: Option<DeviceInfo>,
    /// From the first `Capabilities` line; empty set when absent.
    pub capabilities: CapabilitySet,
    /// Every message in file order (attach/detach lines removed).
    pub messages: Vec<Message>,
}

impl Fixture {
    /// Parse a JSONL fixture. Fails with a `Parse` error naming the line.
    pub fn parse(text: &str) -> DeviceResult<Self> {
        let mut summary = None;
        let mut info = None;
        let mut capabilities = None;
        let mut messages = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let msg: Message = serde_json::from_str(line)
                .map_err(|e| DeviceError::parse(format!("fixture line {}", idx + 1), e))?;
            match msg {
                Message::DeviceAttached { device } => {
                    summary.get_or_insert(device);
                }
                Message::DeviceDetached { .. } => {}
                Message::Info { info: i, .. } => {
                    if info.is_none() {
                        info = Some((*i).clone());
                    }
                    messages.push(Message::Info {
                        device_id: msg_device_id(&messages, &summary),
                        info: i,
                    });
                }
                Message::Capabilities { set, device_id } => {
                    if capabilities.is_none() {
                        capabilities = Some(set.clone());
                    }
                    messages.push(Message::Capabilities { device_id, set });
                }
                other => messages.push(other),
            }
        }
        let summary = summary.ok_or_else(|| {
            DeviceError::parse("fixture", "no device_attached line")
                .with_hint("Fixtures start with a device_attached message.")
        })?;
        Ok(Self {
            summary,
            info,
            capabilities: capabilities.unwrap_or_default(),
            messages,
        })
    }

    /// Read and parse a fixture file.
    pub fn load(path: impl AsRef<Path>) -> DeviceResult<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| DeviceError::io(format!("reading fixture {}", path.display()), e))?;
        Self::parse(&text)
    }

    /// Duration covered by the timestamped messages, in ms.
    pub fn duration_ms(&self) -> i64 {
        let ts: Vec<i64> = self.messages.iter().filter_map(message_ts).collect();
        match (ts.iter().min(), ts.iter().max()) {
            (Some(a), Some(b)) => b - a,
            _ => 0,
        }
    }
}

fn msg_device_id(_messages: &[Message], summary: &Option<DeviceSummary>) -> String {
    summary.as_ref().map(|s| s.id.clone()).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use statasmon_protocol::{ConnectionState, MetricId, MetricValue, Platform, Transport};

    fn attached() -> Message {
        Message::DeviceAttached {
            device: DeviceSummary {
                id: "fake:x".into(),
                platform: Platform::Fake,
                name: "X".into(),
                model_name: "X".into(),
                transport: Transport::Fake,
                state: ConnectionState::Tier2,
            },
        }
    }

    #[test]
    fn parses_lines_and_extracts_summary() {
        let lines = [
            attached(),
            Message::Samples {
                device_id: "fake:x".into(),
                ts_ms: 1000,
                values: vec![MetricValue {
                    id: MetricId::CpuTotal,
                    value: 1.0,
                }],
            },
            Message::Samples {
                device_id: "fake:x".into(),
                ts_ms: 3000,
                values: vec![],
            },
        ];
        let text: String = lines
            .iter()
            .map(|m| serde_json::to_string(m).unwrap() + "\n")
            .collect();
        let f = Fixture::parse(&text).unwrap();
        assert_eq!(f.summary.id, "fake:x");
        assert_eq!(f.messages.len(), 2);
        assert_eq!(f.duration_ms(), 2000);
    }

    #[test]
    fn missing_attached_is_a_parse_error() {
        let err = Fixture::parse(
            "{\"type\":\"progress\",\"task_id\":\"a\",\"pct\":1,\"message\":\"\"}\n",
        )
        .unwrap_err();
        assert_eq!(err.code(), statasmon_protocol::ErrorCode::Parse);
        assert!(err.hint().is_some());
    }

    #[test]
    fn bad_json_names_the_line() {
        let text = format!(
            "{}\nnot json\n",
            serde_json::to_string(&attached()).unwrap()
        );
        let err = Fixture::parse(&text).unwrap_err();
        assert!(err.to_string().contains("line 2"));
    }
}
