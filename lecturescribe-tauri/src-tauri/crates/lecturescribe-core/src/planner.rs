use crate::{
    stable_id, AppError, ErrorCategory, ItemState, PlanRequest, PlannedAction, PlannedItem,
    PreviewSnapshot, ProviderKind, ResourceClass, RunMode, RunPlan, TaskKind, TaskSpec,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::Path;
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
    let mut settings = request.settings.sanitized();
    if let Some(model_id) = request
        .overrides
        .model_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        settings.model = model_id.to_string();
        settings = settings.sanitized();
    }
    let created_at = Utc::now();
    let selected_items = preview
        .items
        .iter()
        .filter(|item| selected.contains(&item.id))
        .collect::<Vec<_>>();
    let batch_name = batch_name(
        &selected_items,
        request.overrides.batch_name.as_deref(),
        created_at,
    );
    let batch_name = unique_batch_name(&settings.output_dir, batch_name, &plan_id);
    let batch_output_dir = Path::new(&settings.output_dir)
        .join(&batch_name)
        .to_string_lossy()
        .to_string();
    let mut items = Vec::new();
    let mut estimated_requests = 0usize;
    let mut runnable_count = 0usize;
    let mut excluded_count = 0usize;
    let mut blocked_count = 0usize;
    let mut selected_count = 0usize;
    let mut capability_issues = Vec::<AppError>::new();

    for item in &preview.items {
        if !selected.contains(&item.id) {
            continue;
        }
        selected_count += 1;
        let ordinal = selected_count;
        let (mut action, mut reason) = planned_action(item, request.mode, &settings);
        let item_blockers = capability_blockers(action, &capabilities);
        if !item_blockers.is_empty() {
            reason = item_blockers
                .iter()
                .map(|error| error.user_message.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            for error in item_blockers {
                if !capability_issues
                    .iter()
                    .any(|known| known.code == error.code)
                {
                    capability_issues.push(error);
                }
            }
            action = PlannedAction::Blocked;
        }
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
            output_stem: String::new(),
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

    assign_output_stems(&mut items);

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
    if runnable_count == 0 {
        blocking_errors.extend(capability_issues);
    }

    Ok(RunPlan {
        id: plan_id,
        preview_id: preview.id.clone(),
        created_at,
        mode: request.mode,
        settings,
        batch_name,
        batch_output_dir,
        items,
        selected_count,
        runnable_count,
        excluded_count,
        blocked_count,
        estimated_requests,
        blocking_errors,
    })
}

fn batch_name(
    items: &[&crate::PreviewItem],
    override_name: Option<&str>,
    created_at: chrono::DateTime<Utc>,
) -> String {
    if let Some(name) = override_name.filter(|name| !name.trim().is_empty()) {
        return clean_output_name(name);
    }
    if let [item] = items {
        return clean_output_name(&item.title);
    }
    let common_group = items
        .first()
        .map(|item| item.source_group.trim())
        .filter(|group| !group.is_empty())
        .filter(|group| items.iter().all(|item| item.source_group.trim() == *group));
    common_group
        .map(clean_output_name)
        .unwrap_or_else(|| created_at.format("Batch %Y-%m-%d %H-%M").to_string())
}

fn unique_batch_name(output_dir: &str, base: String, plan_id: &str) -> String {
    let root = Path::new(output_dir);
    if !root.join(&base).exists() {
        return base;
    }
    for ordinal in 2..=999 {
        let candidate = format!("{base} ({ordinal})");
        if !root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{} [{}]", base, &plan_id[..8])
}

fn assign_output_stems(items: &mut [PlannedItem]) {
    let bases = items
        .iter()
        .map(|item| clean_output_name(&item.item.title))
        .collect::<Vec<_>>();
    let mut counts = HashMap::<String, usize>::new();
    for base in &bases {
        *counts.entry(base.to_ascii_lowercase()).or_default() += 1;
    }
    for (item, base) in items.iter_mut().zip(bases) {
        item.output_stem = if counts[&base.to_ascii_lowercase()] > 1 {
            format!(
                "{} [{}]",
                base,
                &stable_id("output-stem", &item.item.id)[..8]
            )
        } else {
            base
        };
    }
}

fn clean_output_name(value: &str) -> String {
    let value = value.trim();
    if value.is_empty()
        || value.contains(":\\")
        || value.contains(":/")
        || value.starts_with("\\\\")
        || value.starts_with('/')
        || value.contains("://")
    {
        return "Untitled item".to_string();
    }
    let mut cleaned = String::with_capacity(value.len());
    let mut previous_space = false;
    for character in value.chars() {
        let character = if character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) {
            ' '
        } else {
            character
        };
        if character.is_whitespace() {
            if !previous_space {
                cleaned.push(' ');
            }
            previous_space = true;
        } else {
            cleaned.push(character);
            previous_space = false;
        }
    }
    let cleaned = cleaned
        .trim_matches([' ', '.'])
        .chars()
        .take(100)
        .collect::<String>();
    if cleaned.is_empty() || is_windows_reserved_name(&cleaned) {
        "Untitled item".to_string()
    } else {
        cleaned
    }
}

fn is_windows_reserved_name(value: &str) -> bool {
    let value = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        value.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
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

fn capability_blockers(action: PlannedAction, capabilities: &PlanCapabilities) -> Vec<AppError> {
    let needs_transcription = matches!(
        action,
        PlannedAction::DownloadAndTranscribe
            | PlannedAction::ReuseMediaAndTranscribe
            | PlannedAction::TranscribeLocal
    );
    let needs_downloader = matches!(
        action,
        PlannedAction::DownloadAndTranscribe | PlannedAction::DownloadOnly
    );
    let mut blockers = Vec::new();
    if needs_transcription && !capabilities.api_key_ready {
        blockers.push(setup_error(
            "api_key_missing",
            "Add a Gemini API key before transcribing.",
            "open_setup_api",
            "Add API key",
        ));
    }
    if needs_transcription && !capabilities.ffmpeg_ready {
        blockers.push(setup_error(
            "ffmpeg_missing",
            "FFmpeg and FFprobe are required for transcription.",
            "open_setup_ffmpeg",
            "Fix FFmpeg",
        ));
    }
    if needs_downloader && !capabilities.downloader_ready {
        blockers.push(setup_error(
            "downloader_missing",
            "The Downloader is required for the selected link.",
            "open_setup_downloader",
            "Fix Downloader",
        ));
    }
    blockers
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
        TaskKind::Download => 3,
        // Gemini already applies bounded HTTP retries per segment. Retrying the whole
        // task would multiply requests; users can explicitly retry failed items.
        TaskKind::Transcribe => 1,
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
    use crate::{AppSettings, PreviewItem, ProviderKind, RunOverrides, SourceKind};
    use std::fs;

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
                overrides: RunOverrides::default(),
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
                overrides: RunOverrides::default(),
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

    #[test]
    fn download_plan_does_not_require_gemini() {
        let source = preview(vec![item("remote", ProviderKind::YouTube)]);
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["remote".to_string()],
                mode: RunMode::Download,
                settings: AppSettings::default(),
                overrides: RunOverrides::default(),
            },
            PlanCapabilities {
                api_key_ready: false,
                downloader_ready: true,
                output_ready: true,
                ..PlanCapabilities::default()
            },
        )
        .unwrap();
        assert!(plan.blocking_errors.is_empty());
        assert_eq!(plan.items[0].action, PlannedAction::DownloadOnly);
    }

    #[test]
    fn reused_transcript_does_not_require_gemini_or_ffmpeg() {
        let mut completed = item("completed", ProviderKind::Local);
        completed.existing_transcript_path = Some("completed.txt".to_string());
        let source = preview(vec![completed]);
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["completed".to_string()],
                mode: RunMode::Transcribe,
                settings: AppSettings::default(),
                overrides: RunOverrides::default(),
            },
            PlanCapabilities {
                output_ready: true,
                ..PlanCapabilities::default()
            },
        )
        .unwrap();
        assert!(plan.blocking_errors.is_empty());
        assert_eq!(plan.items[0].action, PlannedAction::ReuseTranscript);
    }

    #[test]
    fn missing_api_key_blocks_actual_transcription() {
        let source = preview(vec![item("local", ProviderKind::Local)]);
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["local".to_string()],
                mode: RunMode::Transcribe,
                settings: AppSettings::default(),
                overrides: RunOverrides::default(),
            },
            PlanCapabilities {
                ffmpeg_ready: true,
                output_ready: true,
                ..PlanCapabilities::default()
            },
        )
        .unwrap();
        assert_eq!(plan.blocking_errors.len(), 1);
        assert_eq!(plan.blocking_errors[0].code, "api_key_missing");
    }

    #[test]
    fn missing_downloader_blocks_only_remote_item_in_mixed_transcription() {
        let source = preview(vec![
            item("local", ProviderKind::Local),
            item("remote", ProviderKind::YouTube),
        ]);
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["local".to_string(), "remote".to_string()],
                mode: RunMode::Transcribe,
                settings: AppSettings::default(),
                overrides: RunOverrides::default(),
            },
            PlanCapabilities {
                api_key_ready: true,
                ffmpeg_ready: true,
                downloader_ready: false,
                output_ready: true,
            },
        )
        .unwrap();
        assert!(plan.blocking_errors.is_empty());
        assert_eq!(plan.runnable_count, 1);
        assert_eq!(plan.blocked_count, 1);
        assert_eq!(plan.items[0].action, PlannedAction::TranscribeLocal);
        assert_eq!(plan.items[1].action, PlannedAction::Blocked);
        assert!(plan.items[1].reason.contains("Downloader"));
    }

    #[test]
    fn batch_name_and_path_are_frozen_from_a_sanitized_override() {
        let source = preview(vec![item("item-1", ProviderKind::YouTube)]);
        let settings = AppSettings {
            output_dir: "Output root".to_string(),
            ..AppSettings::default()
        };
        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["item-1".to_string()],
                mode: RunMode::Transcribe,
                settings,
                overrides: RunOverrides {
                    batch_name: Some("Fall/2026: Week 1".to_string()),
                    ..RunOverrides::default()
                },
            },
            ready_capabilities(),
        )
        .unwrap();

        assert_eq!(plan.batch_name, "Fall 2026 Week 1");
        assert!(Path::new(&plan.batch_output_dir).ends_with(&plan.batch_name));
        assert_eq!(plan.batch_output_dir, plan.clone().batch_output_dir);
    }

    #[test]
    fn colliding_sanitized_titles_receive_stable_item_suffixes() {
        let mut first = item("first-id", ProviderKind::YouTube);
        first.title = "Lecture: Intro".to_string();
        let mut second = item("second-id", ProviderKind::YouTube);
        second.title = "Lecture? Intro".to_string();
        let source = preview(vec![first, second]);
        let request = PlanRequest {
            preview_id: source.id.clone(),
            selected_item_ids: vec!["first-id".to_string(), "second-id".to_string()],
            mode: RunMode::Transcribe,
            settings: AppSettings::default(),
            overrides: RunOverrides {
                batch_name: Some("Stable batch".to_string()),
                ..RunOverrides::default()
            },
        };

        let first_plan = build_plan(&source, request.clone(), ready_capabilities()).unwrap();
        let resumed_plan = first_plan.clone();
        let second_plan = build_plan(&source, request, ready_capabilities()).unwrap();
        let first_stems = first_plan
            .items
            .iter()
            .map(|item| item.output_stem.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            first_stems,
            resumed_plan
                .items
                .iter()
                .map(|item| item.output_stem.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            first_stems,
            second_plan
                .items
                .iter()
                .map(|item| item.output_stem.clone())
                .collect::<Vec<_>>()
        );
        assert_ne!(first_stems[0], first_stems[1]);
        assert!(first_stems
            .iter()
            .all(|stem| stem.starts_with("Lecture Intro [")));
    }

    #[test]
    fn existing_batch_folder_is_never_reused_by_a_new_plan() {
        let root =
            std::env::temp_dir().join(format!("lecturescribe-batch-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("item-1")).unwrap();
        let source = preview(vec![item("item-1", ProviderKind::YouTube)]);
        let settings = AppSettings {
            output_dir: root.to_string_lossy().to_string(),
            ..AppSettings::default()
        };

        let plan = build_plan(
            &source,
            PlanRequest {
                preview_id: source.id.clone(),
                selected_item_ids: vec!["item-1".to_string()],
                mode: RunMode::Transcribe,
                settings,
                overrides: RunOverrides::default(),
            },
            ready_capabilities(),
        )
        .unwrap();

        assert_eq!(plan.batch_name, "item-1 (2)");
        assert!(root.join("item-1").is_dir());
        let _ = fs::remove_dir_all(root);
    }

    fn ready_capabilities() -> PlanCapabilities {
        PlanCapabilities {
            api_key_ready: true,
            ffmpeg_ready: true,
            downloader_ready: true,
            output_ready: true,
        }
    }
}
