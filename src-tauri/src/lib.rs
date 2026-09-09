//! Tauri entry point: builds the engine, attaches device discovery and
//! exposes the commands in `commands.rs`. The UI subscribes once and then
//! receives every `Message` over a Channel (docs/PLAN.md §3.6).

mod commands;
mod state;

use std::sync::Arc;

use parking_lot::Mutex;
use statasmon_core::{Engine, EngineConfig};
use statasmon_device_android::adb::{AdbDiscovery, AdbLocation};
use statasmon_device_fake::FakeDiscovery;
use tauri::Manager;

use state::{AppState, SourceMode};

/// `STATASMON_SOURCE=fake` (what `pnpm dev:mock` sets) runs the fake
/// devices; `real`, `auto` or unset attach the real providers.
fn source_mode() -> SourceMode {
    match std::env::var("STATASMON_SOURCE").as_deref() {
        Ok("fake") => SourceMode::Fake,
        _ => SourceMode::Real,
    }
}

/// Attach the Android discovery. When adb is missing the discovery keeps
/// re-running the locator, so the device appears once platform-tools is
/// installed; the hint is logged here (surfacing it on the bus needs an
/// engine-side notice API, see the step 2.2 report).
fn attach_android(engine: &Engine) {
    let engine = engine.clone();
    tauri::async_runtime::spawn(async move {
        match statasmon_device_android::adb::locate().await {
            AdbLocation::Found {
                path,
                source,
                version,
            } => tracing::info!(
                adb = %path.display(),
                ?source,
                version = version.map(|v| v.version).unwrap_or_default(),
                "android: adb located"
            ),
            AdbLocation::Missing { hint } => {
                tracing::error!(code = "adb_not_found", hint = %hint, "android: adb not found");
                engine.notice(statasmon_protocol::Message::Error {
                    device_id: None,
                    code: statasmon_protocol::ErrorCode::AdbNotFound,
                    message: "adb was not found on this computer".to_owned(),
                    hint: Some(hint),
                });
            }
        }
        engine.attach_discovery(Box::new(AdbDiscovery::from_locator()));
    });
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let mode = source_mode();
            let engine =
                tauri::async_runtime::block_on(async { Engine::new(EngineConfig::default()) });
            let fake = match mode {
                SourceMode::Fake => {
                    let discovery = FakeDiscovery::from_env();
                    let handle = discovery.handle();
                    let e = engine.clone();
                    tauri::async_runtime::spawn(async move {
                        e.attach_discovery(Box::new(discovery));
                    });
                    tracing::info!("device source: fake");
                    Some(handle)
                }
                SourceMode::Real => {
                    attach_android(&engine);
                    tracing::info!(
                        "device source: real (android over adb; iOS arrives in Phase 3)"
                    );
                    None
                }
            };
            app.manage(AppState {
                engine,
                mode,
                fake,
                forwarder: Arc::new(Mutex::new(None)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::subscribe,
            commands::unsubscribe,
            commands::list_devices,
            commands::metric_catalog,
            commands::snapshot,
            commands::start_session,
            commands::pause_session,
            commands::resume_session,
            commands::stop_session,
            commands::mark,
            commands::query_logs,
            commands::query_history,
            commands::export_data,
            commands::device_action,
            commands::fake_detach,
            commands::fake_reattach,
            commands::ui_report,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("error while running StatasMon: {e}"));
}
