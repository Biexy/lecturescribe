use lecturescribe_core::{AppEvent, AppSettings, EnvironmentSnapshot, HistoryEntry};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOG_FILE_LIMIT: u64 = 5 * 1024 * 1024;
const LOG_FILE_COUNT: usize = 10;

pub struct TraceLogger {
    directory: PathBuf,
    lock: Mutex<()>,
}

impl TraceLogger {
    pub fn new(directory: PathBuf) -> Self {
        let _ = fs::create_dir_all(&directory);
        Self {
            directory,
            lock: Mutex::new(()),
        }
    }

    pub fn event(&self, event: &AppEvent) {
        let _guard = self.lock.lock().ok();
        let _ = self.rotate_if_needed();
        let path = self.directory.join("lecturescribe.jsonl");
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            if let Ok(mut value) = serde_json::to_value(event) {
                sanitize_value(&mut value);
                let _ = writeln!(file, "{}", value);
            }
        }
    }

    pub fn recent_lines(&self, limit: usize) -> Vec<String> {
        let path = self.directory.join("lecturescribe.jsonl");
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .rev()
            .take(limit.min(500))
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn rotate_if_needed(&self) -> std::io::Result<()> {
        let current = self.directory.join("lecturescribe.jsonl");
        if fs::metadata(&current)
            .map(|value| value.len())
            .unwrap_or_default()
            < LOG_FILE_LIMIT
        {
            return Ok(());
        }
        let oldest = self
            .directory
            .join(format!("lecturescribe.{}.jsonl", LOG_FILE_COUNT - 1));
        let _ = fs::remove_file(oldest);
        for index in (1..LOG_FILE_COUNT - 1).rev() {
            let source = self.directory.join(format!("lecturescribe.{index}.jsonl"));
            let target = self
                .directory
                .join(format!("lecturescribe.{}.jsonl", index + 1));
            if source.exists() {
                fs::rename(source, target)?;
            }
        }
        fs::rename(current, self.directory.join("lecturescribe.1.jsonl"))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub environment: EnvironmentSnapshot,
    pub settings: Value,
    pub recent_history: Vec<HistoryEntry>,
    pub recent_events: Vec<Value>,
    pub privacy_notice: String,
}

pub fn build_diagnostic_report(
    environment: EnvironmentSnapshot,
    settings: &AppSettings,
    history: Vec<HistoryEntry>,
    log_lines: Vec<String>,
) -> DiagnosticReport {
    let mut settings = serde_json::to_value(settings).unwrap_or(Value::Null);
    sanitize_settings(&mut settings);
    let recent_events = log_lines
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .map(|mut value| {
            sanitize_value(&mut value);
            value
        })
        .collect();
    DiagnosticReport {
        generated_at: chrono::Utc::now(),
        environment,
        settings,
        recent_history: history,
        recent_events,
        privacy_notice: "API keys, cookies, URLs, filenames, transcript content, and personal paths are removed. Review before sharing.".to_string(),
    }
}

pub fn write_diagnostic_report(path: &Path, report: &DiagnosticReport) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(report)?;
    fs::write(path, bytes)
}

pub fn sanitize_text(value: &str) -> String {
    let key =
        Regex::new(r"(?i)(api[_ -]?key|x-goog-api-key|authorization|cookie)(\s*[:=]\s*)[^\s,;]+")
            .expect("secret regex");
    let url = Regex::new(r#"https?://[^\s"']+"#).expect("URL regex");
    let windows_path = Regex::new(r#"(?i)\b[A-Z]:\\[^\r\n"']+"#).expect("path regex");
    let unc_path = Regex::new(r#"\\\\[^\s\\]+\\[^\r\n"']+"#).expect("UNC regex");
    let value = key.replace_all(value, "$1$2[REDACTED]");
    let value = url.replace_all(&value, "[PRIVATE_URL]");
    let value = windows_path.replace_all(&value, "[PRIVATE_PATH]");
    unc_path.replace_all(&value, "[PRIVATE_PATH]").to_string()
}

fn sanitize_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = sanitize_text(text),
        Value::Array(values) => values.iter_mut().for_each(sanitize_value),
        Value::Object(values) => {
            for (key, value) in values {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "api_key" | "cookies_file" | "cookies_from_browser" | "source" | "url"
                ) {
                    *value = Value::String("[REDACTED]".to_string());
                } else {
                    sanitize_value(value);
                }
            }
        }
        _ => {}
    }
}

fn sanitize_settings(value: &mut Value) {
    if let Value::Object(settings) = value {
        for key in [
            "output_dir",
            "download_dir",
            "work_dir",
            "ffmpeg_path",
            "ffprobe_path",
            "downloader_path",
            "cookies_file",
            "cookies_from_browser",
            "additional_prompt",
        ] {
            if settings.contains_key(key) {
                settings.insert(key.to_string(), Value::String("[REDACTED]".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_urls_paths_and_keys() {
        let value =
            sanitize_text("api_key=secret https://example.com/private C:\\Users\\Person\\file.mp4");
        assert!(!value.contains("secret"));
        assert!(!value.contains("example.com"));
        assert!(!value.contains("Person"));
    }
}
