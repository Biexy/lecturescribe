use crate::process::{run_output, CommandSpec};
use crate::tools::ToolResolver;
use lecturescribe_core::{
    extract_urls, inspect_source_values, AppError, AppSettings, ArtifactKind, ErrorCategory,
    InspectSourcesRequest, ItemState, PreviewSnapshot, ProviderKind, SourceInput, SourceKind,
};
use lecturescribe_engine::{JobControl, Store};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const MEDIA_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "mp4", "webm", "wav", "aac", "flac", "ogg", "opus", "mov", "mkv",
];
const SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "Transcripts",
    "downloads",
    "cache",
];

#[derive(Clone)]
pub struct SourceInspector {
    store: Arc<Store>,
    tools: ToolResolver,
}

impl SourceInspector {
    pub fn new(store: Arc<Store>, tools: ToolResolver) -> Self {
        Self { store, tools }
    }

    pub fn inspect(
        &self,
        request: InspectSourcesRequest,
        settings: &AppSettings,
    ) -> Result<PreviewSnapshot, AppError> {
        let source_count = request.sources.len();
        let resolved = self.tools.resolve(settings);
        let control = JobControl::default();
        let mut warnings = Vec::new();
        let mut flattened = Vec::<(SourceInput, String)>::new();

        for source in &request.sources {
            self.expand_source(
                source,
                request.confirm_large_playlists,
                request.playlist_limit.clamp(1, 200),
                settings,
                resolved.downloader.path.as_deref(),
                &control,
                &mut flattened,
                &mut warnings,
            )?;
        }

        let (mut items, duplicate_count) = inspect_source_values(&flattened);
        for item in &mut items {
            if item.duplicate_of.is_some() || item.error.is_some() {
                continue;
            }
            match item.provider {
                ProviderKind::Local => {
                    enrich_local(item, resolved.ffprobe.path.as_deref(), &control);
                }
                _ => {
                    let url = item.url.clone();
                    if let (Some(downloader), Some(url)) =
                        (resolved.downloader.path.as_deref(), url.as_deref())
                    {
                        if let Err(error) = enrich_link(item, downloader, url, settings, &control) {
                            item.status = ItemState::Blocked;
                            item.error = Some(error);
                            item.selected = false;
                        }
                    }
                }
            }
            self.attach_existing_artifacts(item)?;
        }

        let preview = PreviewSnapshot {
            id: Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now(),
            items,
            duplicate_count,
            source_count,
            warnings,
        };
        self.store.save_preview(&preview)?;
        Ok(preview)
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_source(
        &self,
        source: &SourceInput,
        confirm_large_playlists: bool,
        playlist_limit: usize,
        settings: &AppSettings,
        downloader: Option<&Path>,
        control: &JobControl,
        flattened: &mut Vec<(SourceInput, String)>,
        warnings: &mut Vec<String>,
    ) -> Result<(), AppError> {
        match source.kind {
            SourceKind::PastedLink => {
                let urls = extract_urls(&source.value);
                for url in urls {
                    if is_playlist_only_url(&url) {
                        self.expand_playlist(
                            source,
                            &url,
                            confirm_large_playlists,
                            playlist_limit,
                            settings,
                            downloader,
                            control,
                            flattened,
                            warnings,
                        )?;
                    } else {
                        flattened.push((source.clone(), url));
                    }
                }
                for line in source
                    .value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    if !line.starts_with("http://")
                        && !line.starts_with("https://")
                        && looks_like_media_path(line)
                    {
                        flattened.push((source.clone(), line.trim_matches('"').to_string()));
                    }
                }
            }
            SourceKind::TextFile | SourceKind::AutomaticFile => {
                let path = PathBuf::from(source.value.trim());
                let text = fs::read_to_string(&path).map_err(|error| {
                    AppError::new(
                        "link_file_unreadable",
                        ErrorCategory::Input,
                        "LectureScribe could not read this link file.",
                        format!("{}: {error}", path.display()),
                    )
                    .with_action(
                        "choose_link_file",
                        "Choose another file",
                        "choose_link_file",
                    )
                })?;
                let urls = extract_urls(&text);
                if urls.is_empty() {
                    warnings.push(format!(
                        "No supported links were found in {}.",
                        path.file_name()
                            .and_then(|value| value.to_str())
                            .unwrap_or("the link file")
                    ));
                }
                for url in urls {
                    if is_playlist_only_url(&url) {
                        self.expand_playlist(
                            source,
                            &url,
                            confirm_large_playlists,
                            playlist_limit,
                            settings,
                            downloader,
                            control,
                            flattened,
                            warnings,
                        )?;
                    } else {
                        flattened.push((source.clone(), url));
                    }
                }
            }
            SourceKind::LocalMedia => {
                let path = PathBuf::from(source.value.trim());
                if path.is_dir() {
                    collect_media(&path, source, flattened)?;
                } else {
                    flattened.push((source.clone(), path.to_string_lossy().to_string()));
                }
            }
            SourceKind::Directory => {
                collect_media(Path::new(source.value.trim()), source, flattened)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_playlist(
        &self,
        source: &SourceInput,
        url: &str,
        confirmed: bool,
        limit: usize,
        settings: &AppSettings,
        downloader: Option<&Path>,
        control: &JobControl,
        flattened: &mut Vec<(SourceInput, String)>,
        warnings: &mut Vec<String>,
    ) -> Result<(), AppError> {
        let Some(downloader) = downloader else {
            warnings.push("Install the Downloader before expanding this playlist.".to_string());
            return Ok(());
        };
        let mut args = vec![
            "--flat-playlist".to_string(),
            "--dump-single-json".to_string(),
            "--no-warnings".to_string(),
        ];
        append_cookie_args(&mut args, settings);
        args.push(url.to_string());
        let mut spec = CommandSpec::new(downloader);
        spec.args = args;
        spec.timeout = Duration::from_secs(90);
        let result = run_output(&spec, control)?;
        if !result.status.success() {
            return Err(download_metadata_error(&result.stderr));
        }
        let metadata: Value = serde_json::from_str(result.stdout.trim()).map_err(|error| {
            AppError::new(
                "playlist_metadata_invalid",
                ErrorCategory::Download,
                "The Downloader returned invalid playlist information.",
                error.to_string(),
            )
        })?;
        let entries = metadata["entries"].as_array().cloned().unwrap_or_default();
        if entries.len() > 50 && !confirmed {
            warnings.push(format!(
                "playlist_confirmation_required:{}:{}:{}",
                source.id,
                entries.len(),
                metadata["title"].as_str().unwrap_or("Playlist")
            ));
            return Ok(());
        }
        if entries.len() > limit {
            warnings.push(format!(
                "The playlist contains {} items; the first {limit} were added.",
                entries.len()
            ));
        }
        for entry in entries.into_iter().take(limit) {
            let item_url = entry["webpage_url"]
                .as_str()
                .map(ToString::to_string)
                .or_else(|| {
                    entry["id"]
                        .as_str()
                        .map(|id| format!("https://www.youtube.com/watch?v={id}"))
                });
            if let Some(item_url) = item_url {
                flattened.push((source.clone(), item_url));
            }
        }
        Ok(())
    }

    fn attach_existing_artifacts(
        &self,
        item: &mut lecturescribe_core::PreviewItem,
    ) -> Result<(), AppError> {
        if let Some(artifact) = self
            .store
            .latest_artifact(&item.id, ArtifactKind::DownloadedMedia)?
            .filter(|artifact| artifact_valid_at_preview(artifact))
        {
            item.existing_media_path = Some(artifact.path);
        }
        for kind in [
            ArtifactKind::TextTranscript,
            ArtifactKind::MarkdownTranscript,
        ] {
            if let Some(artifact) = self
                .store
                .latest_artifact(&item.id, kind)?
                .filter(|artifact| artifact_valid_at_preview(artifact))
            {
                item.existing_transcript_path = Some(artifact.path);
                break;
            }
        }
        Ok(())
    }
}

fn collect_media(
    directory: &Path,
    source: &SourceInput,
    flattened: &mut Vec<(SourceInput, String)>,
) -> Result<(), AppError> {
    if !directory.exists() {
        return Err(AppError::new(
            "media_directory_missing",
            ErrorCategory::Input,
            "The selected media folder no longer exists.",
            directory.display().to_string(),
        ));
    }
    let entries = fs::read_dir(directory).map_err(|error| {
        AppError::new(
            "media_directory_unreadable",
            ErrorCategory::Input,
            "LectureScribe could not read the selected media folder.",
            error.to_string(),
        )
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !SKIP_DIRECTORIES
                .iter()
                .any(|skip| name.eq_ignore_ascii_case(skip))
            {
                collect_media(&path, source, flattened)?;
            }
        } else if is_media_file(&path) {
            flattened.push((source.clone(), path.to_string_lossy().to_string()));
        }
    }
    Ok(())
}

fn enrich_local(
    item: &mut lecturescribe_core::PreviewItem,
    ffprobe: Option<&Path>,
    control: &JobControl,
) {
    let Some(path) = item.media_path.as_ref().map(PathBuf::from) else {
        return;
    };
    if !path.exists() || !path.is_file() {
        item.status = ItemState::Blocked;
        item.selected = false;
        item.error = Some(
            AppError::new(
                "local_media_missing",
                ErrorCategory::Input,
                "This local media file no longer exists.",
                path.display().to_string(),
            )
            .with_action("choose_media", "Choose media", "choose_media"),
        );
        return;
    }
    if !is_media_file(&path) {
        item.status = ItemState::Blocked;
        item.selected = false;
        item.error = Some(AppError::new(
            "local_media_unsupported",
            ErrorCategory::Input,
            "This local file type is not supported.",
            path.display().to_string(),
        ));
        return;
    }
    item.title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Local media")
        .to_string();
    item.expected_media_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string);
    if let Some(ffprobe) = ffprobe {
        item.duration_seconds = probe_duration(ffprobe, &path, control).ok();
    }
}

fn enrich_link(
    item: &mut lecturescribe_core::PreviewItem,
    downloader: &Path,
    url: &str,
    settings: &AppSettings,
    control: &JobControl,
) -> Result<(), AppError> {
    let mut args = vec![
        "--dump-single-json".to_string(),
        "--no-download".to_string(),
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
    ];
    append_cookie_args(&mut args, settings);
    args.push(url.to_string());
    let mut spec = CommandSpec::new(downloader);
    spec.args = args;
    spec.timeout = Duration::from_secs(60);
    let result = run_output(&spec, control)?;
    if !result.status.success() {
        return Err(download_metadata_error(&result.stderr));
    }
    let metadata: Value = serde_json::from_str(result.stdout.trim()).map_err(|error| {
        AppError::new(
            "source_metadata_invalid",
            ErrorCategory::Download,
            "The Downloader returned invalid source information.",
            error.to_string(),
        )
    })?;
    if let Some(title) = metadata["title"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
    {
        item.title = title.trim().to_string();
    }
    item.duration_seconds = metadata["duration"].as_f64();
    item.thumbnail_url = metadata["thumbnail"].as_str().map(ToString::to_string);
    item.expected_media_name = metadata["filename"]
        .as_str()
        .or_else(|| metadata["_filename"].as_str())
        .map(ToString::to_string);
    Ok(())
}

fn probe_duration(ffprobe: &Path, media: &Path, control: &JobControl) -> Result<f64, AppError> {
    let mut spec = CommandSpec::new(ffprobe);
    spec.args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-show_entries".to_string(),
        "format=duration".to_string(),
        "-of".to_string(),
        "default=noprint_wrappers=1:nokey=1".to_string(),
        media.to_string_lossy().to_string(),
    ];
    spec.timeout = Duration::from_secs(30);
    let result = run_output(&spec, control)?;
    if !result.status.success() {
        return Err(AppError::new(
            "media_probe_failed",
            ErrorCategory::Media,
            "LectureScribe could not inspect this media file.",
            result.stderr,
        ));
    }
    result.stdout.trim().parse::<f64>().map_err(|error| {
        AppError::new(
            "media_duration_invalid",
            ErrorCategory::Media,
            "The media duration could not be read.",
            error.to_string(),
        )
    })
}

fn append_cookie_args(args: &mut Vec<String>, settings: &AppSettings) {
    if !settings.cookies_file.trim().is_empty() {
        args.extend([
            "--cookies".to_string(),
            settings.cookies_file.trim().to_string(),
        ]);
    } else if !settings.cookies_from_browser.trim().is_empty() {
        args.extend([
            "--cookies-from-browser".to_string(),
            settings.cookies_from_browser.trim().to_string(),
        ]);
    }
}

fn download_metadata_error(detail: &str) -> AppError {
    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("private")
        || lowered.contains("sign in")
        || lowered.contains("login")
        || lowered.contains("permission")
    {
        AppError::new(
            "private_source_access_denied",
            ErrorCategory::Authentication,
            "This source is private or requires sign-in.",
            detail,
        )
        .with_action(
            "configure_cookies",
            "Add browser cookies",
            "configure_cookies",
        )
    } else {
        AppError::new(
            "source_metadata_failed",
            ErrorCategory::Download,
            "LectureScribe could not inspect this link.",
            detail,
        )
        .retryable("The source remains in the queue for retry.")
    }
}

fn artifact_valid_at_preview(artifact: &lecturescribe_core::ArtifactRecord) -> bool {
    let path = Path::new(&artifact.path);
    path.is_file()
        && fs::metadata(path)
            .map(|metadata| metadata.len() == artifact.size_bytes)
            .unwrap_or(false)
}

fn is_playlist_only_url(url: &str) -> bool {
    url.contains("list=") && !url.contains("watch?v=")
}

fn looks_like_media_path(value: &str) -> bool {
    is_media_file(Path::new(value.trim_matches('"')))
}

fn is_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            MEDIA_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_extensions_are_case_insensitive() {
        assert!(is_media_file(Path::new("Lecture.MP4")));
        assert!(is_media_file(Path::new("audio.opus")));
        assert!(!is_media_file(Path::new("notes.txt")));
    }
}
