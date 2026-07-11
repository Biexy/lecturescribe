use super::{artifact, filesystem_error, PipelineExecutor};
use crate::media::{atomic_write, atomic_write_json, sha256_file};
use crate::paths::safe_component;
use crate::transcript::write_outputs;
use lecturescribe_core::{
    AppError, ArtifactKind, ErrorCategory, RunMode, TranscriptDocument, TranscriptFormat,
};
use lecturescribe_engine::{JobControl, TaskContext, TaskExecutionResult};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

impl PipelineExecutor {
    pub(super) fn save(
        &self,
        context: &TaskContext,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        if context.plan.mode == RunMode::Download {
            return Ok(TaskExecutionResult {
                message: "Downloaded media saved".to_string(),
                artifacts: Vec::new(),
            });
        }
        let canonical = self.artifact_for(context, ArtifactKind::CanonicalTranscript)?;
        let document: TranscriptDocument =
            serde_json::from_str(&fs::read_to_string(&canonical.path).map_err(filesystem_error)?)
                .map_err(|error| {
                AppError::new(
                    "canonical_transcript_invalid",
                    ErrorCategory::Transcription,
                    "The validated transcript file could not be read.",
                    error.to_string(),
                )
            })?;
        let _guard = self.output_lock.lock().map_err(|error| {
            AppError::new(
                "output_lock_failed",
                ErrorCategory::Internal,
                "LectureScribe could not coordinate transcript output.",
                error.to_string(),
            )
        })?;
        let output_dir = Path::new(&context.plan.settings.output_dir);
        let written = write_outputs(&document, output_dir, &context.plan.settings, control)?;
        let mut artifacts = written
            .into_iter()
            .map(|output| {
                artifact(
                    context,
                    output.kind,
                    &output.path,
                    output.checksum,
                    output.size_bytes,
                    BTreeMap::new(),
                )
            })
            .collect::<Vec<_>>();
        if context.plan.settings.keep_downloaded_media {
            let media = self.media_path(context)?;
            let media_dir = output_dir.join("Media");
            fs::create_dir_all(&media_dir).map_err(filesystem_error)?;
            let target =
                media_dir.join(media.file_name().unwrap_or_else(|| OsStr::new("media.bin")));
            if media != target {
                copy_atomic(&media, &target)?;
            }
            let checksum = sha256_file(&target, control)?;
            let size = fs::metadata(&target).map_err(filesystem_error)?.len();
            artifacts.push(artifact(
                context,
                ArtifactKind::DownloadedMedia,
                &target,
                checksum,
                size,
                BTreeMap::new(),
            ));
        }
        artifacts.extend(self.write_index(context, control)?);
        Ok(TaskExecutionResult {
            message: "Transcript outputs saved".to_string(),
            artifacts,
        })
    }

    pub(super) fn reuse(
        &self,
        context: &TaskContext,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let path = context
            .item
            .item
            .existing_transcript_path
            .as_ref()
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .ok_or_else(|| {
                AppError::new(
                    "cached_transcript_missing",
                    ErrorCategory::Filesystem,
                    "The cached transcript is no longer available.",
                    "Preview referenced a transcript that no longer exists.",
                )
                .retryable("The source remains available for a forced transcription.")
            })?;
        let kind = match path.extension().and_then(|value| value.to_str()) {
            Some("md") => ArtifactKind::MarkdownTranscript,
            Some("srt") => ArtifactKind::SrtTranscript,
            Some("vtt") => ArtifactKind::VttTranscript,
            _ => ArtifactKind::TextTranscript,
        };
        let checksum = sha256_file(&path, control)?;
        let size = fs::metadata(&path).map_err(filesystem_error)?.len();
        Ok(TaskExecutionResult {
            message: "Verified transcript reused".to_string(),
            artifacts: vec![artifact(
                context,
                kind,
                &path,
                checksum,
                size,
                BTreeMap::new(),
            )],
        })
    }

    fn write_index(
        &self,
        context: &TaskContext,
        control: &JobControl,
    ) -> Result<Vec<lecturescribe_core::ArtifactRecord>, AppError> {
        let output_dir = Path::new(&context.plan.settings.output_dir);
        let mut lines = vec![
            "# LectureScribe output".to_string(),
            String::new(),
            "| Status | Title | Outputs |".to_string(),
            "|---|---|---|".to_string(),
        ];
        for item in &context.plan.items {
            let base = format!(
                "{} [{}]",
                safe_component(&item.item.title),
                safe_component(&item.item.id)
            );
            let mut outputs = Vec::new();
            for format in &context.plan.settings.output_formats {
                let extension = extension_for(*format);
                let path = output_dir.join(format!("{base}.{extension}"));
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                        outputs.push(format!("[.{extension}]({name})"));
                    }
                }
            }
            lines.push(format!(
                "| {} | {} | {} |",
                if outputs.is_empty() {
                    "Pending"
                } else {
                    "Done"
                },
                item.item.title.replace('|', "\\|"),
                outputs.join(" ")
            ));
        }
        let index_path = output_dir.join("00_index.md");
        atomic_write(&index_path, format!("{}\n", lines.join("\n")).as_bytes())?;
        let manifest_path = output_dir.join("batch-manifest.json");
        atomic_write_json(
            &manifest_path,
            &serde_json::json!({
                "schema_version": 1,
                "plan_id": context.plan.id,
                "mode": context.plan.mode,
                "created_at": context.plan.created_at,
                "updated_at": chrono::Utc::now(),
                "items": context.plan.items,
            }),
        )?;
        let index_checksum = sha256_file(&index_path, control)?;
        let manifest_checksum = sha256_file(&manifest_path, control)?;
        Ok(vec![
            artifact(
                context,
                ArtifactKind::Index,
                &index_path,
                index_checksum,
                fs::metadata(&index_path).map_err(filesystem_error)?.len(),
                BTreeMap::new(),
            ),
            artifact(
                context,
                ArtifactKind::BatchManifest,
                &manifest_path,
                manifest_checksum,
                fs::metadata(&manifest_path)
                    .map_err(filesystem_error)?
                    .len(),
                BTreeMap::new(),
            ),
        ])
    }
}

fn extension_for(format: TranscriptFormat) -> &'static str {
    match format {
        TranscriptFormat::Text => "txt",
        TranscriptFormat::Markdown => "md",
        TranscriptFormat::Srt => "srt",
        TranscriptFormat::Vtt => "vtt",
    }
}

fn copy_atomic(source: &Path, target: &Path) -> Result<(), AppError> {
    let temporary = target.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    fs::copy(source, &temporary).map_err(filesystem_error)?;
    if target.exists() {
        fs::remove_file(target).map_err(filesystem_error)?;
    }
    fs::rename(temporary, target).map_err(filesystem_error)?;
    Ok(())
}
