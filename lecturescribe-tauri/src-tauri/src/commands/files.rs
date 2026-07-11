use super::blocking;
use super::setup::collect_environment;
use crate::app_state::AppState;
use lecturescribe_adapters::diagnostics::{
    build_diagnostic_report, write_diagnostic_report, DiagnosticReport,
};
use lecturescribe_core::{
    AppError, AppEvent, ArtifactKind, ErrorCategory, HistoryEntry, JobSnapshot, JobState,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct DiagnosticExport {
    pub path: String,
    pub report: DiagnosticReport,
}

#[tauri::command]
pub fn get_job_snapshot(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<JobSnapshot, AppError> {
    let snapshot = state.store.get_job_snapshot(&job_id)?;
    if matches!(
        snapshot.state,
        JobState::Complete | JobState::Failed | JobState::Cancelled
    ) {
        state.remove_control(&job_id);
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn events_since(
    job_id: String,
    sequence: i64,
    state: State<'_, AppState>,
) -> Result<Vec<AppEvent>, AppError> {
    state.store.events_since(&job_id, sequence)
}

#[tauri::command]
pub fn list_history(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<HistoryEntry>, AppError> {
    state.store.list_history(limit.unwrap_or(50).clamp(1, 100))
}

#[tauri::command]
pub fn unfinished_jobs(state: State<'_, AppState>) -> Result<Vec<JobSnapshot>, AppError> {
    state.store.unfinished_jobs()
}

#[tauri::command]
pub fn open_known_link(target: String) -> Result<String, AppError> {
    let url = match target.as_str() {
        "ai_studio" => "https://aistudio.google.com/app/apikey",
        "github" => "https://github.com/Biexy/lecturescribe",
        "ffmpeg" => "https://ffmpeg.org/download.html",
        "yt_dlp" => "https://github.com/yt-dlp/yt-dlp",
        _ => {
            return Err(AppError::new(
                "external_link_not_allowed",
                ErrorCategory::Input,
                "That external link is not available.",
                "The requested target was not in the LectureScribe allowlist.",
            ))
        }
    };
    open_url(url)?;
    Ok(url.to_string())
}

#[tauri::command]
pub fn open_output_folder(state: State<'_, AppState>) -> Result<String, AppError> {
    let path = PathBuf::from(state.settings()?.output_dir);
    fs::create_dir_all(&path).map_err(filesystem_error)?;
    open_path(&path, false)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_artifact(
    job_id: String,
    item_id: String,
    kind: ArtifactKind,
    reveal: bool,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let artifact = state
        .store
        .artifacts_for_item(&job_id, &item_id)?
        .into_iter()
        .rev()
        .find(|artifact| artifact.kind == kind)
        .ok_or_else(|| {
            AppError::new(
                "artifact_not_found",
                ErrorCategory::Filesystem,
                "That output is not available yet.",
                format!("No {kind:?} artifact was recorded for item {item_id}."),
            )
        })?;
    let path = PathBuf::from(&artifact.path);
    if !path.exists() {
        return Err(AppError::new(
            "artifact_file_missing",
            ErrorCategory::Filesystem,
            "That output file has moved or was deleted.",
            artifact.path,
        ));
    }
    open_path(&path, reveal)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn preview_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticReport, AppError> {
    diagnostic_report(&state).await
}

#[tauri::command]
pub async fn export_diagnostics(
    destination: String,
    state: State<'_, AppState>,
) -> Result<DiagnosticExport, AppError> {
    let destination = diagnostic_destination(&destination)?;
    let report = diagnostic_report(&state).await?;
    let report_to_write = report.clone();
    let output = destination.clone();
    blocking(move || {
        write_diagnostic_report(&output, &report_to_write).map_err(filesystem_error)?;
        Ok(())
    })
    .await?;
    Ok(DiagnosticExport {
        path: destination.to_string_lossy().to_string(),
        report,
    })
}

async fn diagnostic_report(state: &State<'_, AppState>) -> Result<DiagnosticReport, AppError> {
    let store = state.store.clone();
    let credentials = state.credentials.clone();
    let tools = state.tools.clone();
    let settings = state.settings()?;
    let verified = state.api_verified();
    let logger = state.logger.clone();
    blocking(move || {
        let environment = collect_environment(
            store.clone(),
            credentials,
            tools,
            settings.clone(),
            verified,
        )?;
        let history = store.list_history(20)?;
        Ok(build_diagnostic_report(
            environment,
            &settings,
            history,
            logger.recent_lines(200),
        ))
    })
    .await
}

fn diagnostic_destination(value: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(value.trim());
    if path.as_os_str().is_empty()
        || path.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(AppError::new(
            "diagnostic_path_invalid",
            ErrorCategory::Filesystem,
            "Choose a .json file for the diagnostic report.",
            "The destination was empty or did not use the .json extension.",
        ));
    }
    Ok(path)
}

fn open_path(path: &Path, reveal: bool) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        if reveal && path.is_file() {
            command.arg("/select,");
        }
        command.arg(path);
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let target = if reveal && path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let mut command = Command::new("xdg-open");
        command.arg(target);
        command
    };
    command.spawn().map(|_| ()).map_err(filesystem_error)
}

fn open_url(url: &str) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer.exe");
    #[cfg(not(target_os = "windows"))]
    let mut command = Command::new("xdg-open");
    command
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(filesystem_error)
}

fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "filesystem_operation_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not complete the file operation.",
        error.to_string(),
    )
}
