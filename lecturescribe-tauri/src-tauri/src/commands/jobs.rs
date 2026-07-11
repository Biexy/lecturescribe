use super::blocking;
use crate::app_state::AppState;
use lecturescribe_core::planner::{build_plan as create_plan, PlanCapabilities};
use lecturescribe_core::{
    extract_urls, stable_id, AppError, ErrorCategory, InspectSourcesRequest, JobSnapshot, JobState,
    PlanRequest, PreviewSnapshot, RunPlan, SourceInput, SourceKind,
};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

const LINK_FILE_LIMIT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct StartPlanResponse {
    pub job_id: String,
    pub snapshot: JobSnapshot,
}

#[derive(Debug, Serialize)]
pub struct RetryResponse {
    pub job_id: String,
    pub reset_items: usize,
    pub snapshot: JobSnapshot,
}

#[derive(Debug, Serialize)]
pub struct SourceFileSummary {
    pub source: SourceInput,
    pub link_count: usize,
}

#[tauri::command]
pub async fn inspect_link_file(path: String) -> Result<SourceFileSummary, AppError> {
    blocking(move || summarize_link_file(Path::new(path.trim()), SourceKind::TextFile, false)).await
}

#[tauri::command]
pub async fn discover_automatic_sources() -> Result<Vec<SourceFileSummary>, AppError> {
    blocking(|| {
        let mut roots = Vec::new();
        if let Ok(current) = std::env::current_dir() {
            roots.push(current);
        }
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(project_root) = manifest.parent().and_then(Path::parent) {
            roots.push(project_root.to_path_buf());
        }
        let mut seen = HashSet::new();
        let mut summaries = Vec::new();
        for root in roots {
            for name in ["Drive links.txt", "links.txt"] {
                let path = root.join(name);
                let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if !path.is_file() || !seen.insert(identity) {
                    continue;
                }
                let summary = summarize_link_file(&path, SourceKind::AutomaticFile, true)?;
                if summary.link_count > 0 {
                    summaries.push(summary);
                }
            }
        }
        Ok(summaries)
    })
    .await
}

#[tauri::command]
pub async fn inspect_sources(
    request: InspectSourcesRequest,
    state: State<'_, AppState>,
) -> Result<PreviewSnapshot, AppError> {
    let inspector = state.inspector.clone();
    let settings = state.settings()?;
    blocking(move || inspector.inspect(request, &settings)).await
}

#[tauri::command]
pub async fn build_plan(
    mut request: PlanRequest,
    state: State<'_, AppState>,
) -> Result<RunPlan, AppError> {
    let store = state.store.clone();
    let tools = state.tools.clone();
    let paths = state.paths.clone();
    let credentials = state.credentials.clone();
    blocking(move || {
        request.settings = paths.settings_with_defaults(request.settings);
        let preview = store.get_preview(&request.preview_id)?;
        let resolved = tools.resolve(&request.settings);
        let output_ready =
            tools.output_writable(&PathBuf::from(request.settings.output_dir.clone()));
        let capabilities = PlanCapabilities {
            api_key_ready: credentials.configured(),
            ffmpeg_ready: resolved.ffmpeg.path.is_some() && resolved.ffprobe.path.is_some(),
            downloader_ready: resolved.downloader.path.is_some(),
            output_ready,
        };
        let plan = create_plan(&preview, request, capabilities)?;
        store.save_settings(&plan.settings)?;
        store.save_plan(&plan)?;
        Ok(plan)
    })
    .await
}

#[tauri::command]
pub fn start_plan(
    plan_id: String,
    state: State<'_, AppState>,
) -> Result<StartPlanResponse, AppError> {
    let plan = state.store.get_plan(&plan_id)?;
    if let Some(error) = plan.blocking_errors.first() {
        return Err(error.clone());
    }
    if plan.runnable_count == 0 {
        return Err(AppError::new(
            "plan_has_no_runnable_items",
            ErrorCategory::Input,
            "Nothing in this plan can run.",
            "The immutable plan contained no runnable items.",
        ));
    }
    let (job_id, control) = state.runner.start(plan)?;
    state.set_control(job_id.clone(), control);
    let snapshot = state.store.get_job_snapshot(&job_id)?;
    Ok(StartPlanResponse { job_id, snapshot })
}

