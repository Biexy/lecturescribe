use super::{artifact, filesystem_error, transcription_config_hash, PipelineExecutor};
use crate::gemini::SegmentTranscript;
use crate::media::{
    atomic_write_json, segment_audio, sha256_file, SegmentDescriptor, SegmentManifest,
};
use crate::transcript::{canonical_path, merge_transcripts};
use lecturescribe_core::{
    stable_id, AppError, ArtifactKind, ErrorCategory, ProgressKind, ProgressMetric,
};
use lecturescribe_engine::{
    CacheEntry, JobControl, ProgressReporter, TaskContext, TaskExecutionResult,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

impl PipelineExecutor {
    pub(super) fn transcribe(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let manifest = self.segment_manifest(context)?;
        let settings = &context.plan.settings;
        let config_hash = transcription_config_hash(context);
        let transcript_dir = self.item_work_dir(context).join("segment-transcripts");
        fs::create_dir_all(&transcript_dir).map_err(filesystem_error)?;
        let mut artifacts = Vec::new();
        for (offset, descriptor) in manifest.segments.iter().enumerate() {
            control.checkpoint()?;
            let path = transcript_dir.join(format!("segment_{:04}.json", descriptor.index));
            let transcript = match read_cached_segment(&path, &config_hash, &descriptor.checksum) {
                Some(transcript) if !settings.force => transcript,
                _ => {
                    let result = self.gemini.transcribe_segment(
                        descriptor,
                        &context.item.item.title,
                        settings,
                        control,
                        reporter,
                    );
                    let transcript = match result {
                        Ok(transcript) => transcript,
                        Err(error) if should_split_after_error(&error) => {
                            self.transcribe_split_fallback(context, descriptor, reporter, control)?
                        }
                        Err(error) => return Err(error),
                    };
                    atomic_write_json(
                        &path,
                        &CachedSegmentTranscript {
                            schema_version: 1,
                            config_hash: config_hash.clone(),
                            segment_checksum: descriptor.checksum.clone(),
                            transcript: transcript.clone(),
                        },
                    )?;
                    transcript
                }
            };
            let checksum = sha256_file(&path, control)?;
            let size = fs::metadata(&path).map_err(filesystem_error)?.len();
            let mut metadata = BTreeMap::new();
            metadata.insert("segment_index".to_string(), descriptor.index.to_string());
            metadata.insert("config_hash".to_string(), config_hash.clone());
            let item_artifact = artifact(
                context,
                ArtifactKind::SegmentTranscript,
                &path,
                checksum.clone(),
                size,
                metadata,
            );
            self.store.put_cache(&CacheEntry {
                cache_key: stable_id(
                    "segment-cache",
                    &format!("{}:{}", descriptor.checksum, config_hash),
                ),
                item_id: context.item.item.id.clone(),
                kind: ArtifactKind::SegmentTranscript,
                path: path.to_string_lossy().to_string(),
                checksum,
                size_bytes: size,
                completed: true,
                last_used_at: chrono::Utc::now(),
                metadata: serde_json::json!({
                    "segment_index": descriptor.index,
                    "speech_segments": transcript.segments.len()
                }),
            })?;
            artifacts.push(item_artifact);
            reporter.report(
                ProgressMetric {
                    kind: ProgressKind::Segments,
                    current: (offset + 1) as f64,
                    total: Some(manifest.segments.len() as f64),
                    unit: "segments".to_string(),
                    rate: None,
                    eta_seconds: None,
                },
                &format!(
                    "Transcribed segment {} of {}",
                    offset + 1,
                    manifest.segments.len()
                ),
            );
        }
        Ok(TaskExecutionResult {
            message: format!("{} segments transcribed", manifest.segments.len()),
            artifacts,
        })
    }

    fn transcribe_split_fallback(
        &self,
        context: &TaskContext,
        descriptor: &SegmentDescriptor,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<SegmentTranscript, AppError> {
        let settings = &context.plan.settings;
        let tools = self.tools.resolve(settings);
        let ffmpeg = tools.ffmpeg.path.ok_or_else(|| {
            AppError::new(
                "ffmpeg_missing",
                ErrorCategory::Setup,
                "FFmpeg is required to split a rejected segment.",
                tools.ffmpeg.status.detail,
            )
        })?;
        let fallback_dir = self
            .item_work_dir(context)
            .join("fallback-segments")
            .join(format!("{:04}", descriptor.index));
        let duration = (descriptor.end_seconds - descriptor.start_seconds).max(60.0);
        let target = (duration / 2.0).clamp(300.0, 600.0) as u32;
        let manifest = segment_audio(
            &ffmpeg,
            Path::new(&descriptor.path),
            &fallback_dir,
            target,
            settings.overlap_seconds,
            control,
            reporter,
        )?;
        let mut all = Vec::new();
        let mut language = "unknown".to_string();
        for mut child in manifest.segments {
            child.start_seconds += descriptor.start_seconds;
            child.end_seconds =
                (child.end_seconds + descriptor.start_seconds).min(descriptor.end_seconds);
            let transcript = self.gemini.transcribe_segment(
                &child,
                &context.item.item.title,
                settings,
                control,
                reporter,
            )?;
            if transcript.language != "unknown" {
                language = transcript.language.clone();
            }
            all.extend(transcript.segments);
        }
        Ok(SegmentTranscript {
            language,
            segments: all,
        })
    }

    pub(super) fn validate(&self, context: &TaskContext) -> Result<TaskExecutionResult, AppError> {
        let transcripts = self.load_segment_transcripts(context)?;
        if transcripts.iter().all(|value| value.segments.is_empty()) {
            return Err(AppError::new(
                "transcript_contains_no_speech",
                ErrorCategory::Transcription,
                "No speech was detected in this media.",
                "Every validated segment response was empty.",
            ));
        }
        for transcript in &transcripts {
            for pair in transcript.segments.windows(2) {
                if pair[1].start_seconds < pair[0].start_seconds {
                    return Err(AppError::new(
                        "transcript_timestamp_order_invalid",
                        ErrorCategory::Transcription,
                        "Transcript timestamps were not in chronological order.",
                        serde_json::to_string(pair).unwrap_or_default(),
                    ));
                }
            }
        }
        Ok(TaskExecutionResult {
            message: "Transcript segments validated".to_string(),
            artifacts: Vec::new(),
        })
    }

    pub(super) fn merge(
        &self,
        context: &TaskContext,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let document = merge_transcripts(
            &context.item.item.id,
            &context.item.item.title,
            &context.item.item.source,
            &context.plan.settings.model,
            self.load_segment_transcripts(context)?,
        )?;
        let path = canonical_path(&self.item_work_dir(context));
        atomic_write_json(&path, &document)?;
        let checksum = sha256_file(&path, control)?;
        let size = fs::metadata(&path).map_err(filesystem_error)?.len();
        Ok(TaskExecutionResult {
            message: "Transcript merged without boundary duplication".to_string(),
            artifacts: vec![artifact(
                context,
                ArtifactKind::CanonicalTranscript,
                &path,
                checksum,
                size,
                BTreeMap::new(),
            )],
        })
    }

    fn segment_manifest(&self, context: &TaskContext) -> Result<SegmentManifest, AppError> {
        let artifact = self.artifact_for(context, ArtifactKind::SegmentManifest)?;
        serde_json::from_str(&fs::read_to_string(artifact.path).map_err(filesystem_error)?).map_err(
            |error| {
                AppError::new(
                    "segment_manifest_invalid",
                    ErrorCategory::Media,
                    "The audio segment manifest is invalid.",
                    error.to_string(),
                )
            },
        )
    }

    fn load_segment_transcripts(
        &self,
        context: &TaskContext,
    ) -> Result<Vec<SegmentTranscript>, AppError> {
        let manifest = self.segment_manifest(context)?;
        let config_hash = transcription_config_hash(context);
        let directory = self.item_work_dir(context).join("segment-transcripts");
        manifest
            .segments
            .iter()
            .map(|segment| {
                let path = directory.join(format!("segment_{:04}.json", segment.index));
                read_cached_segment(&path, &config_hash, &segment.checksum).ok_or_else(|| {
                    AppError::new(
                        "segment_transcript_cache_invalid",
                        ErrorCategory::Transcription,
                        "A segment transcript is missing or does not match this run.",
                        path.display().to_string(),
                    )
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedSegmentTranscript {
    schema_version: u16,
    config_hash: String,
    segment_checksum: String,
    transcript: SegmentTranscript,
}

fn read_cached_segment(
    path: &Path,
    config_hash: &str,
    segment_checksum: &str,
) -> Option<SegmentTranscript> {
    let value: CachedSegmentTranscript =
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    (value.schema_version == 1
        && value.config_hash == config_hash
        && value.segment_checksum == segment_checksum)
        .then_some(value.transcript)
}

fn should_split_after_error(error: &AppError) -> bool {
    matches!(
        error.code.as_str(),
        "transcript_truncated"
            | "transcript_repetition_detected"
            | "transcript_schema_invalid"
            | "transcript_timestamp_out_of_range"
    )
}
