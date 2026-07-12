#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod commands;
mod migration;

use app_state::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = AppState::initialize(app.handle().clone())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::jobs::discover_automatic_sources,
            commands::jobs::inspect_link_file,
            commands::jobs::inspect_sources,
            commands::jobs::build_plan,
            commands::jobs::start_plan,
            commands::jobs::pause_job,
            commands::jobs::resume_job,
            commands::jobs::cancel_job,
            commands::jobs::retry_items,
            commands::files::get_job_snapshot,
            commands::files::events_since,
            commands::files::list_history,
            commands::files::unfinished_jobs,
            commands::setup::load_settings,
            commands::setup::save_settings,
            commands::setup::save_api_key,
            commands::setup::delete_api_key,
            commands::setup::check_environment,
            commands::setup::install_downloader,
            commands::setup::list_transcription_models,
            commands::setup::validate_transcription_model,
            commands::setup::run_setup_test,
            commands::files::open_known_link,
            commands::files::open_output_folder,
            commands::files::open_job_output,
            commands::files::open_artifact,
            commands::files::preview_diagnostics,
            commands::files::export_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LectureScribe");
}
