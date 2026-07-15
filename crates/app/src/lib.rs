//! Layer 4 — Frameworks & drivers (composition root + Tauri). The only crate that knows concrete
//! adapter types. `run()` initializes logging and the environment, then starts the Tauri shell
//! exposing the command handlers; `setup_state` loads configuration and wires the composition root
//! once Tauri can resolve the app-data directory.

mod commands;
mod composition_root;
mod config;
mod observability;

use tauri::Manager;

use composition_root::AppState;
use config::{AppConfig, API_KEY_ENV};

/// Entry point invoked by the binary: start the desktop app.
///
/// # Panics
/// Panics if the Tauri runtime fails to start. Configuration and database failures surface through
/// [`setup_state`] instead, so they are reported rather than unwinding from here.
pub fn run() {
    observability::init();
    load_dotenv();

    tauri::Builder::default()
        .setup(setup_state)
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::classify_all,
            commands::list_crates,
            commands::run_summary,
            commands::track_audit,
            commands::list_triage_queue,
            commands::apply_triage_action,
            commands::list_crate_options,
            commands::count_deferred,
            commands::resume_deferred,
            commands::get_settings,
            commands::preview_threshold,
            commands::update_threshold,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}

/// Loads `.env` into the process environment before any config read.
///
/// The documented setup copies `.env.example` to `.env` and puts the API key there; `std::env::var`
/// alone would never see it. A missing file is the normal case for a packaged build, not an error.
fn load_dotenv() {
    match dotenvy::dotenv() {
        Ok(path) => tracing::debug!(?path, "loaded .env"),
        Err(error) if error.not_found() => {
            tracing::debug!("no .env file found; using the process environment only");
        }
        Err(error) => {
            tracing::warn!(%error, "could not read .env; using the process environment only");
        }
    }
}

/// Tauri `setup` hook: resolves the app-data directory, loads configuration, and wires the
/// composition root into managed state.
///
/// # Errors
/// If the app-data directory cannot be resolved or created, or the database cannot be opened.
/// Returning here aborts startup through Tauri's error path rather than a bare panic.
fn setup_state(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let config = AppConfig::from_env(&data_dir);
    tracing::info!(
        has_api_key = config.has_api_key(),
        database = ?config.database_path(),
        "starting SoundCloud Crate Sorter"
    );
    if !config.has_api_key() {
        tracing::warn!(
            "{API_KEY_ENV} is not set: AI genre classification is unavailable, so every track \
             needing it is routed to triage. Set it in .env to enable classification."
        );
    }

    app.manage(AppState::build(&config)?);
    Ok(())
}
