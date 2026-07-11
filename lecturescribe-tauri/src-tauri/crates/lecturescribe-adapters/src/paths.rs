use lecturescribe_core::{AppError, AppSettings, ErrorCategory};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub tools_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub database_path: PathBuf,
    pub install_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        let data_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("LectureScribe");
        let install_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        Self {
            tools_dir: data_dir.join("tools"),
            cache_dir: data_dir.join("cache"),
            logs_dir: data_dir.join("logs"),
            database_path: data_dir.join("lecturescribe.sqlite3"),
            data_dir,
            install_dir,
        }
    }

    pub fn ensure(&self) -> Result<(), AppError> {
        for path in [
            &self.data_dir,
            &self.tools_dir,
            &self.cache_dir,
            &self.logs_dir,
        ] {
            fs::create_dir_all(path).map_err(|error| {
                AppError::new(
                    "app_folder_create_failed",
                    ErrorCategory::Filesystem,
                    "LectureScribe could not create its local folders.",
                    format!("{}: {error}", path.display()),
                )
            })?;
        }
        Ok(())
    }

    pub fn settings_with_defaults(&self, mut settings: AppSettings) -> AppSettings {
        if settings.output_dir.trim().is_empty() {
            settings.output_dir = self
                .data_dir
                .join("Transcripts")
                .to_string_lossy()
                .to_string();
        }
        if settings.download_dir.trim().is_empty() {
            settings.download_dir = self
                .data_dir
                .join("Downloads")
                .to_string_lossy()
                .to_string();
        }
        if settings.work_dir.trim().is_empty() {
            settings.work_dir = self.cache_dir.to_string_lossy().to_string();
        }
        settings.sanitized()
    }

    pub fn item_cache(&self, cache_key: &str) -> PathBuf {
        self.cache_dir.join(safe_component(cache_key))
    }

    pub fn bundled_downloader_candidates(&self) -> Vec<PathBuf> {
        let name = executable_name("yt-dlp");
        vec![
            self.install_dir.join(&name),
            self.install_dir.join("resources").join(&name),
            self.install_dir.join("resources").join("tools").join(&name),
        ]
    }
}

pub fn executable_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn safe_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim_matches(['.', ' ']);
    if cleaned.is_empty() {
        "item".to_string()
    } else {
        cleaned.chars().take(120).collect()
    }
}
