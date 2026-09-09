//! Tauri commands: a 1:1 mapping of the UI's `commands.ts` onto the engine.

use std::sync::Arc;

use serde::Serialize;
use statasmon_core::{DeviceSnapshot, EngineError};
use statasmon_device::ActionResult;
use statasmon_protocol::{
    DeviceAction, DeviceSummary, ErrorCode, ExportRequest, HistoryPoint, HistoryQuery, LogFilter,
    LogPage, Message, MetricId, MetricMeta, SessionState,
};
use tauri::State;
use tauri::ipc::Channel;

use crate::state::AppState;

/// Error shape delivered to the UI (`{ code, message, hint }`).
#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
}

impl From<EngineError> for CommandError {
    fn from(e: EngineError) -> Self {
        Self {
            code: e.code(),
            message: e.to_string(),
            hint: e.hint(),
        }
    }
}

impl CommandError {
    fn unsupported(message: &str, hint: &str) -> Self {
        Self {
            code: ErrorCode::Unsupported,
            message: message.to_owned(),
            hint: Some(hint.to_owned()),
        }
    }
}

type CmdResult<T> = Result<T, CommandError>;

/// Starts forwarding every engine message to the UI over `channel`. Late
/// subscribers first receive the current state of every attached device.
#[tauri::command]
pub fn subscribe(state: State<'_, AppState>, channel: Channel<Message>) -> CmdResult<()> {
    let engine = state.engine.clone();
    for notice in engine.notices() {
        let _ = channel.send(notice);
    }
    // Replay current state so the UI never starts empty.
    for summary in engine.devices() {
        if let Some(snap) = engine.snapshot(&summary.id) {
            let id = summary.id.clone();
            let _ = channel.send(Message::DeviceAttached {
                device: snap.summary,
            });
            let _ = channel.send(Message::Capabilities {
                device_id: id.clone(),
                set: snap.capabilities,
            });
            if let Some(info) = snap.info {
                let _ = channel.send(Message::Info {
                    device_id: id.clone(),
                    info: Box::new(info),
                });
            }
            let _ = channel.send(Message::Headline {
                device_id: id.clone(),
                headline: snap.headline,
            });
            let _ = channel.send(Message::Session {
                device_id: id.clone(),
                session: snap.session,
            });
            if let Some(ts_ms) = snap.latest_ts_ms {
                let _ = channel.send(Message::Samples {
                    device_id: id.clone(),
                    ts_ms,
                    values: snap.latest,
                });
                let _ = channel.send(Message::Cores {
                    device_id: id.clone(),
                    ts_ms,
                    cores: snap.cores,
                });
                let _ = channel.send(Message::Processes {
                    device_id: id,
                    ts_ms,
                    rows: snap.processes,
                });
            }
        }
    }
    let mut rx = engine.subscribe();
    let handle = tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if channel.send(Arc::unwrap_or_clone(msg)).is_err() {
                        tracing::debug!("ui channel closed; stopping forwarder");
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "ui subscriber lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    if let Some(old) = state.forwarder.lock().replace(handle) {
        old.abort();
    }
    Ok(())
}

#[tauri::command]
pub fn unsubscribe(state: State<'_, AppState>) {
    if let Some(h) = state.forwarder.lock().take() {
        h.abort();
    }
}

#[tauri::command]
pub fn list_devices(state: State<'_, AppState>) -> Vec<DeviceSummary> {
    state.engine.devices()
}

#[tauri::command]
pub fn metric_catalog() -> Vec<MetricMeta> {
    MetricId::catalog()
}

#[tauri::command]
pub fn snapshot(state: State<'_, AppState>, device_id: String) -> CmdResult<DeviceSnapshot> {
    state
        .engine
        .snapshot(&device_id)
        .ok_or_else(|| EngineError::UnknownDevice(device_id).into())
}

#[tauri::command]
pub fn start_session(state: State<'_, AppState>, device_id: String) -> CmdResult<SessionState> {
    Ok(state.engine.start_session(&device_id)?)
}

#[tauri::command]
pub fn pause_session(state: State<'_, AppState>, device_id: String) -> CmdResult<SessionState> {
    Ok(state.engine.pause(&device_id)?)
}

#[tauri::command]
pub fn resume_session(state: State<'_, AppState>, device_id: String) -> CmdResult<SessionState> {
    Ok(state.engine.resume(&device_id)?)
}

#[tauri::command]
pub fn stop_session(state: State<'_, AppState>, device_id: String) -> CmdResult<SessionState> {
    Ok(state.engine.stop_session(&device_id)?)
}

#[tauri::command]
pub fn mark(
    state: State<'_, AppState>,
    device_id: String,
    label: String,
) -> CmdResult<SessionState> {
    Ok(state.engine.mark(&device_id, &label)?)
}

#[tauri::command]
pub fn query_logs(
    state: State<'_, AppState>,
    filter: LogFilter,
    offset: Option<usize>,
    limit: Option<usize>,
) -> CmdResult<LogPage> {
    Ok(state
        .engine
        .query_logs(&filter, offset.unwrap_or(0), limit.unwrap_or(500).min(5000))?)
}

#[tauri::command]
pub fn query_history(
    state: State<'_, AppState>,
    query: HistoryQuery,
) -> CmdResult<Vec<HistoryPoint>> {
    Ok(state.engine.history(&query)?)
}

#[tauri::command]
pub fn export_data(_state: State<'_, AppState>, request: ExportRequest) -> CmdResult<String> {
    let _ = request;
    Err(CommandError::unsupported(
        "Export is not available yet",
        "Session storage and export arrive in Phase 5 of the plan.",
    ))
}

/// What a device action produced, in a UI-friendly shape.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ActionOutcome {
    Done,
    File { path: String },
    Started { task_id: String },
}

#[tauri::command]
pub async fn device_action(
    state: State<'_, AppState>,
    device_id: String,
    action: DeviceAction,
) -> CmdResult<ActionOutcome> {
    let engine = state.engine.clone();
    Ok(match engine.run_action(&device_id, action).await? {
        ActionResult::Done => ActionOutcome::Done,
        ActionResult::File { path } => ActionOutcome::File { path },
        ActionResult::Started { task_id } => ActionOutcome::Started { task_id },
    })
}

#[tauri::command]
pub fn fake_detach(state: State<'_, AppState>, device_id: String) -> CmdResult<()> {
    fake_control(&state, &device_id, true)
}

#[tauri::command]
pub fn fake_reattach(state: State<'_, AppState>, device_id: String) -> CmdResult<()> {
    fake_control(&state, &device_id, false)
}

fn fake_control(state: &AppState, device_id: &str, detach: bool) -> CmdResult<()> {
    let Some(handle) = &state.fake else {
        return Err(CommandError::unsupported(
            "Not running on fake devices",
            "Start the app with STATASMON_SOURCE=fake (pnpm dev:mock).",
        ));
    };
    let r = if detach {
        handle.detach(device_id)
    } else {
        handle.reattach(device_id)
    };
    r.map_err(|_| CommandError {
        code: ErrorCode::Internal,
        message: "fake discovery has stopped".to_owned(),
        hint: None,
    })
}

/// Debug-only health check: the UI reports what it received so the Channel
/// path can be verified from the app log without a screenshot.
#[tauri::command]
pub fn ui_report(message: String) {
    tracing::info!(target: "ui", "{message}");
}
