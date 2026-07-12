use crate::process::{run_output, run_streaming, CommandSpec, StreamKind};
use lecturescribe_core::{AppError, ErrorCategory, ProgressKind, ProgressMetric};
use lecturescribe_engine::{JobControl, ProgressReporter};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbe {
    pub duration_seconds: f64,
    pub has_audio: bool,
    pub format_name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentDescriptor {
    pub index: usize,
    pub path: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub checksum: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentManifest {
    pub schema_version: u16,
    pub media_checksum: String,
    pub target_seconds: u32,
    pub overlap_seconds: u32,
    pub segments: Vec<SegmentDescriptor>,
}

pub fn probe_media(
    ffprobe: &Path,
    media: &Path,
    control: &JobControl,
) -> Result<MediaProbe, AppError> {
    if !media.is_file() {
        return Err(AppError::new(
            "media_missing",
            ErrorCategory::Media,
            "The media file could not be found.",
            media.display().to_string(),
        ));
    }
    let mut spec = CommandSpec::new(ffprobe);
    spec.args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-show_entries".to_string(),
        "format=duration,format_name,size:stream=codec_type".to_string(),
        "-of".to_string(),
        "json".to_string(),
        media.to_string_lossy().to_string(),
    ];
    spec.timeout = Duration::from_secs(45);
    let result = run_output(&spec, control)?;
    if !result.status.success() {
        return Err(AppError::new(
            "media_probe_failed",
            ErrorCategory::Media,
            "This media file is unreadable or unsupported.",
            result.stderr,
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&result.stdout).map_err(|error| {
        AppError::new(
            "media_probe_invalid",
            ErrorCategory::Media,
            "FFprobe returned invalid media information.",
            error.to_string(),
        )
    })?;
    let duration_seconds = value["format"]["duration"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| value["format"]["duration"].as_f64())
        .unwrap_or_default();
    let has_audio = value["streams"]
        .as_array()
        .is_some_and(|streams| streams.iter().any(|stream| stream["codec_type"] == "audio"));
    if !has_audio {
        return Err(AppError::new(
            "media_has_no_audio",
            ErrorCategory::Media,
            "This media file has no audio track to transcribe.",
            media.display().to_string(),
        ));
    }
    if duration_seconds <= 0.0 {
        return Err(AppError::new(
            "media_duration_invalid",
            ErrorCategory::Media,
            "The media duration could not be determined.",
            result.stdout,
        ));
    }
    Ok(MediaProbe {
        duration_seconds,
        has_audio,
        format_name: value["format"]["format_name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        size_bytes: fs::metadata(media)
            .map(|value| value.len())
            .unwrap_or_default(),
    })
}

pub fn normalize_audio(
    ffmpeg: &Path,
    source: &Path,
    target: &Path,
    duration_seconds: f64,
    control: &JobControl,
    reporter: &dyn ProgressReporter,
) -> Result<(), AppError> {
    if target.is_file() && fs::metadata(target).is_ok_and(|value| value.len() > 0) {
        reporter.report(
            ProgressMetric::fraction(duration_seconds, duration_seconds, "seconds"),
            "Reusing prepared audio",
        );
        return Ok(());
    }
    ensure_parent(target)?;
    let temporary = temporary_path(target);
    let mut spec = CommandSpec::new(ffmpeg);
    spec.args = vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-i".to_string(),
        source.to_string_lossy().to_string(),
        "-map".to_string(),
        "0:a:0".to_string(),
        "-vn".to_string(),
        "-ac".to_string(),
        "1".to_string(),
        "-ar".to_string(),
        "16000".to_string(),
        "-c:a".to_string(),
        "libmp3lame".to_string(),
        "-b:a".to_string(),
        "64k".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
        "-nostats".to_string(),
        temporary.to_string_lossy().to_string(),
    ];
    let mut current = 0.0;
    let result = run_streaming(&spec, control, &mut |kind, line| {
        if kind == StreamKind::Stdout {
            if let Some(value) = line.strip_prefix("out_time_us=") {
                current = value.parse::<f64>().unwrap_or_default() / 1_000_000.0;
                reporter.report(
                    ProgressMetric {
                        kind: ProgressKind::Duration,
                        current: current.min(duration_seconds),
                        total: Some(duration_seconds),
                        unit: "seconds".to_string(),
                        rate: None,
                        eta_seconds: None,
                    },
                    "Preparing audio",
                );
            }
        }
    })?;
    if !result.status.success() {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::new(
            "audio_prepare_failed",
            ErrorCategory::Media,
            "FFmpeg could not prepare this media's audio.",
            result.stderr,
        )
        .retryable("The original media and verified downloads were preserved."));
    }
    atomic_replace(&temporary, target)?;
    reporter.report(
        ProgressMetric::fraction(duration_seconds, duration_seconds, "seconds"),
        "Audio prepared",
    );
    Ok(())
}

pub fn segment_audio(
    ffmpeg: &Path,
    normalized_audio: &Path,
    segment_dir: &Path,
    target_seconds: u32,
    overlap_seconds: u32,
    control: &JobControl,
    reporter: &dyn ProgressReporter,
) -> Result<SegmentManifest, AppError> {
    fs::create_dir_all(segment_dir).map_err(filesystem_error)?;
    let manifest_path = segment_dir.join("segments.json");
    let media_checksum = sha256_file(normalized_audio, control)?;
    if let Ok(text) = fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = serde_json::from_str::<SegmentManifest>(&text) {
            if manifest.media_checksum == media_checksum
                && manifest.target_seconds == target_seconds
                && manifest.overlap_seconds == overlap_seconds
                && manifest.segments.iter().all(segment_valid)
            {
                reporter.report(
                    ProgressMetric::fraction(
                        manifest.segments.len() as f64,
                        manifest.segments.len() as f64,
                        "segments",
                    ),
                    "Reusing verified audio segments",
                );
                return Ok(manifest);
            }
        }
    }

    let probe = probe_with_ffmpeg(ffmpeg, normalized_audio, control)?;
    let silence_points = detect_silence(ffmpeg, normalized_audio, control)?;
    let boundaries = choose_boundaries(probe, target_seconds as f64, &silence_points);
    let ranges = ranges_with_overlap(probe, &boundaries, overlap_seconds as f64);
    let mut segments = Vec::new();
    for (offset, (start, end)) in ranges.iter().copied().enumerate() {
        control.checkpoint()?;
        let index = offset + 1;
        let target = segment_dir.join(format!("segment_{index:04}.mp3"));
        let temporary = temporary_path(&target);
        let mut spec = CommandSpec::new(ffmpeg);
        spec.args = vec![
            "-y".to_string(),
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "error".to_string(),
            "-ss".to_string(),
            format!("{start:.3}"),
            "-i".to_string(),
            normalized_audio.to_string_lossy().to_string(),
            "-t".to_string(),
            format!("{:.3}", end - start),
            "-ac".to_string(),
            "1".to_string(),
            "-ar".to_string(),
            "16000".to_string(),
            "-c:a".to_string(),
            "libmp3lame".to_string(),
            "-b:a".to_string(),
            "64k".to_string(),
            temporary.to_string_lossy().to_string(),
        ];
        let result = run_output(&spec, control)?;
        if !result.status.success() {
            let _ = fs::remove_file(&temporary);
            return Err(AppError::new(
                "audio_segment_failed",
                ErrorCategory::Media,
                "FFmpeg could not create an audio segment.",
                result.stderr,
            )
            .retryable("Previously verified segments were preserved."));
        }
        atomic_replace(&temporary, &target)?;
        let size_bytes = fs::metadata(&target).map_err(filesystem_error)?.len();
        let checksum = sha256_file(&target, control)?;
        segments.push(SegmentDescriptor {
            index,
            path: target.to_string_lossy().to_string(),
            start_seconds: start,
            end_seconds: end,
            checksum,
            size_bytes,
        });
        reporter.report(
            ProgressMetric {
                kind: ProgressKind::Segments,
                current: index as f64,
                total: Some(ranges.len() as f64),
                unit: "segments".to_string(),
                rate: None,
                eta_seconds: None,
            },
            &format!("Created segment {index} of {}", ranges.len()),
        );
    }
    let manifest = SegmentManifest {
        schema_version: 1,
        media_checksum,
        target_seconds,
        overlap_seconds,
        segments,
    };
    atomic_write_json(&manifest_path, &manifest)?;
    Ok(manifest)
}

pub fn sha256_file(path: &Path, control: &JobControl) -> Result<String, AppError> {
    let mut file = fs::File::open(path).map_err(filesystem_error)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        control.checkpoint()?;
        let read = file.read(&mut buffer).map_err(filesystem_error)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        AppError::new(
            "json_serialize_failed",
            ErrorCategory::Filesystem,
            "LectureScribe could not prepare a local data file.",
            error.to_string(),
        )
    })?;
    atomic_write(path, &bytes)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    ensure_parent(path)?;
    let temporary = temporary_path(path);
    let mut file = fs::File::create(&temporary).map_err(filesystem_error)?;
    file.write_all(bytes).map_err(filesystem_error)?;
    file.flush().map_err(filesystem_error)?;
    file.sync_all().map_err(filesystem_error)?;
    atomic_replace(&temporary, path)
}

