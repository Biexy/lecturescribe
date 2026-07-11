use super::DEFAULT_MODEL;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Light
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Transcribe,
    Download,
}

impl Default for RunMode {
    fn default() -> Self {
        Self::Transcribe
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptFormat {
    Text,
    Markdown,
    Srt,
    Vtt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub output_dir: String,
    pub download_dir: String,
    pub work_dir: String,
    pub model: String,
    pub theme: Theme,
    pub output_formats: Vec<TranscriptFormat>,
    pub language: String,
    pub prompt_preset: String,
    pub additional_prompt: String,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub downloader_path: String,
    pub cookies_from_browser: String,
    pub cookies_file: String,
    pub keep_downloaded_media: bool,
    pub force: bool,
    pub segment_minutes: u32,
    pub overlap_seconds: u32,
    pub request_delay_ms: u64,
    pub cache_limit_gib: u32,
    pub cache_max_age_days: u32,
    pub update_channel: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            output_dir: String::new(),
            download_dir: String::new(),
            work_dir: String::new(),
            model: DEFAULT_MODEL.to_string(),
            theme: Theme::Light,
            output_formats: vec![TranscriptFormat::Text, TranscriptFormat::Markdown],
            language: "auto".to_string(),
            prompt_preset: "default".to_string(),
            additional_prompt: String::new(),
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            downloader_path: String::new(),
            cookies_from_browser: String::new(),
            cookies_file: String::new(),
            keep_downloaded_media: false,
            force: false,
            segment_minutes: 20,
            overlap_seconds: 2,
            request_delay_ms: 0,
            cache_limit_gib: 20,
            cache_max_age_days: 30,
            update_channel: "stable".to_string(),
        }
    }
}

impl AppSettings {
    pub fn sanitized(mut self) -> Self {
        self.model = nonempty_or(self.model, DEFAULT_MODEL)
            .trim_start_matches("models/")
            .to_string();
        self.language = nonempty_or(self.language, "auto");
        self.prompt_preset = nonempty_or(self.prompt_preset, "default");
        self.update_channel = match self.update_channel.trim() {
            "beta" => "beta".to_string(),
            _ => "stable".to_string(),
        };
        self.output_formats.sort();
        self.output_formats.dedup();
        if self.output_formats.is_empty() {
            self.output_formats = vec![TranscriptFormat::Text, TranscriptFormat::Markdown];
        }
        self.segment_minutes = self.segment_minutes.clamp(5, 30);
        self.overlap_seconds = self.overlap_seconds.clamp(0, 10);
        self.request_delay_ms = self.request_delay_ms.min(120_000);
        self.cache_limit_gib = self.cache_limit_gib.clamp(1, 200);
        self.cache_max_age_days = self.cache_max_age_days.clamp(1, 365);
        self
    }
}

fn nonempty_or(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}
