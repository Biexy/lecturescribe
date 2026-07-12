use super::{artifact, filesystem_error, PipelineExecutor};
use crate::media::{atomic_write, atomic_write_json, sha256_file};
use crate::transcript::write_outputs;
use lecturescribe_core::{AppError, ArtifactKind, ErrorCategory, RunMode, TranscriptDocument};
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
            let _guard = self.output_lock.lock().map_err(|error| {
                AppError::new(
                    "output_lock_failed",
                    ErrorCategory::Internal,
                    "LectureScribe could not coordinate batch output.",
                    error.to_string(),
                )
            })?;
            return Ok(TaskExecutionResult {
                message: "Downloaded media saved".to_string(),
                artifacts: self.write_batch_summary(context, control, &[])?,
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
        let output_dir = Path::new(&context.plan.batch_output_dir);
        let written = write_outputs(
            &document,
            &output_dir.join("Transcripts"),
            &context.item.output_stem,
            &context.plan.settings,
            control,
        )?;
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
            if media != target && !target.exists() {
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
        artifacts.extend(self.write_batch_summary(context, control, &artifacts)?);
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
        let recorded = self
            .store
            .latest_artifact(&context.item.item.id, kind)?
            .filter(|artifact| Path::new(&artifact.path) == path)
            .ok_or_else(|| {
                AppError::new(
                    "cached_transcript_record_missing",
                    ErrorCategory::Filesystem,
                    "The previous transcript could not be verified for reuse.",
                    "No matching completed artifact record was found for the transcript path.",
                )
                .retryable("The source remains available for a new transcription.")
            })?;
        if recorded.checksum != checksum {
            return Err(AppError::new(
                "cached_transcript_checksum_mismatch",
                ErrorCategory::Filesystem,
                "The previous transcript changed and will not be reused.",
                "The transcript checksum no longer matches its completed artifact record.",
            )
            .retryable("The source remains available for a new transcription."));
        }
        let output_dir = Path::new(&context.plan.batch_output_dir).join("Transcripts");
        fs::create_dir_all(&output_dir).map_err(filesystem_error)?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("txt");
        let target = output_dir.join(format!("{}.{}", context.item.output_stem, extension));
        if path != target && !target.exists() {
            copy_atomic(&path, &target)?;
        }
        let output_path = if target.is_file() { target } else { path };
        let checksum = sha256_file(&output_path, control)?;
        let size = fs::metadata(&output_path).map_err(filesystem_error)?.len();
        let transcript_artifact =
            artifact(context, kind, &output_path, checksum, size, BTreeMap::new());
        let _guard = self.output_lock.lock().map_err(|error| {
            AppError::new(
                "output_lock_failed",
                ErrorCategory::Internal,
                "LectureScribe could not coordinate batch output.",
                error.to_string(),
            )
        })?;
        let mut artifacts = vec![transcript_artifact.clone()];
        artifacts.extend(self.write_batch_summary(context, control, &[transcript_artifact])?);
        Ok(TaskExecutionResult {
            message: "Verified transcript reused".to_string(),
            artifacts,
        })
    }

    fn write_batch_summary(
        &self,
        context: &TaskContext,
        control: &JobControl,
        current_artifacts: &[lecturescribe_core::ArtifactRecord],
    ) -> Result<Vec<lecturescribe_core::ArtifactRecord>, AppError> {
        let output_dir = Path::new(&context.plan.batch_output_dir);
        let snapshot = self.store.get_job_snapshot(&context.job_id)?;
        let mut rows = Vec::new();
        let mut manifest_items = Vec::new();

        for item in &context.plan.items {
            let snapshot_item = snapshot
                .items
                .iter()
                .find(|candidate| candidate.item.item.id == item.item.id);
            let mut artifacts = snapshot_item
                .map(|candidate| candidate.artifacts.clone())
                .unwrap_or_default();
            if item.item.id == context.item.item.id {
                artifacts.extend_from_slice(current_artifacts);
            }
            let languages = transcript_languages(&artifacts);
            let artifacts = relative_artifacts(output_dir, artifacts);
            let status = snapshot_item
                .map(|candidate| enum_label(candidate.state))
                .unwrap_or_else(|| "planned".to_string());
            let outcome = snapshot_item
                .and_then(|candidate| candidate.outcome)
                .map(enum_label);
            rows.push(BatchSummaryRow {
                title: public_title(&item.item.title),
                status: status.clone(),
                artifacts: artifacts.clone(),
            });
            manifest_items.push(serde_json::json!({
                "provider": enum_label(item.item.provider),
                "item_id": item.item.id,
                "title": public_title(&item.item.title),
                "status": status,
                "outcome": outcome,
                "languages": languages,
                "artifacts": artifacts,
            }));
        }

        let index_path = output_dir.join("00 - Batch summary.html");
        atomic_write(
            &index_path,
            render_batch_summary(&context.plan.batch_name, &rows).as_bytes(),
        )?;
        let manifest_path = output_dir.join("Metadata").join("batch-manifest.json");
        atomic_write_json(
            &manifest_path,
            &serde_json::json!({
                "schema_version": 1,
                "plan_id": context.plan.id,
                "batch_name": context.plan.batch_name,
                "mode": context.plan.mode,
                "created_at": context.plan.created_at,
                "updated_at": chrono::Utc::now(),
                "items": manifest_items,
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

#[derive(Debug, Clone, serde::Serialize)]
struct RelativeArtifact {
    kind: String,
    path: String,
    checksum: String,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct BatchSummaryRow {
    title: String,
    status: String,
    artifacts: Vec<RelativeArtifact>,
}

fn relative_artifacts(
    batch_dir: &Path,
    artifacts: Vec<lecturescribe_core::ArtifactRecord>,
) -> Vec<RelativeArtifact> {
    let mut output = artifacts
        .into_iter()
        .filter_map(|artifact| {
            let relative = Path::new(&artifact.path).strip_prefix(batch_dir).ok()?;
            let path = relative.to_string_lossy().replace('\\', "/");
            (!path.is_empty() && !path.starts_with("../")).then_some(RelativeArtifact {
                kind: enum_label(artifact.kind),
                path,
                checksum: artifact.checksum,
                size_bytes: artifact.size_bytes,
            })
        })
        .collect::<Vec<_>>();
    output.sort_by(|left, right| left.path.cmp(&right.path));
    output.dedup_by(|left, right| left.path == right.path && left.kind == right.kind);
    output
}

fn transcript_languages(artifacts: &[lecturescribe_core::ArtifactRecord]) -> Vec<String> {
    let Some(path) = artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.kind == ArtifactKind::CanonicalTranscript)
        .map(|artifact| PathBuf::from(&artifact.path))
    else {
        return Vec::new();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<TranscriptDocument>(&text).ok())
        .map(|document| document.languages)
        .unwrap_or_default()
}

fn enum_label(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn public_title(title: &str) -> String {
    let title = title.trim();
    if title.is_empty()
        || title.contains(":\\")
        || title.contains(":/")
        || title.starts_with("\\\\")
        || title.starts_with('/')
        || title.contains("://")
    {
        "Untitled item".to_string()
    } else {
        title.to_string()
    }
}

fn render_batch_summary(batch_name: &str, rows: &[BatchSummaryRow]) -> String {
    let rows = rows
        .iter()
        .map(|row| {
            let artifacts = row
                .artifacts
                .iter()
                .map(|artifact| {
                    format!(
                        "<a href=\"{}\">{}</a>",
                        escape_html(&artifact.path),
                        escape_html(&artifact.path)
                    )
                })
                .collect::<Vec<_>>()
                .join("<br>");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&row.status),
                escape_html(&row.title),
                artifacts
            )
        })
        .collect::<String>();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>body{{font-family:system-ui,sans-serif;margin:2rem;color:#1f2937}}main{{max-width:960px;margin:auto}}table{{border-collapse:collapse;width:100%}}th,td{{border-bottom:1px solid #d1d5db;padding:.65rem;text-align:left;vertical-align:top}}th{{font-weight:600}}a{{color:#075985}}</style></head><body><main><h1>{}</h1><table><thead><tr><th>Status</th><th>Title</th><th>Outputs</th></tr></thead><tbody>{}</tbody></table></main></body></html>\n",
        escape_html(batch_name),
        escape_html(batch_name),
        rows
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn copy_atomic(source: &Path, target: &Path) -> Result<(), AppError> {
    if target.exists() {
        return Ok(());
    }
    let temporary = target.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    fs::copy(source, &temporary).map_err(filesystem_error)?;
    match fs::rename(&temporary, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(temporary);
            Err(filesystem_error(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn batch_metadata_excludes_private_sources_and_personal_paths() {
        let batch = PathBuf::from("D:/Output/Batch");
        let artifacts = relative_artifacts(
            &batch,
            vec![lecturescribe_core::ArtifactRecord {
                id: "artifact".to_string(),
                job_id: "job".to_string(),
                item_id: "item".to_string(),
                task_id: "task".to_string(),
                kind: ArtifactKind::TextTranscript,
                path: "C:\\Users\\Alice\\private.txt".to_string(),
                checksum: "checksum".to_string(),
                size_bytes: 1,
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            }],
        );
        let summary = render_batch_summary(
            "Batch",
            &[BatchSummaryRow {
                title: public_title("https://drive.google.com/file/d/private-token"),
                status: "complete".to_string(),
                artifacts,
            }],
        );

        assert!(!summary.contains("drive.google.com"));
        assert!(!summary.contains("C:\\Users"));
        assert!(summary.contains("Untitled item"));
    }

    #[test]
    fn relative_artifacts_keep_only_paths_inside_the_batch() {
        let batch = PathBuf::from("D:/Output/Batch");
        let artifacts = relative_artifacts(
            &batch,
            vec![lecturescribe_core::ArtifactRecord {
                id: "artifact".to_string(),
                job_id: "job".to_string(),
                item_id: "item".to_string(),
                task_id: "task".to_string(),
                kind: ArtifactKind::TextTranscript,
                path: "D:/Output/Batch/Transcripts/Lecture.txt".to_string(),
                checksum: "checksum".to_string(),
                size_bytes: 1,
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            }],
        );

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, "Transcripts/Lecture.txt");
    }
}
