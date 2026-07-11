use crate::{
    stable_id, AppError, ErrorCategory, ItemState, PlanRequest, PlannedAction, PlannedItem,
    PreviewSnapshot, ProviderKind, ResourceClass, RunMode, RunPlan, TaskKind, TaskSpec,
};
use chrono::Utc;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct PlanCapabilities {
    pub api_key_ready: bool,
    pub ffmpeg_ready: bool,
    pub downloader_ready: bool,
    pub output_ready: bool,
}

pub fn build_plan(
    preview: &PreviewSnapshot,
    request: PlanRequest,
    capabilities: PlanCapabilities,
) -> Result<RunPlan, AppError> {
    if request.preview_id != preview.id {
        return Err(AppError::new(
            "plan_preview_mismatch",
            ErrorCategory::Input,
            "The queue changed. Review the refreshed preview before starting.",
            "Plan request referenced a different preview ID.",
        ));
    }

    let selected = request
        .selected_item_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    if selected.is_empty() {
        return Err(AppError::new(
            "plan_empty_selection",
            ErrorCategory::Input,
            "Select at least one queue item.",
            "No item IDs were supplied to the planner.",
        ));
    }

    let plan_id = Uuid::new_v4().to_string();
    let settings = request.settings.sanitized();
    let mut items = Vec::new();
    let mut estimated_requests = 0usize;
    let mut runnable_count = 0usize;
    let mut excluded_count = 0usize;
    let mut blocked_count = 0usize;
    let mut selected_count = 0usize;

    for item in &preview.items {
        if !selected.contains(&item.id) {
            continue;
        }
        selected_count += 1;
        let ordinal = selected_count;
        let (action, reason) = planned_action(item, request.mode, &settings);
        let estimated_segments = item
            .duration_seconds
            .map(|seconds| (seconds / (settings.segment_minutes as f64 * 60.0)).ceil() as usize)
            .unwrap_or(1)
            .max(1);
        let requests = if matches!(
            action,
            PlannedAction::DownloadAndTranscribe
                | PlannedAction::ReuseMediaAndTranscribe
                | PlannedAction::TranscribeLocal
        ) {
            estimated_segments
        } else {
            0
        };
        estimated_requests += requests;

        let mut planned = PlannedItem {
            item: item.clone(),
            ordinal,
            action,
            reason,
            estimated_segments,
            estimated_requests: requests,
            tasks: Vec::new(),
        };
        match action {
            PlannedAction::Excluded => excluded_count += 1,
            PlannedAction::Blocked => blocked_count += 1,
            _ => {
                runnable_count += 1;
                planned.tasks = build_tasks(&plan_id, &planned);
            }
        }
        items.push(planned);
    }

    if selected_count != selected.len() {
        return Err(AppError::new(
            "plan_stale_selection",
            ErrorCategory::Input,
            "Some selected items no longer exist. Review the queue again.",
            "One or more selected item IDs were not present in the preview.",
        ));
    }

    let mut blocking_errors = Vec::new();
    if !capabilities.output_ready {
        blocking_errors.push(setup_error(
            "output_not_writable",
            "The output folder is not writable.",
            "choose_output_folder",
            "Choose output folder",
        ));
    }
    if request.mode == RunMode::Transcribe && !capabilities.api_key_ready {
        blocking_errors.push(setup_error(
            "api_key_missing",
            "Add a Gemini API key before transcribing.",
            "open_setup_api",
            "Add API key",
        ));
    }
    if request.mode == RunMode::Transcribe && !capabilities.ffmpeg_ready {
        blocking_errors.push(setup_error(
            "ffmpeg_missing",
            "FFmpeg and FFprobe are required for transcription.",
            "open_setup_ffmpeg",
            "Fix FFmpeg",
        ));
    }
    let needs_downloader = items.iter().any(|item| {
        matches!(
            item.action,
            PlannedAction::DownloadAndTranscribe | PlannedAction::DownloadOnly
        )
    });
    if needs_downloader && !capabilities.downloader_ready {
        blocking_errors.push(setup_error(
            "downloader_missing",
            "The Downloader is required for the selected links.",
            "open_setup_downloader",
            "Fix Downloader",
        ));
    }

    Ok(RunPlan {
        id: plan_id,
        preview_id: preview.id.clone(),
        created_at: Utc::now(),
        mode: request.mode,
        settings,
        items,
        selected_count,
        runnable_count,
        excluded_count,
        blocked_count,
        estimated_requests,
        blocking_errors,
    })
}