#[tauri::command]
pub fn pause_job(job_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    let control = state.control(&job_id).ok_or_else(|| {
        AppError::new(
            "job_control_missing",
            ErrorCategory::Input,
            "This run is not active in the current app session.",
            "No live JobControl matched the requested job ID.",
        )
    })?;
    control.pause();
    Ok(())
}

#[tauri::command]
pub fn resume_job(job_id: String, state: State<'_, AppState>) -> Result<JobSnapshot, AppError> {
    let snapshot = state.store.get_job_snapshot(&job_id)?;
    if let Some(control) = state.control(&job_id) {
        control.resume();
        return state.store.get_job_snapshot(&job_id);
    }
    if !matches!(
        snapshot.state,
        JobState::Paused | JobState::Waiting | JobState::Interrupted
    ) {
        return Err(AppError::new(
            "job_not_resumable",
            ErrorCategory::Input,
            "This run does not need to be resumed.",
            format!("Job state was {:?}.", snapshot.state),
        ));
    }
    let control = state.runner.resume(job_id.clone())?;
    state.set_control(job_id.clone(), control);
    state.store.get_job_snapshot(&job_id)
}

#[tauri::command]
pub fn cancel_job(job_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    let control = state.control(&job_id).ok_or_else(|| {
        AppError::new(
            "job_control_missing",
            ErrorCategory::Input,
            "This run is not active in the current app session.",
            "No live JobControl matched the requested job ID.",
        )
    })?;
    control.cancel();
    Ok(())
}

#[tauri::command]
pub fn retry_items(job_id: String, state: State<'_, AppState>) -> Result<RetryResponse, AppError> {
    let snapshot = state.store.get_job_snapshot(&job_id)?;
    if !matches!(
        snapshot.state,
        JobState::Complete | JobState::Failed | JobState::Cancelled
    ) {
        return Err(AppError::new(
            "job_retry_while_active",
            ErrorCategory::Input,
            "Wait for the current run to stop before retrying failed items.",
            format!("Job state was {:?}.", snapshot.state),
        ));
    }
    let reset_items = state.store.reset_failed_items(&job_id)?;
    if reset_items == 0 {
        return Err(AppError::new(
            "job_has_no_failed_items",
            ErrorCategory::Input,
            "There are no failed items to retry.",
            "The job ledger did not contain any failed outcomes.",
        ));
    }
    state.remove_control(&job_id);
    let control = state.runner.resume(job_id.clone())?;
    state.set_control(job_id.clone(), control);
    let snapshot = state.store.get_job_snapshot(&job_id)?;
    Ok(RetryResponse {
        job_id,
        reset_items,
        snapshot,
    })
}
fn summarize_link_file(
    path: &Path,
    kind: SourceKind,
    automatic: bool,
) -> Result<SourceFileSummary, AppError> {
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
    {
        return Err(AppError::new(
            "link_file_extension_invalid",
            ErrorCategory::Input,
            "Choose a .txt link file.",
            "The selected file did not use the .txt extension.",
        ));
    }
    let metadata = fs::metadata(path).map_err(|error| {
        AppError::new(
            "link_file_unreadable",
            ErrorCategory::Input,
            "LectureScribe could not read this link file.",
            error.to_string(),
        )
    })?;
    if metadata.len() > LINK_FILE_LIMIT_BYTES {
        return Err(AppError::new(
            "link_file_too_large",
            ErrorCategory::Input,
            "This link file is unexpectedly large.",
            format!(
                "The file is {} bytes; the limit is {LINK_FILE_LIMIT_BYTES}.",
                metadata.len()
            ),
        ));
    }
    let text = fs::read_to_string(path).map_err(|error| {
        AppError::new(
            "link_file_unreadable",
            ErrorCategory::Input,
            "LectureScribe could not read this link file.",
            error.to_string(),
        )
    })?;
    let links = extract_urls(&text);
    let value = path.to_string_lossy().to_string();
    let label = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Link file")
        .to_string();
    Ok(SourceFileSummary {
        source: SourceInput {
            id: stable_id("source-file", &value),
            kind,
            value,
            label,
            automatic,
        },
        link_count: links.len(),
    })
}
