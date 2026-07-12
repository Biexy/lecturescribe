mod media_stages;
mod output_stages;
mod transcription_stages;

use crate::gemini::GeminiClient;
use crate::paths::AppPaths;
use crate::tools::ToolResolver;
use lecturescribe_core::{
    stable_id, AppError, ArtifactKind, ArtifactRecord, ErrorCategory, TaskKind,
    TRANSCRIPT_SCHEMA_VERSION,
};
use lecturescribe_engine::{
    JobControl, ProgressReporter, Store, TaskContext, TaskExecutionResult, TaskExecutor,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PipelineExecutor {
    pub(super) paths: AppPaths,
    pub(super) store: Arc<Store>,
    pub(super) tools: ToolResolver,
    pub(super) gemini: GeminiClient,
    pub(super) output_lock: Arc<Mutex<()>>,
}

impl PipelineExecutor {
    pub fn new(
        paths: AppPaths,
        store: Arc<Store>,
        tools: ToolResolver,
        gemini: GeminiClient,
    ) -> Self {
        Self {
            paths,
            store,
            tools,
            gemini,
            output_lock: Arc::new(Mutex::new(())),
        }
    }

    fn execute_task(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        control.checkpoint()?;
        match context.task.kind {
            TaskKind::Inspect => self.inspect(context),
            TaskKind::Download => self.download(context, reporter, control),
            TaskKind::Verify => self.verify(context, control),
            TaskKind::Prepare => self.prepare(context, reporter, control),
            TaskKind::Segment => self.segment(context, reporter, control),
            TaskKind::Transcribe => self.transcribe(context, reporter, control),
            TaskKind::Validate => self.validate(context),
            TaskKind::Merge => self.merge(context, control),
            TaskKind::Save => self.save(context, control),
            TaskKind::Reuse => self.reuse(context, control),
        }
    }

    pub(super) fn media_path(&self, context: &TaskContext) -> Result<PathBuf, AppError> {
        if let Some(path) = &context.item.item.media_path {
            return Ok(PathBuf::from(path));
        }
        if let Ok(artifact) = self.artifact_for(context, ArtifactKind::DownloadedMedia) {
            return Ok(PathBuf::from(artifact.path));
        }
        context
            .item
            .item
            .existing_media_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| {
                AppError::new(
                    "media_artifact_missing",
                    ErrorCategory::Media,
                    "The media file for this item is not available.",
                    "No local, downloaded, or verified media artifact was found.",
                )
            })
    }

    pub(super) fn artifact_for(
        &self,
        context: &TaskContext,
        kind: ArtifactKind,
    ) -> Result<ArtifactRecord, AppError> {
        self.store
            .artifacts_for_item(&context.job_id, &context.item.item.id)?
            .into_iter()
            .rev()
            .find(|artifact| artifact.kind == kind && Path::new(&artifact.path).is_file())
            .or_else(|| {
                self.store
                    .latest_artifact(&context.item.item.id, kind)
                    .ok()
                    .flatten()
                    .filter(|artifact| Path::new(&artifact.path).is_file())
            })
            .ok_or_else(|| {
                AppError::new(
                    "required_artifact_missing",
                    ErrorCategory::Filesystem,
                    "A verified intermediate file is missing.",
                    format!("Missing {kind:?} for {}", context.item.item.id),
                )
            })
    }

    pub(super) fn item_work_dir(&self, context: &TaskContext) -> PathBuf {
        self.paths.item_cache(&transcription_config_hash(context))
    }
}

impl TaskExecutor for PipelineExecutor {
    fn execute(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        self.execute_task(context, reporter, control)
    }
}

pub(super) fn transcription_config_hash(context: &TaskContext) -> String {
    stable_id(
        "transcription-cache-v2",
        &serde_json::json!({
            "item": context.item.item.canonical_source,
            "model": context.plan.settings.model,
            "language_preferences": context.plan.settings.language,
            "prompt": {
                "profile": context.plan.settings.prompt_preset,
                "additional": context.plan.settings.additional_prompt,
            },
            "segmentation": {
                "segment_minutes": context.plan.settings.segment_minutes,
                "overlap_seconds": context.plan.settings.overlap_seconds,
            },
            "transcript_schema_version": TRANSCRIPT_SCHEMA_VERSION,
        })
        .to_string(),
    )
}

pub(super) fn artifact(
    context: &TaskContext,
    kind: ArtifactKind,
    path: &Path,
    checksum: String,
    size_bytes: u64,
    metadata: BTreeMap<String, String>,
) -> ArtifactRecord {
    ArtifactRecord {
        id: stable_id(
            "artifact",
            &format!(
                "{}:{}:{kind:?}:{}",
                context.job_id,
                context.task.id,
                path.to_string_lossy()
            ),
        ),
        job_id: context.job_id.clone(),
        item_id: context.item.item.id.clone(),
        task_id: context.task.id.clone(),
        kind,
        path: path.to_string_lossy().to_string(),
        checksum,
        size_bytes,
        created_at: chrono::Utc::now(),
        metadata,
    }
}

pub(super) fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "pipeline_filesystem_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not read or write a pipeline file.",
        error.to_string(),
    )
}
