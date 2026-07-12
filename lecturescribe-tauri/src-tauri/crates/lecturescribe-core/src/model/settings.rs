use super::DEFAULT_MODEL;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Transcribe,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptFormat {
    Text,
    Markdown,
    Srt,
    Vtt,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMode {
    #[default]
    Auto,
    Hints,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanguagePreferences {
    pub mode: LanguageMode,
    pub hints: Vec<String>,
}

impl Default for LanguagePreferences {
    fn default() -> Self {
        Self {
            mode: LanguageMode::Auto,
            hints: Vec::new(),
        }
    }
}

impl LanguagePreferences {
    pub fn from_legacy(value: impl AsRef<str>) -> Self {
        match value.as_ref().trim().to_ascii_lowercase().as_str() {
            "en" => Self {
                mode: LanguageMode::Hints,
                hints: vec!["en".to_string()],
            },
            "ar" => Self {
                mode: LanguageMode::Hints,
                hints: vec!["ar".to_string()],
            },
            _ => Self::default(),
        }
    }

    pub fn sanitized(mut self) -> Self {
        let mut seen = HashSet::new();
        self.hints = self
            .hints
            .into_iter()
            .filter_map(|hint| normalize_language_hint(&hint))
            .filter(|hint| seen.insert(hint.clone()))
            .take(5)
            .collect();
        if self.mode == LanguageMode::Hints && self.hints.is_empty() {
            self.mode = LanguageMode::Auto;
        }
        self
    }

    /// Compatibility view for callers that still use the former single language string.
    pub fn as_str(&self) -> &str {
        match self.mode {
            LanguageMode::Auto => "auto",
            LanguageMode::Hints => self.hints.first().map(String::as_str).unwrap_or("auto"),
        }
    }
}

impl<'de> Deserialize<'de> for LanguagePreferences {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(default)]
        struct ObjectPreferences {
            mode: LanguageMode,
            hints: Vec<String>,
        }

        impl Default for ObjectPreferences {
            fn default() -> Self {
                Self {
                    mode: LanguageMode::Auto,
                    hints: Vec::new(),
                }
            }
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum PreferencesWire {
            Object(ObjectPreferences),
            Legacy(String),
        }

        let preferences = match PreferencesWire::deserialize(deserializer)? {
            PreferencesWire::Object(value) => Self {
                mode: value.mode,
                hints: value.hints,
            },
            PreferencesWire::Legacy(value) => Self::from_legacy(value),
        };
        Ok(preferences.sanitized())
    }
}

fn normalize_language_hint(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 63 {
        return None;
    }
    let mut parts = value.split('-');
    let primary = parts.next()?;
    if !(2..=8).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return None;
    }
    if !parts.all(|part| {
        (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
    }) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputPackage {
    Readable,
    Subtitles,
    Complete,
    #[default]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub output_dir: String,
    pub download_dir: String,
    pub work_dir: String,
    pub model: String,
    pub theme: Theme,
    #[serde(default)]
    pub output_package: OutputPackage,
    pub output_formats: Vec<TranscriptFormat>,
    pub language: LanguagePreferences,
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
            output_package: OutputPackage::Readable,
            output_formats: vec![TranscriptFormat::Text, TranscriptFormat::Markdown],
            language: LanguagePreferences::default(),
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
        self.language = self.language.sanitized();
        self.prompt_preset = nonempty_or(self.prompt_preset, "default");
        self.update_channel = match self.update_channel.trim() {
            "beta" => "beta".to_string(),
            _ => "stable".to_string(),
        };
        self.output_formats = match self.output_package {
            OutputPackage::Readable => vec![TranscriptFormat::Text, TranscriptFormat::Markdown],
            OutputPackage::Subtitles => vec![TranscriptFormat::Srt, TranscriptFormat::Vtt],
            OutputPackage::Complete => vec![
                TranscriptFormat::Text,
                TranscriptFormat::Markdown,
                TranscriptFormat::Srt,
                TranscriptFormat::Vtt,
            ],
            OutputPackage::Custom => {
                self.output_formats.sort();
                self.output_formats.dedup();
                if self.output_formats.is_empty() {
                    self.output_package = OutputPackage::Readable;
                    vec![TranscriptFormat::Text, TranscriptFormat::Markdown]
                } else {
                    self.output_formats
                }
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_language_strings_deserialize_to_preferences() {
        let auto: LanguagePreferences = serde_json::from_str("\"auto\"").unwrap();
        let english: LanguagePreferences = serde_json::from_str("\"en\"").unwrap();
        let arabic: LanguagePreferences = serde_json::from_str("\"ar\"").unwrap();

        assert_eq!(auto, LanguagePreferences::default());
        assert_eq!(english.hints, vec!["en"]);
        assert_eq!(arabic.hints, vec!["ar"]);
        assert_eq!(serde_json::to_value(english).unwrap()["mode"], "hints");
    }

    #[test]
    fn language_hints_are_normalized_deduplicated_and_limited() {
        let preferences = LanguagePreferences {
            mode: LanguageMode::Hints,
            hints: vec![
                " EN ".to_string(),
                "en".to_string(),
                "ar-EG".to_string(),
                "bad_tag!".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "es".to_string(),
                "it".to_string(),
            ],
        }
        .sanitized();

        assert_eq!(preferences.mode, LanguageMode::Hints);
        assert_eq!(preferences.hints, vec!["en", "ar-eg", "fr", "de", "es"]);
    }

    #[test]
    fn output_packages_set_the_expected_formats() {
        let settings = AppSettings {
            output_package: OutputPackage::Subtitles,
            ..AppSettings::default()
        };
        assert_eq!(
            settings.sanitized().output_formats,
            vec![TranscriptFormat::Srt, TranscriptFormat::Vtt]
        );

        let settings = AppSettings {
            output_package: OutputPackage::Complete,
            ..AppSettings::default()
        };
        assert_eq!(settings.sanitized().output_formats.len(), 4);

        let settings = AppSettings {
            output_package: OutputPackage::Custom,
            output_formats: vec![
                TranscriptFormat::Srt,
                TranscriptFormat::Text,
                TranscriptFormat::Srt,
            ],
            ..AppSettings::default()
        };
        assert_eq!(
            settings.sanitized().output_formats,
            vec![TranscriptFormat::Text, TranscriptFormat::Srt]
        );
    }

    #[test]
    fn legacy_settings_without_an_output_package_keep_custom_formats() {
        let settings: AppSettings = serde_json::from_str(r#"{"output_formats":["srt"]}"#).unwrap();

        assert_eq!(settings.output_package, OutputPackage::Custom);
        assert_eq!(
            settings.sanitized().output_formats,
            vec![TranscriptFormat::Srt]
        );
    }
}
