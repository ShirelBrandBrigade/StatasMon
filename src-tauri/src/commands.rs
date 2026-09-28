//! Tauri commands: a 1:1 mapping of the UI's `commands.ts` onto the engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use statasmon_core::{DeviceSnapshot, EngineError};
use statasmon_device::ActionResult;
use statasmon_protocol::{
    DeviceAction, DeviceSummary, ErrorCode, ExportRequest, ExportResult, HistoryPoint,
    HistoryQuery, LogFilter, LogPage, Message, MetricId, MetricMeta, SessionDetail, SessionState,
    SessionSummary, Settings,
};
use statasmon_store::StoreStats;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::settings;
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

/// Every stored session (newest first) plus the live ones.
#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> CmdResult<Vec<SessionSummary>> {
    Ok(state.engine.session_summaries()?)
}

/// Summary, device info, marks, events and metric list of one session.
#[tauri::command]
pub fn open_session(state: State<'_, AppState>, session_id: String) -> CmdResult<SessionDetail> {
    Ok(state.engine.session_detail(&session_id)?)
}

/// Delete a stored session (a live one is stopped first).
#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    let engine = state.engine.clone();
    Ok(engine.delete_session(&session_id).await?)
}

/// Store root, total size and session count.
#[tauri::command]
pub fn store_stats(state: State<'_, AppState>) -> CmdResult<StoreStats> {
    let store = state.engine.store().ok_or_else(|| {
        CommandError::unsupported(
            "Session storage is not available",
            "Sessions are not being saved in this run; restart the app.",
        )
    })?;
    store.store_stats().map_err(|e| CommandError {
        code: ErrorCode::Io,
        message: e.to_string(),
        hint: Some("Check the app data folder is readable.".to_owned()),
    })
}

/// The settings in effect.
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

/// Validate, save and adopt new settings. What is returned is what was
/// stored (normalised). Sampling, retention and the adb path are read by
/// the engine, the store and the adb locator at startup, so they take
/// effect at the next launch; the UI says so (an engine-side setter is a
/// core follow-up, see ADR-014).
#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let settings = settings::normalise(settings);
    settings::validate(&settings).map_err(|reason| CommandError {
        code: ErrorCode::Unsupported,
        message: reason,
        hint: None,
    })?;
    let dir = state.settings_dir.as_deref().ok_or_else(|| {
        CommandError::unsupported(
            "Settings cannot be saved",
            "The app data folder is unavailable in this run; settings apply until the app quits.",
        )
    });
    match dir {
        Ok(dir) => settings::save(dir, &settings).map_err(|e| CommandError {
            code: ErrorCode::Io,
            message: format!("Settings could not be written: {e}"),
            hint: Some(format!(
                "Check that {} is writable.",
                settings::path_in(dir).display()
            )),
        })?,
        Err(e) => {
            tracing::warn!(message = %e.message, "settings kept in memory only");
        }
    }
    *state.settings.lock() = settings.clone();
    tracing::info!(
        fast_ms = settings.sampling.fast_ms,
        keep_days = settings.retention.keep_days,
        adb = settings.adb_path.as_deref().unwrap_or("auto"),
        "settings saved"
    );
    Ok(settings)
}

static EXPORT_SEQ: AtomicU64 = AtomicU64::new(1);

/// Export on a blocking thread; progress arrives as `Message::Progress`
/// with `task_id = "export-<n>"`, the result is returned when done.
#[tauri::command]
pub async fn export_data(
    state: State<'_, AppState>,
    request: ExportRequest,
) -> CmdResult<ExportResult> {
    let engine = state.engine.clone();
    let store = engine.store().ok_or_else(|| {
        CommandError::unsupported(
            "Session storage is not available",
            "Sessions are not being saved in this run, so there is nothing to export.",
        )
    })?;
    let mut request = request;
    if request.session_id.is_none() {
        request.session_id = engine.current_session_id(&request.device_id)?;
    }
    if request.session_id.is_none() {
        return Err(CommandError::unsupported(
            "No session to export",
            "Start a recording or pick a stored session first.",
        ));
    }
    let task_id = format!("export-{}", EXPORT_SEQ.fetch_add(1, Ordering::Relaxed));
    let app_data = state.app_data_dir.clone();
    let device_id = request.device_id.clone();
    let progress_engine = engine.clone();
    let progress_task = task_id.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        statasmon_export::export(&store, app_data.as_deref(), &request, |pct, msg| {
            progress_engine.progress(&progress_task, pct, msg);
        })
    })
    .await
    .map_err(|e| CommandError {
        code: ErrorCode::Internal,
        message: format!("export task failed: {e}"),
        hint: None,
    })?;
    match outcome {
        Ok(result) => {
            tracing::info!(path = %result.path, bytes = result.bytes, entries = result.entries, "export done");
            Ok(result)
        }
        Err(e) => {
            let err = CommandError {
                code: e.code(),
                message: e.to_string(),
                hint: e.hint(),
            };
            engine.progress(&task_id, 100.0, &format!("Export failed: {e}"));
            engine.notice(Message::Error {
                device_id: Some(device_id),
                code: err.code,
                message: format!("Export failed: {e}"),
                hint: err.hint.clone(),
            });
            Err(err)
        }
    }
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

