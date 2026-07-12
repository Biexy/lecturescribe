use super::{artifact, filesystem_error, PipelineExecutor};
use crate::downloader::{download, DownloadRequest};
use crate::media::{normalize_audio, probe_media, segment_audio, sha256_file};
use lecturescribe_core::{AppError, ArtifactKind, ErrorCategory, RunMode};
use lecturescribe_engine::{JobControl, ProgressReporter, TaskContext, TaskExecutionResult};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

impl PipelineExecutor {
    pub(super) fn inspect(&self, context: &TaskContext) -> Result<TaskExecutionResult, AppError> {
        let item = &context.item.item;
        if let Some(path) = &item.media_path {
            if !Path::new(path).is_file() {
                return Err(AppError::new(
                    "local_media_missing",
                    ErrorCategory::Input,
                    "This local media file no longer exists.",
                    path,
                )
                .with_action("choose_media", "Choose media", "choose_media"));
            }
        } else if item.url.as_deref().is_none_or(str::is_empty) {
            return Err(AppError::new(
                "link_source_missing",
                ErrorCategory::Input,
                "This queue item has no usable link.",
                &item.canonical_source,
            ));
        }
        Ok(TaskExecutionResult {
            message: "Source is available".to_string(),
            artifacts: Vec::new(),
        })
    }

    pub(super) fn download(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let settings = &context.plan.settings;
        let tools = self.tools.resolve(settings);
        let downloader = tools.downloader.path.ok_or_else(|| {
            AppError::new(
                "downloader_missing",
                ErrorCategory::Setup,
                "Install the Downloader before processing links.",
                tools.downloader.status.detail,
            )
            .with_action(
                "install_downloader",
                "Install Downloader",
                "install_downloader",
            )
        })?;
        let output_dir = if context.plan.mode == RunMode::Download {
            PathBuf::from(&context.plan.batch_output_dir).join("Media")
        } else {
            self.item_work_dir(context).join("media")
        };
        let result = download(
            &downloader,
            &DownloadRequest {
                url: context.item.item.url.clone().ok_or_else(|| {
                    AppError::new(
                        "download_url_missing",
                        ErrorCategory::Input,
                        "This queue item has no download link.",
                        &context.item.item.canonical_source,
                    )
                })?,
                item_id: context.item.item.id.clone(),
                title: context.item.item.title.clone(),
                output_dir,
                download_only: context.plan.mode == RunMode::Download,
                cookies_file: settings.cookies_file.clone(),
                cookies_from_browser: settings.cookies_from_browser.clone(),
            },
            control,
            reporter,
        )?;
        Ok(TaskExecutionResult {
            message: "Download complete and checksummed".to_string(),
            artifacts: vec![artifact(
                context,
                ArtifactKind::DownloadedMedia,
                &result.path,
                result.checksum,
                result.size_bytes,
                BTreeMap::new(),
            )],
        })
    }

    pub(super) fn verify(
        &self,
        context: &TaskContext,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let settings = &context.plan.settings;
        let media = self.media_path(context)?;
        if context.plan.mode == RunMode::Download {
            let size = fs::metadata(&media).map_err(filesystem_error)?.len();
            if size == 0 {
                return Err(AppError::new(
                    "download_file_empty",
                    ErrorCategory::Download,
                    "The downloaded media file is empty.",
                    media.display().to_string(),
                )
                .retryable("The source can be downloaded again."));
            }
            let checksum = sha256_file(&media, control)?;
            return Ok(TaskExecutionResult {
                message: "Verified downloaded file".to_string(),
                artifacts: vec![artifact(
                    context,
                    ArtifactKind::VerifiedMedia,
                    &media,
                    checksum,
                    size,
                    BTreeMap::new(),
                )],
            });
        }
        let tools = self.tools.resolve(settings);
        let ffprobe = tools.ffprobe.path.ok_or_else(|| {
            AppError::new(
                "ffprobe_missing",
                ErrorCategory::Setup,
                "FFprobe is required to verify media.",
                tools.ffprobe.status.detail,
            )
            .with_action("open_setup_ffmpeg", "Fix FFmpeg", "open_setup_ffmpeg")
        })?;
        let probe = probe_media(&ffprobe, &media, control)?;
        let checksum = sha256_file(&media, control)?;
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "duration_seconds".to_string(),
            probe.duration_seconds.to_string(),
        );
        metadata.insert("format_name".to_string(), probe.format_name);
        Ok(TaskExecutionResult {
            message: format!(
                "Verified {:.1} minutes of media",
                probe.duration_seconds / 60.0
            ),
            artifacts: vec![artifact(
                context,
                ArtifactKind::VerifiedMedia,
                &media,
                checksum,
                probe.size_bytes,
                metadata,
            )],
        })
    }

    pub(super) fn prepare(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let settings = &context.plan.settings;
        let tools = self.tools.resolve(settings);
        let ffmpeg = tools.ffmpeg.path.ok_or_else(|| {
            AppError::new(
                "ffmpeg_missing",
                ErrorCategory::Setup,
                "FFmpeg is required to prepare audio.",
                tools.ffmpeg.status.detail,
            )
            .with_action("open_setup_ffmpeg", "Fix FFmpeg", "open_setup_ffmpeg")
        })?;
        let media = self.media_path(context)?;
        let duration = self
            .artifact_for(context, ArtifactKind::VerifiedMedia)?
            .metadata
            .get("duration_seconds")
            .and_then(|value| value.parse::<f64>().ok())
            .or(context.item.item.duration_seconds)
            .unwrap_or(1.0);
        let target = self
            .item_work_dir(context)
            .join("audio")
            .join("normalized.mp3");
        normalize_audio(&ffmpeg, &media, &target, duration, control, reporter)?;
        let checksum = sha256_file(&target, control)?;
        let size = fs::metadata(&target).map_err(filesystem_error)?.len();
        Ok(TaskExecutionResult {
            message: "Audio normalized to 16 kHz mono".to_string(),
            artifacts: vec![artifact(
                context,
                ArtifactKind::NormalizedAudio,
                &target,
                checksum,
                size,
                BTreeMap::new(),
            )],
        })
    }

    pub(super) fn segment(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError> {
        let settings = &context.plan.settings;
        let tools = self.tools.resolve(settings);
        let ffmpeg = tools.ffmpeg.path.ok_or_else(|| {
            AppError::new(
                "ffmpeg_missing",
                ErrorCategory::Setup,
                "FFmpeg is required to create audio segments.",
                tools.ffmpeg.status.detail,
            )
        })?;
        let normalized = PathBuf::from(
            self.artifact_for(context, ArtifactKind::NormalizedAudio)?
                .path,
        );
        let segment_dir = self.item_work_dir(context).join("segments");
        let manifest = segment_audio(
            &ffmpeg,
            &normalized,
            &segment_dir,
            settings.segment_minutes * 60,
            settings.overlap_seconds,
            control,
            reporter,
        )?;
        let path = segment_dir.join("segments.json");
        let checksum = sha256_file(&path, control)?;
        let size = fs::metadata(&path).map_err(filesystem_error)?.len();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "segment_count".to_string(),
            manifest.segments.len().to_string(),
        );
        Ok(TaskExecutionResult {
            message: format!("{} verified audio segments ready", manifest.segments.len()),
            artifacts: vec![artifact(
                context,
                ArtifactKind::SegmentManifest,
                &path,
                checksum,
                size,
                metadata,
            )],
        })
    }
}
