//! Layer 4 — Frameworks & drivers (composition root + Tauri). The only crate that knows concrete
//! adapter types. `run()` initializes logging, loads config, wires the composition root, and starts
//! the Tauri shell exposing the command handlers.

mod commands;
mod composition_root;
mod config;
mod observability;

use composition_root::AppState;
use config::AppConfig;

/// Entry point invoked by the binary: start the desktop app.
///
/// # Panics
/// Panics if the database cannot be opened or the Tauri runtime fails to start — both are
/// unrecoverable at launch.
pub fn run() {
    observability::init();

    let config = AppConfig::from_env();
    tracing::info!(
        has_api_key = config.has_api_key(),
        database = ?config.database_path(),
        "starting SoundCloud Crate Sorter"
    );

    let state = AppState::build(&config).expect("failed to build application state");

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::classify_all,
            commands::list_crates,
            commands::run_summary,
            commands::track_audit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}
