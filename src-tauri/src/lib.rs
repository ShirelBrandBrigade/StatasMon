//! Tauri entry point: builds the engine, attaches device discovery and
//! exposes the commands in `commands.rs`. The UI subscribes once and then
//! receives every `Message` over a Channel (docs/PLAN.md §3.6).

mod commands;
mod settings;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use statasmon_core::Engine;
use statasmon_device_android::adb::{AdbDiscovery, AdbLocation, AdbServer};
use statasmon_device_fake::FakeDiscovery;
use statasmon_device_ios::MuxdDiscovery;
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
/// The platform-tools folder shipped inside the app (ADR-021), registered
/// with the locator. On Unix the copy is made executable in case the
/// packager dropped the mode bit.
fn export_bundled_adb(resource_dir: Option<PathBuf>) {
    let Some(dir) = resource_dir.map(|d| d.join("platform-tools")) else {
        return;
    };
    let adb = dir.join(if cfg!(windows) { "adb.exe" } else { "adb" });
    if !adb.is_file() {
        tracing::info!(dir = %dir.display(), "android: no bundled adb in this build");
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&adb)
            && meta.permissions().mode() & 0o111 == 0
            && let Err(e) = std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755))
        {
            tracing::warn!(error = %e, "android: bundled adb is not executable");
        }
    }
    statasmon_device_android::adb::locator::set_bundled_dir(dir.clone());
    tracing::info!(dir = %dir.display(), "android: bundled adb available");
}

fn attach_android(engine: &Engine, adb_path: Option<PathBuf>) {
    let engine = engine.clone();
    tauri::async_runtime::spawn(async move {
        // Settings > locator: an explicit adb path from settings.json is
        // used as is (it was validated to exist when saved).
        if let Some(path) = adb_path {
            tracing::info!(adb = %path.display(), "android: adb from settings");
            engine.attach_discovery(Box::new(AdbDiscovery::new(AdbServer::new(path))));
            return;
        }
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

/// Attach the iOS discovery (usbmuxd). The discovery itself keeps retrying
/// every 5 s while usbmuxd is unreachable; the probe here only produces the
/// one-time `DriverMissing` notice with the install hint (Windows needs
/// Apple Mobile Device Support from iTunes / the Apple Devices app).
fn attach_ios(engine: &Engine) {
    let engine = engine.clone();
    tauri::async_runtime::spawn(async move {
        let discovery = MuxdDiscovery::new();
        match discovery.probe().await {
            Ok(()) => tracing::info!(addr = ?discovery.addr(), "ios: usbmuxd reachable"),
            Err(e) => {
                tracing::error!(code = "driver_missing", error = %e, "ios: usbmuxd unreachable");
                engine.notice(statasmon_protocol::Message::Error {
                    device_id: None,
                    code: e.code(),
                    message: e.to_string(),
                    hint: e.hint(),
                });
            }
        }
        engine.attach_discovery(Box::new(discovery));
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let mode = source_mode();
            // Store root: `STATASMON_STORE_DIR` (dev) or the app data folder
            // (`~/Library/Application Support/com.statasmon.app` on macOS).
            export_bundled_adb(app.path().resource_dir().ok());
            let app_data_dir = match app.path().app_data_dir() {
                Ok(dir) => Some(dir),
                Err(e) => {
                    tracing::error!(error = %e, "app data dir unavailable; sessions are not saved");
                    None
                }
            };
            let store_root = std::env::var_os("STATASMON_STORE_DIR")
                .map(PathBuf::from)
                .or_else(|| app_data_dir.clone());
            // Settings live next to the store (`settings.json`) and shape
            // the engine before it starts: sampling cadence, retention,
            // and the adb path (ADR-014).
            let settings_dir = store_root.clone();
            let settings = match &settings_dir {
                Some(dir) => settings::load(dir),
                None => statasmon_protocol::Settings::default(),
            };
            if let Some(root) = &store_root {
                tracing::info!(root = %root.display(), "session store root");
            }
            let config = settings::engine_config(store_root.as_deref(), &settings);
            let engine = tauri::async_runtime::block_on(async { Engine::new(config) });
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
                    attach_android(&engine, settings.adb_path.as_deref().map(PathBuf::from));
                    attach_ios(&engine);
                    tracing::info!("device source: real (android over adb, iOS over usbmuxd)");
                    None
                }
            };
            let update_check = settings.update_check;
            app.manage(AppState {
                engine,
                mode,
                fake,
                forwarder: Arc::new(Mutex::new(None)),
                app_data_dir,
                settings_dir,
                settings: Mutex::new(settings),
            });
            // Opt-in launch check (ADR-015): announces a new version as a
            // notice; installing is a separate, explicit command.
            if update_check {
                commands::spawn_startup_check(app.handle().clone());
            }
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
            commands::list_sessions,
            commands::open_session,
            commands::delete_session,
            commands::store_stats,
            commands::get_settings,
            commands::set_settings,
            commands::device_action,
            commands::fake_detach,
            commands::fake_reattach,
            commands::ui_report,
            commands::check_for_update,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("error while running StatasMon: {e}"));
}
