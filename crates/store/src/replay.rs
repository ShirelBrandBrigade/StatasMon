//! Replay a recorded bus stream (JSONL of protocol messages, e.g.
//! `fixtures/fake/session-3dev-30s.jsonl`) into the store: one session per
//! device, timestamps as recorded. Used by tests and the fixture tooling.

use std::collections::BTreeMap;
use std::sync::Arc;

use statasmon_protocol::{EventKind, Mark, Message};

use crate::error::{StoreError, StoreResult};
use crate::ids::session_id_for;
use crate::store::Store;

/// The timestamp a message carries, if any.
fn message_ts(msg: &Message) -> Option<i64> {
    match msg {
        Message::Samples { ts_ms, .. }
        | Message::Cores { ts_ms, .. }
        | Message::Processes { ts_ms, .. }
        | Message::Event { ts_ms, .. } => Some(*ts_ms),
        Message::Log { entries, .. } => entries.iter().map(|e| e.ts_ms).max(),
        _ => None,
    }
}

/// Parse `text` (one JSON message per line; blank and `#` lines skipped).
pub fn parse_jsonl(text: &str) -> StoreResult<Vec<Message>> {
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let msg: Message = serde_json::from_str(line).map_err(|e| {
            StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line {}: {e}", idx + 1),
            ))
        })?;
        out.push(msg);
    }
    Ok(out)
}

/// Write every message of `messages` into `store`, beginning one session
/// per `DeviceAttached` device and ending it at the device's last
/// timestamp. Returns the session ids in device order. Flushes before
/// returning, so queries see everything.
pub fn replay(store: &Store, messages: &[Message]) -> StoreResult<Vec<String>> {
    let mut first_ts: BTreeMap<String, i64> = BTreeMap::new();
    let mut last_ts: BTreeMap<String, i64> = BTreeMap::new();
    for m in messages {
        if let (Some(id), Some(ts)) = (m.device_id(), message_ts(m)) {
            first_ts
                .entry(id.to_owned())
                .and_modify(|t| *t = (*t).min(ts))
                .or_insert(ts);
            last_ts
                .entry(id.to_owned())
                .and_modify(|t| *t = (*t).max(ts))
                .or_insert(ts);
        }
    }
    let mut sessions: BTreeMap<String, String> = BTreeMap::new();
    let mut order = Vec::new();
    for m in messages {
        match m {
            Message::DeviceAttached { device } => {
                if !sessions.contains_key(&device.id) {
                    let started = first_ts.get(&device.id).copied().unwrap_or(0);
                    let id =
                        store.begin(&session_id_for(&device.id, started), device, None, started)?;
                    sessions.insert(device.id.clone(), id.clone());
                    order.push(id);
                }
            }
            Message::Info { device_id, info } => {
                if let Some(sid) = sessions.get(device_id) {
                    store.set_info(sid, info)?;
                }
            }
            Message::Event {
                device_id,
                ts_ms,
                kind: EventKind::Mark,
                detail,
                ..
            } => {
                if let Some(sid) = sessions.get(device_id) {
                    store.mark(
                        sid,
                        &Mark {
                            ts_ms: *ts_ms,
                            label: detail.clone(),
                        },
                    )?;
                }
            }
            Message::Samples { .. }
            | Message::Cores { .. }
            | Message::Processes { .. }
            | Message::Log { .. }
            | Message::Event { .. } => {
                // Replay is offline: wait for room instead of dropping.
                store.write_blocking(&Arc::new(m.clone()))?;
            }
            _ => {}
        }
    }
    for (device_id, sid) in &sessions {
        let started = first_ts.get(device_id).copied().unwrap_or(0);
        let ended = last_ts.get(device_id).copied().unwrap_or(started);
        store.end(sid, ended, ended - started)?;
    }
    store.flush()?;
    Ok(order)
}

/// [`parse_jsonl`] then [`replay`].
pub fn replay_jsonl(store: &Store, text: &str) -> StoreResult<Vec<String>> {
    replay(store, &parse_jsonl(text)?)
}