// ---- Updates (Phase 7.4, ADR-015) ------------------------------------------
//
// The check runs on the user's explicit opt-in (Settings → "Check for
// updates on launch") or on an explicit command; nothing is ever downloaded
// or installed without `install_update`. The plugin verifies the minisign
// signature against `plugins.updater.pubkey` before installing.

/// What `check_for_update` answers.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Task id of the update notices and download progress.
pub const UPDATE_TASK: &str = "update";

/// A repository owner left as a placeholder marks an unconfigured endpoint.
const ENDPOINT_PLACEHOLDER: &str = "OWNER";

/// True when the endpoints are real (non-empty, no placeholder) and a public
/// key is present; anything else means "updater not configured".
fn updater_usable(endpoints: &[&str], pubkey: &str) -> bool {
    !endpoints.is_empty()
        && endpoints
            .iter()
            .all(|e| !e.trim().is_empty() && !e.contains(ENDPOINT_PLACEHOLDER))
        && !pubkey.trim().is_empty()
}

/// Refuse to touch the network unless `plugins.updater` in tauri.conf.json
/// carries a real endpoint and a public key (docs/RELEASE.md §5).
fn updater_configured(app: &AppHandle) -> CmdResult<()> {
    let cfg = app.config().plugins.0.get("updater");
    let endpoints: Vec<&str> = cfg
        .and_then(|c| c.get("endpoints"))
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    let pubkey = cfg
        .and_then(|c| c.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if updater_usable(&endpoints, pubkey) {
        Ok(())
    } else {
        Err(CommandError::unsupported(
            "Updates are not configured in this build",
            "Set plugins.updater.endpoints and pubkey in src-tauri/tauri.conf.json (docs/RELEASE.md §5).",
        ))
    }
}

fn update_error(what: &str, e: tauri_plugin_updater::Error) -> CommandError {
    CommandError {
        code: ErrorCode::Io,
        message: format!("{what}: {e}"),
        hint: Some(
            "Check the network connection; the release endpoint is on github.com.".to_owned(),
        ),
    }
}

/// First line of the release notes, trimmed to 120 characters, for the
/// notice text; a fixed phrase when there are none.
fn notes_summary(body: Option<&str>) -> String {
    let line = body
        .and_then(|b| b.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("see the release notes");
    let mut s: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        s.push('…');
    }
    s
}

/// Ask the release endpoint once. A user action, so it runs regardless of
/// the launch setting; rejects with `unsupported` when the build has no
/// updater configured.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> CmdResult<UpdateStatus> {
    updater_configured(&app)?;
    let updater = app
        .updater()
        .map_err(|e| update_error("Updater unavailable", e))?;
    match updater
        .check()
        .await
        .map_err(|e| update_error("Update check failed", e))?
    {
        Some(update) => {
            tracing::info!(version = %update.version, current = %update.current_version, "update available");
            Ok(UpdateStatus {
                available: true,
                version: Some(update.version),
                notes: update.body,
            })
        }
        None => {
            tracing::info!("update check: up to date");
            Ok(UpdateStatus {
                available: false,
                version: None,
                notes: None,
            })
        }
    }
}