fn planned_action(
    item: &crate::PreviewItem,
    mode: RunMode,
    settings: &crate::AppSettings,
) -> (PlannedAction, String) {
    if item.duplicate_of.is_some() {
        return (
            PlannedAction::Excluded,
            "Duplicate of another queue item".to_string(),
        );
    }
    if item.status == ItemState::Blocked || item.error.is_some() {
        return (
            PlannedAction::Blocked,
            item.error
                .as_ref()
                .map(|error| error.user_message.clone())
                .unwrap_or_else(|| "This item needs attention".to_string()),
        );
    }

    match mode {
        RunMode::Download => {
            if item.provider == ProviderKind::Local {
                (
                    PlannedAction::Excluded,
                    "Already local; no download is needed".to_string(),
                )
            } else {
                (
                    PlannedAction::DownloadOnly,
                    "Download original media".to_string(),
                )
            }
        }
        RunMode::Transcribe => {
            if item.existing_transcript_path.is_some() && !settings.force {
                (
                    PlannedAction::ReuseTranscript,
                    "Reuse verified completed transcript".to_string(),
                )
            } else if item.provider == ProviderKind::Local {
                (
                    PlannedAction::TranscribeLocal,
                    "Prepare and transcribe local media".to_string(),
                )
            } else if item.existing_media_path.is_some() {
                (
                    PlannedAction::ReuseMediaAndTranscribe,
                    "Reuse downloaded media and transcribe".to_string(),
                )
            } else {
                (
                    PlannedAction::DownloadAndTranscribe,
                    "Download missing media and transcribe".to_string(),
                )
            }
        }
    }
}

fn build_tasks(plan_id: &str, item: &PlannedItem) -> Vec<TaskSpec> {
    let kinds = match item.action {
        PlannedAction::DownloadAndTranscribe => vec![
            TaskKind::Inspect,
            TaskKind::Download,
            TaskKind::Verify,
            TaskKind::Prepare,
            TaskKind::Segment,
            TaskKind::Transcribe,
            TaskKind::Validate,
            TaskKind::Merge,
            TaskKind::Save,
        ],
        PlannedAction::ReuseMediaAndTranscribe | PlannedAction::TranscribeLocal => vec![
            TaskKind::Inspect,
            TaskKind::Verify,
            TaskKind::Prepare,
            TaskKind::Segment,
            TaskKind::Transcribe,
            TaskKind::Validate,
            TaskKind::Merge,
            TaskKind::Save,
        ],
        PlannedAction::DownloadOnly => vec![
            TaskKind::Inspect,
            TaskKind::Download,
            TaskKind::Verify,
            TaskKind::Save,
        ],
        PlannedAction::ReuseTranscript => vec![TaskKind::Reuse],
        PlannedAction::Excluded | PlannedAction::Blocked => Vec::new(),
    };
    let mut previous = Vec::<String>::new();
    kinds
        .into_iter()
        .map(|kind| {
            let id = stable_id("task", &format!("{plan_id}:{}:{kind:?}", item.item.id));
            let task = TaskSpec {
                id: id.clone(),
                item_id: item.item.id.clone(),
                kind,
                resource: resource_for(kind),
                depends_on: previous.clone(),
                idempotency_key: stable_id("idempotency", &format!("{}:{kind:?}:v1", item.item.id)),
                max_attempts: max_attempts(kind),
                weight: weight_for(item.action, kind),
            };
            previous = vec![id];
            task
        })
        .collect()
}

