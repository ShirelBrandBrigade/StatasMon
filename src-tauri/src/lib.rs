//! Tauri entry point: builds the engine, attaches device discovery and
//! exposes the commands in `commands.rs`. The UI subscribes once and then
//! receives every `Message` over a Channel (docs/PLAN.md §3.6).

mod commands;
mod state;

use std::sync::Arc;

use parking_lot::Mutex;
use statasmon_core::{Engine, EngineConfig};
use statasmon_device_fake::FakeDiscovery;
use tauri::Manager;

use state::{AppState, SourceMode};

fn source_mode() -> SourceMode {
    match std::env::var("STATASMON_SOURCE").as_deref() {
        Ok("real") => SourceMode::Real,
        // Real providers arrive in Phases 2–4; until then the fake is the default.
        _ => SourceMode::Fake,
    }
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
                    tracing::warn!("device source: real (no providers implemented yet)");
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
