use lecturescribe_adapters::AppPaths;
use lecturescribe_core::{AppError, AppSettings, Theme, TranscriptFormat};
use lecturescribe_engine::Store;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn initial_settings(store: &Store, paths: &AppPaths) -> Result<AppSettings, AppError> {
    if let Some(settings) = store.load_settings()? {
        return Ok(paths.settings_with_defaults(settings));
    }

    let mut settings = AppSettings::default();
    for candidate in legacy_candidates(paths) {
        if let Some(legacy) = read_legacy(&candidate) {
            apply_legacy(&mut settings, legacy);
            break;
        }
    }
    Ok(paths.settings_with_defaults(settings))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacySettings {
    output_dir: String,
    download_dir: String,
    work_dir: String,
    model: String,
    theme: String,
    transcript_format: String,
    prompt_preset: String,
    ffmpeg_path: String,
    ffprobe_path: String,
    downloader_path: String,
    chunk_minutes: u32,
    request_delay_seconds: f64,
    cookies_from_browser: String,
    cookies_file: String,
    keep_downloaded_media: bool,
    force: bool,
}

fn read_legacy(path: &Path) -> Option<LegacySettings> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn apply_legacy(settings: &mut AppSettings, legacy: LegacySettings) {
    copy_nonempty(&mut settings.output_dir, legacy.output_dir);
    copy_nonempty(&mut settings.download_dir, legacy.download_dir);
    copy_nonempty(&mut settings.work_dir, legacy.work_dir);
    copy_nonempty(&mut settings.model, legacy.model);
    copy_nonempty(&mut settings.prompt_preset, legacy.prompt_preset);
    copy_nonempty(&mut settings.ffmpeg_path, legacy.ffmpeg_path);
    copy_nonempty(&mut settings.ffprobe_path, legacy.ffprobe_path);
    copy_nonempty(&mut settings.downloader_path, legacy.downloader_path);
    copy_nonempty(
        &mut settings.cookies_from_browser,
        legacy.cookies_from_browser,
    );
    copy_nonempty(&mut settings.cookies_file, legacy.cookies_file);
    settings.theme = if legacy.theme.eq_ignore_ascii_case("dark") {
        Theme::Dark
    } else {
        Theme::Light
    };
    settings.output_formats = match legacy.transcript_format.as_str() {
        "txt" => vec![TranscriptFormat::Text],
        "markdown" => vec![TranscriptFormat::Markdown],
        _ => vec![TranscriptFormat::Text, TranscriptFormat::Markdown],
    };
    if (5..=30).contains(&legacy.chunk_minutes) {
        settings.segment_minutes = legacy.chunk_minutes;
    }
    if legacy.request_delay_seconds.is_finite() && legacy.request_delay_seconds > 0.0 {
        settings.request_delay_ms = (legacy.request_delay_seconds * 1000.0).min(120_000.0) as u64;
    }
    settings.keep_downloaded_media = legacy.keep_downloaded_media;
    settings.force = legacy.force;
}

fn copy_nonempty(target: &mut String, value: String) {
    let value = value.trim();
    if !value.is_empty() {
        *target = value.to_string();
    }
}

fn legacy_candidates(paths: &AppPaths) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current.join(".lecturescribe-settings.json"));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(project_root) = manifest.parent().and_then(Path::parent) {
        candidates.push(project_root.join(".lecturescribe-settings.json"));
    }
    candidates.push(paths.data_dir.join(".lecturescribe-settings.json"));

    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .filter(|path| path.is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_theme_migrates_to_light() {
        let mut settings = AppSettings::default();
        apply_legacy(
            &mut settings,
            LegacySettings {
                theme: "system".to_string(),
                ..LegacySettings::default()
            },
        );
        assert_eq!(settings.theme, Theme::Light);
    }

    #[test]
    fn unsafe_tiny_legacy_chunks_use_new_default() {
        let mut settings = AppSettings::default();
        apply_legacy(
            &mut settings,
            LegacySettings {
                chunk_minutes: 2,
                ..LegacySettings::default()
            },
        );
        assert_eq!(settings.segment_minutes, 20);
    }
}