fn resource_for(kind: TaskKind) -> ResourceClass {
    match kind {
        TaskKind::Inspect => ResourceClass::Metadata,
        TaskKind::Download => ResourceClass::Download,
        TaskKind::Verify | TaskKind::Prepare | TaskKind::Segment => ResourceClass::Ffmpeg,
        TaskKind::Transcribe => ResourceClass::Gemini,
        TaskKind::Validate | TaskKind::Merge | TaskKind::Save | TaskKind::Reuse => {
            ResourceClass::Filesystem
        }
    }
}

fn max_attempts(kind: TaskKind) -> u32 {
    match kind {
        TaskKind::Download | TaskKind::Transcribe => 3,
        TaskKind::Inspect | TaskKind::Verify => 2,
        _ => 1,
    }
}

fn weight_for(action: PlannedAction, kind: TaskKind) -> f64 {
    match (action, kind) {
        (_, TaskKind::Inspect) => 0.03,
        (PlannedAction::DownloadOnly, TaskKind::Download) => 0.85,
        (PlannedAction::DownloadOnly, TaskKind::Verify | TaskKind::Save) => 0.06,
        (PlannedAction::DownloadAndTranscribe, TaskKind::Download) => 0.20,
        (_, TaskKind::Verify) => 0.04,
        (_, TaskKind::Prepare) => 0.08,
        (_, TaskKind::Segment) => 0.08,
        (_, TaskKind::Transcribe) => 0.50,
        (_, TaskKind::Validate) => 0.02,
        (_, TaskKind::Merge) => 0.02,
        (_, TaskKind::Save) => 0.03,
        (_, TaskKind::Reuse) => 1.0,
        _ => 0.01,
    }
}

fn setup_error(code: &str, message: &str, action: &str, label: &str) -> AppError {
    AppError::new(code, ErrorCategory::Setup, message, message).with_action(action, label, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppSettings, PreviewItem, ProviderKind, SourceKind};

    fn preview(items: Vec<PreviewItem>) -> PreviewSnapshot {
        PreviewSnapshot {
            id: "preview-1".to_string(),
            created_at: Utc::now(),
            source_count: items.len(),
            items,
            duplicate_count: 0,
            warnings: Vec::new(),
        }
    }

    fn item(id: &str, provider: ProviderKind) -> PreviewItem {
        PreviewItem {
            id: id.to_string(),
            source_id: "source".to_string(),
            source_kind: SourceKind::PastedLink,
            provider,
            source_group: "Test".to_string(),
            title: id.to_string(),
            source: id.to_string(),
            canonical_source: id.to_string(),
            url: (provider != ProviderKind::Local).then(|| id.to_string()),
            media_path: (provider == ProviderKind::Local).then(|| id.to_string()),
            existing_media_path: None,
            existing_transcript_path: None,
            thumbnail_url: None,
            duration_seconds: Some(3600.0),
            expected_media_name: None,
            selected: true,
            status: ItemState::Ready,
            duplicate_of: None,
            error: None,
        }
    }

    #[test]
    fn one_selected_row_keeps_its_identity() {
        let source = preview(vec![
            item("item-1", ProviderKind::YouTube),
            item("item-22", ProviderKind::GoogleDrive),
        ]);
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["item-22".to_string()],
                mode: RunMode::Transcribe,
                settings: AppSettings::default(),
            },
            PlanCapabilities {
                api_key_ready: true,
                ffmpeg_ready: true,
                downloader_ready: true,
                output_ready: true,
            },
        )
        .unwrap();
        assert_eq!(plan.items[0].item.id, "item-22");
        assert_eq!(plan.items[0].ordinal, 1);
        assert!(plan.items[0]
            .tasks
            .iter()
            .all(|task| task.item_id == "item-22"));
    }

    #[test]
    fn download_mode_excludes_local_media() {
        let source = preview(vec![item("local", ProviderKind::Local)]);
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["local".to_string()],
                mode: RunMode::Download,
                settings: AppSettings::default(),
            },
            PlanCapabilities {
                output_ready: true,
                ..PlanCapabilities::default()
            },
        )
        .unwrap();
        assert_eq!(plan.runnable_count, 0);
        assert_eq!(plan.excluded_count, 1);
        assert_eq!(plan.items[0].action, PlannedAction::Excluded);
    }
}