fn detect_silence(ffmpeg: &Path, media: &Path, control: &JobControl) -> Result<Vec<f64>, AppError> {
    let mut spec = CommandSpec::new(ffmpeg);
    spec.args = vec![
        "-hide_banner".to_string(),
        "-nostats".to_string(),
        "-i".to_string(),
        media.to_string_lossy().to_string(),
        "-af".to_string(),
        "silencedetect=noise=-35dB:d=0.6".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ];
    let result = run_output(&spec, control)?;
    let regex = Regex::new(r"silence_end:\s*([0-9.]+)").expect("silence regex");
    Ok(regex
        .captures_iter(&result.stderr)
        .filter_map(|capture| capture[1].parse::<f64>().ok())
        .collect())
}

fn probe_with_ffmpeg(ffmpeg: &Path, media: &Path, control: &JobControl) -> Result<f64, AppError> {
    let mut spec = CommandSpec::new(ffmpeg);
    spec.args = vec![
        "-hide_banner".to_string(),
        "-i".to_string(),
        media.to_string_lossy().to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ];
    let result = run_output(&spec, control)?;
    let regex = Regex::new(r"Duration:\s*(\d+):(\d+):([0-9.]+)").expect("duration regex");
    let capture = regex.captures(&result.stderr).ok_or_else(|| {
        AppError::new(
            "prepared_audio_duration_missing",
            ErrorCategory::Media,
            "The prepared audio duration could not be read.",
            result.stderr.clone(),
        )
    })?;
    let hours = capture[1].parse::<f64>().unwrap_or_default();
    let minutes = capture[2].parse::<f64>().unwrap_or_default();
    let seconds = capture[3].parse::<f64>().unwrap_or_default();
    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn choose_boundaries(duration: f64, target: f64, silence_points: &[f64]) -> Vec<f64> {
    let mut boundaries = Vec::new();
    let mut desired = target;
    let minimum_tail = (target * 0.25).max(60.0);
    while desired < duration - minimum_tail {
        let chosen = silence_points
            .iter()
            .copied()
            .filter(|point| (*point - desired).abs() <= 30.0)
            .min_by(|left, right| {
                (left - desired)
                    .abs()
                    .partial_cmp(&(right - desired).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(desired);
        if duration - chosen >= minimum_tail
            && boundaries
                .last()
                .is_none_or(|previous| chosen - previous >= 60.0)
        {
            boundaries.push(chosen);
        }
        desired += target;
    }
    boundaries
}

fn ranges_with_overlap(duration: f64, boundaries: &[f64], overlap: f64) -> Vec<(f64, f64)> {
    let mut points = vec![0.0];
    points.extend(boundaries.iter().copied());
    points.push(duration);
    points
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let start = if index == 0 {
                pair[0]
            } else {
                (pair[0] - overlap).max(0.0)
            };
            let end = if index + 1 == points.len() - 1 {
                pair[1]
            } else {
                (pair[1] + overlap).min(duration)
            };
            (start, end)
        })
        .collect()
}

fn segment_valid(segment: &SegmentDescriptor) -> bool {
    let path = Path::new(&segment.path);
    path.is_file() && fs::metadata(path).is_ok_and(|metadata| metadata.len() == segment.size_bytes)
}

fn ensure_parent(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(filesystem_error)?;
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.tmp{extension}", uuid::Uuid::new_v4()))
}

fn atomic_replace(source: &Path, target: &Path) -> Result<(), AppError> {
    if target.exists() {
        let backup = target.with_extension("previous");
        let _ = fs::remove_file(&backup);
        fs::rename(target, &backup).map_err(filesystem_error)?;
        match fs::rename(source, target) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(backup, target);
                Err(filesystem_error(error))
            }
        }
    } else {
        fs::rename(source, target).map_err(filesystem_error)
    }
}

fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "media_filesystem_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not write its media work files.",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestReporter;

    impl ProgressReporter for TestReporter {
        fn report(&self, _progress: ProgressMetric, _message: &str) {}
    }

    #[test]
    fn silence_points_adjust_boundaries() {
        let boundaries = choose_boundaries(3700.0, 1200.0, &[1188.0, 2410.0]);
        assert_eq!(boundaries, vec![1188.0, 2410.0]);
    }

    #[test]
    fn ranges_add_overlap_without_exceeding_media() {
        let ranges = ranges_with_overlap(100.0, &[50.0], 2.0);
        assert_eq!(ranges, vec![(0.0, 52.0), (48.0, 100.0)]);
    }

    #[test]
    fn temporary_media_paths_keep_the_target_extension() {
        let audio = temporary_path(Path::new("normalized.mp3"));
        let manifest = temporary_path(Path::new("segments.json"));

        assert_eq!(
            audio.extension().and_then(|value| value.to_str()),
            Some("mp3")
        );
        assert_eq!(
            manifest.extension().and_then(|value| value.to_str()),
            Some("json")
        );
        assert_ne!(audio, PathBuf::from("normalized.mp3"));
    }

    #[test]
    fn installed_ffmpeg_can_write_and_commit_normalized_audio() {
        let root = std::env::temp_dir().join(format!(
            "lecturescribe-normalize-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("input.wav");
        let target = root.join("normalized.mp3");
        let generated = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-ar",
                "16000",
                "-ac",
                "1",
            ])
            .arg(&source)
            .status();
        if !generated.is_ok_and(|status| status.success()) {
            let _ = fs::remove_dir_all(root);
            return;
        }

        normalize_audio(
            Path::new("ffmpeg"),
            &source,
            &target,
            1.0,
            &JobControl::default(),
            &TestReporter,
        )
        .unwrap();

        assert!(fs::metadata(&target).is_ok_and(|value| value.len() > 0));
        assert!(!root
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".tmp")));
        let _ = fs::remove_dir_all(root);
    }
}