/// Download, verify and install the latest release, then relaunch. Guarded
/// by the "Check for updates on launch" setting (ADR-015): with it off the
/// app never contacts the network. Progress arrives as `Message::Progress`
/// with `task_id = "update"`.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    updater_configured(&app)?;
    let enabled = state.settings.lock().update_check;
    if !enabled {
        return Err(CommandError::unsupported(
            "Updates are turned off",
            "Turn on \"Check for updates on launch\" in Settings first.",
        ));
    }
    let engine = state.engine.clone();
    let updater = app
        .updater()
        .map_err(|e| update_error("Updater unavailable", e))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| update_error("Update check failed", e))?
    else {
        return Err(CommandError::unsupported(
            "No update available",
            "This is the latest version.",
        ));
    };
    let version = update.version.clone();
    let label = format!("Downloading v{version}…");
    engine.progress(UPDATE_TASK, 0.0, &label);
    let mut received: u64 = 0;
    let progress_engine = engine.clone();
    update
        .download_and_install(
            |chunk, total| {
                received += chunk as u64;
                let pct = total
                    .filter(|t| *t > 0)
                    .map_or(0.0, |t| received as f32 / t as f32 * 100.0);
                progress_engine.progress(UPDATE_TASK, pct, &label);
            },
            || engine.progress(UPDATE_TASK, 100.0, &format!("Installing v{version}…")),
        )
        .await
        .map_err(|e| {
            engine.progress(UPDATE_TASK, 100.0, &format!("Update failed: {e}"));
            update_error("Update failed", e)
        })?;
    tracing::info!(version = %version, "update installed; relaunching");
    app.restart()
}

/// The launch-time check (only called when `settings.update_check` is on):
/// announces a new version as a kept notice so a UI that subscribes later
/// still sees it; never downloads.
pub fn spawn_startup_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = updater_configured(&app) {
            tracing::info!(reason = %e.message, "launch update check skipped");
            return;
        }
        let engine = app.state::<AppState>().engine.clone();
        let result = match app.updater() {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        match result {
            Ok(Some(update)) => {
                tracing::info!(version = %update.version, current = %update.current_version, "update available");
                engine.notice(Message::Progress {
                    task_id: UPDATE_TASK.to_owned(),
                    pct: 100.0,
                    message: format!(
                        "Update available: v{} ({})",
                        update.version,
                        notes_summary(update.body.as_deref())
                    ),
                });
            }
            Ok(None) => tracing::info!("launch update check: up to date"),
            Err(e) => tracing::warn!(error = %e, "launch update check failed"),
        }
    });
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn placeholder_or_missing_config_is_not_usable() {
        assert!(!updater_usable(&[], "key"));
        assert!(!updater_usable(
            &["https://github.com/OWNER/StatasMon/releases/latest/download/latest.json"],
            "key"
        ));
        assert!(!updater_usable(&["https://example.com/latest.json"], "  "));
        assert!(!updater_usable(
            &["https://example.com/latest.json", ""],
            "key"
        ));
        assert!(updater_usable(
            &[
                "https://github.com/ShirelBrandBrigade/StatasMon/releases/latest/download/latest.json"
            ],
            "dW50cnVzdGVk"
        ));
    }

    #[test]
    fn notes_summary_takes_the_first_non_empty_line() {
        assert_eq!(notes_summary(None), "see the release notes");
        assert_eq!(
            notes_summary(Some("  \n\n Fixes the thermal card \nmore")),
            "Fixes the thermal card"
        );
        let long = "x".repeat(150);
        let s = notes_summary(Some(&long));
        assert_eq!(s.chars().count(), 121);
        assert!(s.ends_with('…'));
    }
}

/// What this computer needs for iPhones and iPads, and whether the app can
/// install it (Linux with a known package manager and polkit; ADR-022).
#[tauri::command]
pub fn apple_support_status() -> statasmon_device_ios::setup::AppleSupportStatus {
    statasmon_device_ios::setup::apple_support_status()
}

/// Install usbmuxd through the system's own password dialog. Only reachable
/// from the user's click on the notice; the password never enters the app.
#[tauri::command]
pub async fn install_apple_support() -> CmdResult<String> {
    use statasmon_device_ios::setup::SetupError;
    statasmon_device_ios::setup::install_apple_support()
        .await
        .map_err(|e| CommandError {
            code: match e {
                SetupError::NotInstallable(_) => ErrorCode::Unsupported,
                _ => ErrorCode::Io,
            },
            hint: match &e {
                SetupError::Dismissed | SetupError::NotAuthorised => Some(
                    "Press Install again and enter the password of an administrator.".to_owned(),
                ),
                SetupError::Failed { .. } | SetupError::TimedOut(_) | SetupError::Spawn(_) => {
                    Some(statasmon_device_ios::error::muxd_hint().to_owned())
                }
                SetupError::NotInstallable(_) => None,
            },
            message: e.to_string(),
        })
}
