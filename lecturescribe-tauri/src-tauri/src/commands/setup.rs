use super::blocking;
use crate::app_state::AppState;
use lecturescribe_adapters::media::probe_media;
use lecturescribe_adapters::process::{run_output, CommandSpec};
use lecturescribe_adapters::{CredentialStore, GeminiClient, ToolResolver};
use lecturescribe_core::{
    AppError, AppSettings, EnvironmentSnapshot, ErrorCategory, ErrorSeverity, SetupTestResult,
    ToolStatus,
};
use lecturescribe_engine::{JobControl, Store};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

#[tauri::command]
pub fn load_settings(state: State<'_, AppState>) -> Result<AppSettings, AppError> {
    state.settings()
}

#[tauri::command]
pub fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, AppError> {
    state.save_settings(settings)
}

#[tauri::command]
pub fn save_api_key(api_key: String, state: State<'_, AppState>) -> Result<(), AppError> {
    state.credentials.save_gemini_key(&api_key)?;
    state.mark_api_verified(false);
    Ok(())
}

#[tauri::command]
pub fn delete_api_key(state: State<'_, AppState>) -> Result<(), AppError> {
    state.credentials.delete_gemini_key()?;
    state.mark_api_verified(false);
    Ok(())
}

#[tauri::command]
pub async fn check_environment(
    state: State<'_, AppState>,
) -> Result<EnvironmentSnapshot, AppError> {
    let store = state.store.clone();
    let credentials = state.credentials.clone();
    let tools = state.tools.clone();
    let verified = state.api_verified();
    let settings = state.settings()?;
    blocking(move || collect_environment(store, credentials, tools, settings, verified)).await
}

#[tauri::command]
pub async fn install_downloader(state: State<'_, AppState>) -> Result<ToolStatus, AppError> {
    let tools = state.tools.clone();
    blocking(move || tools.install_downloader().map(|tool| tool.status)).await
}

#[tauri::command]
pub async fn run_setup_test(state: State<'_, AppState>) -> Result<SetupTestResult, AppError> {
    let paths = state.paths.clone();
    let tools = state.tools.clone();
    let credentials = state.credentials.clone();
    let settings = state.settings()?;
    let model = settings.model.clone();
    let result = blocking(move || {
        let resolved = tools.resolve(&settings);
        let ffmpeg = required_tool(
            resolved.ffmpeg.path,
            "setup_ffmpeg_missing",
            "FFmpeg is required for the setup test.",
        )?;
        let ffprobe = required_tool(
            resolved.ffprobe.path,
            "setup_ffprobe_missing",
            "FFprobe is required for the setup test.",
        )?;
        if !credentials.configured() {
            return Err(AppError::new(
                "setup_api_key_missing",
                ErrorCategory::Authentication,
                "Add a Gemini API key before running the setup test.",
                "No Gemini credential was found in Windows Credential Manager.",
            )
            .with_action("open_setup_api", "Add API key", "open_setup_api"));
        }

        let control = JobControl::default();
        let audio_path = paths.cache_dir.join("setup-test.wav");
        let mut command = CommandSpec::new(ffmpeg);
        command.args = vec![
            "-y".to_string(),
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "error".to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "anullsrc=r=16000:cl=mono".to_string(),
            "-t".to_string(),
            "1".to_string(),
            "-c:a".to_string(),
            "pcm_s16le".to_string(),
            audio_path.to_string_lossy().to_string(),
        ];
        command.timeout = Duration::from_secs(30);
        let generated = run_output(&command, &control)?;
        if !generated.status.success() {
            return Err(AppError::new(
                "setup_audio_failed",
                ErrorCategory::Media,
                "FFmpeg could not create the setup-test audio.",
                generated.stderr,
            ));
        }
        probe_media(&ffprobe, &audio_path, &control)?;
        let client = GeminiClient::new(credentials)?;
        let preview = client.setup_test(&model, &audio_path, &control)?;
        Ok(SetupTestResult {
            ok: true,
            message: "Gemini, FFmpeg, FFprobe, and the audio pipeline are ready.".to_string(),
            model,
            transcript_preview: preview,
        })
    })
    .await?;
    state.mark_api_verified(true);
    Ok(result)
}

pub(crate) fn collect_environment(
    store: Arc<Store>,
    credentials: CredentialStore,
    tools: ToolResolver,
    settings: AppSettings,
    verified: bool,
) -> Result<EnvironmentSnapshot, AppError> {
    let resolved = tools.resolve(&settings);
    let output = PathBuf::from(&settings.output_dir);
    let output_writable = tools.output_writable(&output);
    let api_key_configured = credentials.configured();
    let database_ok = store.integrity_ok();
    let mut problems = Vec::new();

    if !api_key_configured {
        problems.push(
            AppError::new(
                "api_key_missing",
                ErrorCategory::Authentication,
                "Add a Gemini API key to transcribe media.",
                "No credential is configured.",
            )
            .with_action("open_setup_api", "Add API key", "open_setup_api"),
        );
    }
    if resolved.ffmpeg.path.is_none() || resolved.ffprobe.path.is_none() {
        problems.push(
            AppError::new(
                "ffmpeg_suite_missing",
                ErrorCategory::Setup,
                "Install or choose FFmpeg and FFprobe to process media.",
                "The verified FFmpeg tool pair was incomplete.",
            )
            .with_action("open_setup_ffmpeg", "Fix FFmpeg", "open_setup_ffmpeg"),
        );
    }
    if resolved.downloader.path.is_none() {
        problems.push(
            AppError::new(
                "downloader_missing",
                ErrorCategory::Setup,
                "Install the Downloader to use YouTube and Google Drive links.",
                "No verified yt-dlp executable was found.",
            )
            .with_action(
                "install_downloader",
                "Install Downloader",
                "install_downloader",
            ),
        );
    }
    if !output_writable {
        problems.push(
            AppError::new(
                "output_not_writable",
                ErrorCategory::Filesystem,
                "Choose a writable output folder.",
                "The configured output directory failed a write test.",
            )
            .with_action(
                "choose_output_folder",
                "Choose output folder",
                "choose_output_folder",
            ),
        );
    }
    if !database_ok {
        let mut problem = AppError::new(
            "database_integrity_failed",
            ErrorCategory::Database,
            "The local run database needs attention.",
            "SQLite quick_check did not return ok.",
        );
        problem.severity = ErrorSeverity::Fatal;
        problems.push(problem);
    }

    Ok(EnvironmentSnapshot {
        api_key_configured,
        api_key_verified: api_key_configured && verified,
        ffmpeg: resolved.ffmpeg.status,
        ffprobe: resolved.ffprobe.status,
        downloader: resolved.downloader.status,
        output_writable,
        free_disk_bytes: tools.free_disk_bytes(&output),
        database_ok,
        network_online: None,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        setup_complete: api_key_configured
            && resolved.ffmpeg.path.is_some()
            && resolved.ffprobe.path.is_some()
            && resolved.downloader.path.is_some()
            && output_writable
            && database_ok,
        problems,
    })
}

fn required_tool(path: Option<PathBuf>, code: &str, message: &str) -> Result<PathBuf, AppError> {
    path.ok_or_else(|| {
        AppError::new(
            code,
            ErrorCategory::Setup,
            message,
            "Tool resolution failed.",
        )
        .with_action("open_setup", "Open setup", "open_setup")
    })
}
