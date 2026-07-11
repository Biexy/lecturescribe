use crate::media::sha256_file;
use crate::paths::safe_component;
use crate::process::{run_streaming, CommandSpec, StreamKind};
use lecturescribe_core::{AppError, ErrorCategory, ProgressKind, ProgressMetric};
use lecturescribe_engine::{JobControl, ProgressReporter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub item_id: String,
    pub title: String,
    pub output_dir: PathBuf,
    pub download_only: bool,
    pub cookies_file: String,
    pub cookies_from_browser: String,
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub path: PathBuf,
    pub checksum: String,
    pub size_bytes: u64,
}

pub fn download(
    downloader: &Path,
    request: &DownloadRequest,
    control: &JobControl,
    reporter: &dyn ProgressReporter,
) -> Result<DownloadResult, AppError> {
    fs::create_dir_all(&request.output_dir).map_err(filesystem_error)?;
    let template = request.output_dir.join(format!(
        "%(title).160B [{}].%(ext)s",
        safe_component(&request.item_id)
    ));
    let mut args = vec![
        "--newline".to_string(),
        "--no-colors".to_string(),
        "--continue".to_string(),
        "--part".to_string(),
        "--no-playlist".to_string(),
        "--progress".to_string(),
        "--progress-template".to_string(),
        "download:LS_PROGRESS:%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s".to_string(),
        "--print".to_string(),
        "after_move:LS_PATH:%(filepath)s".to_string(),
        "--output".to_string(),
        template.to_string_lossy().to_string(),
        "--format".to_string(),
        if request.download_only {
            "best[ext=mp4]/best".to_string()
        } else {
            "bestaudio/best".to_string()
        },
    ];
    if !request.cookies_file.trim().is_empty() {
        args.extend([
            "--cookies".to_string(),
            request.cookies_file.trim().to_string(),
        ]);
    } else if !request.cookies_from_browser.trim().is_empty() {
        args.extend([
            "--cookies-from-browser".to_string(),
            request.cookies_from_browser.trim().to_string(),
        ]);
    }
    args.push(request.url.clone());

    let mut spec = CommandSpec::new(downloader);
    spec.args = args;
    spec.current_dir = Some(request.output_dir.clone());
    spec.timeout = Duration::from_secs(6 * 60 * 60);
    let mut final_path = None;
    let result = run_streaming(&spec, control, &mut |kind, line| {
        if kind != StreamKind::Stdout {
            return;
        }
        if let Some(value) = line.strip_prefix("LS_PATH:") {
            final_path = Some(PathBuf::from(value.trim()));
        } else if let Some(value) = line.strip_prefix("LS_PROGRESS:") {
            if let Some(progress) = parse_progress(value) {
                let downloaded = progress.current;
                let total = progress.total.unwrap_or_default();
                let message = if total > 0.0 {
                    format!(
                        "Downloading {} of {}",
                        format_bytes(downloaded as u64),
                        format_bytes(total as u64)
                    )
                } else {
                    format!("Downloaded {}", format_bytes(downloaded as u64))
                };
                reporter.report(progress, &message);
            }
        }
    })?;
    if !result.status.success() {
        return Err(download_error(&result.stderr));
    }
    let path = final_path.ok_or_else(|| {
        AppError::new(
            "download_output_missing",
            ErrorCategory::Download,
            "The Downloader finished without reporting a media file.",
            result.stdout,
        )
        .retryable("Any valid partial download was preserved.")
    })?;
    if !path.is_file() {
        return Err(AppError::new(
            "download_file_missing",
            ErrorCategory::Download,
            "The downloaded media file could not be found.",
            path.display().to_string(),
        )
        .retryable("Any valid partial download was preserved."));
    }
    let size_bytes = fs::metadata(&path).map_err(filesystem_error)?.len();
    let checksum = sha256_file(&path, control)?;
    reporter.report(
        ProgressMetric {
            kind: ProgressKind::Bytes,
            current: size_bytes as f64,
            total: Some(size_bytes as f64),
            unit: "bytes".to_string(),
            rate: None,
            eta_seconds: Some(0),
        },
        "Download complete",
    );
    Ok(DownloadResult {
        path,
        checksum,
        size_bytes,
    })
}

fn parse_progress(value: &str) -> Option<ProgressMetric> {
    let fields = value.split('|').collect::<Vec<_>>();
    let downloaded = parse_number(fields.first().copied())?;
    let total = parse_number(fields.get(1).copied())
        .or_else(|| parse_number(fields.get(2).copied()))
        .filter(|value| *value > 0.0);
    let rate = parse_number(fields.get(3).copied()).filter(|value| *value > 0.0);
    let eta_seconds = parse_number(fields.get(4).copied()).map(|value| value.max(0.0) as u64);
    Some(ProgressMetric {
        kind: ProgressKind::Bytes,
        current: downloaded,
        total,
        unit: "bytes".to_string(),
        rate,
        eta_seconds,
    })
}

fn parse_number(value: Option<&str>) -> Option<f64> {
    let value = value?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("na") || value.eq_ignore_ascii_case("none") {
        None
    } else {
        value.parse::<f64>().ok()
    }
}

fn download_error(detail: &str) -> AppError {
    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("private")
        || lowered.contains("sign in")
        || lowered.contains("login")
        || lowered.contains("permission")
        || lowered.contains("cookies")
    {
        AppError::new(
            "private_download_denied",
            ErrorCategory::Authentication,
            "This source is private or requires sign-in.",
            detail,
        )
        .with_action(
            "configure_cookies",
            "Add browser cookies",
            "configure_cookies",
        )
    } else if lowered.contains("unsupported url") {
        AppError::new(
            "download_url_unsupported",
            ErrorCategory::Download,
            "The Downloader does not support this link.",
            detail,
        )
    } else {
        AppError::new(
            "download_failed",
            ErrorCategory::Download,
            "The media download failed.",
            detail,
        )
        .retryable("Valid partial download data was preserved for retry.")
    }
}

fn format_bytes(value: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
    let mut number = value as f64;
    let mut unit = 0usize;
    while number >= 1024.0 && unit + 1 < UNITS.len() {
        number /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} {}", UNITS[unit])
    } else {
        format!("{number:.1} {}", UNITS[unit])
    }
}

fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "download_filesystem_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not write the downloaded media.",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_prefers_exact_total_then_estimate() {
        let progress = parse_progress("1024|4096|8192|512|6").unwrap();
        assert_eq!(progress.total, Some(4096.0));
        assert_eq!(progress.rate, Some(512.0));
        assert_eq!(progress.eta_seconds, Some(6));

        let estimated = parse_progress("1024|NA|8192|NA|NA").unwrap();
        assert_eq!(estimated.total, Some(8192.0));
    }
}
