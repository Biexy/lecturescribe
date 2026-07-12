use super::blocking;
use crate::app_state::AppState;
use lecturescribe_adapters::media::probe_media;
use lecturescribe_adapters::models::successful_model_validation;
use lecturescribe_adapters::process::{run_output, CommandSpec};
use lecturescribe_adapters::{CredentialStore, GeminiClient, ToolResolver};
use lecturescribe_core::{
    AppError, AppSettings, CapabilityStatus, EnvironmentSnapshot, ErrorCategory, ErrorSeverity,
    ModelOption, ModelValidation, SetupCapabilities, SetupTestResult, ToolStatus,
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
pub async fn list_transcription_models(
    custom_model: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ModelOption>, AppError> {
    let credentials = state.credentials.clone();
    blocking(move || {
        GeminiClient::new(credentials)?.list_transcription_models(custom_model.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn validate_transcription_model(
    model: String,
    run_audio_test: Option<bool>,
    state: State<'_, AppState>,
) -> Result<ModelValidation, AppError> {
    let paths = state.paths.clone();
    let tools = state.tools.clone();
    let credentials = state.credentials.clone();
    let verification_credentials = credentials.clone();
    let settings = state.settings()?;
    let result = blocking(move || {
        let client = GeminiClient::new(credentials.clone())?;
        let validation = client.validate_transcription_model(&model)?;
        if !run_audio_test.unwrap_or(false) {
            return Ok(validation);
        }

        run_setup_audio_test(paths, tools, credentials, settings, model.clone())?;
        successful_model_validation(
            &model,
            "This Gemini model completed the audio setup test successfully.",
        )
    })
    .await?;
    verification_credentials.mark_gemini_key_verified()?;
    state.mark_api_verified(true);
    Ok(result)
}

#[tauri::command]
pub async fn run_setup_test(
    model: Option<String>,
    state: State<'_, AppState>,
) -> Result<SetupTestResult, AppError> {
    let paths = state.paths.clone();
    let tools = state.tools.clone();
    let credentials = state.credentials.clone();
    let verification_credentials = credentials.clone();
    let settings = state.settings()?;
    let model = selected_setup_model(model, &settings);
    let result =
        blocking(move || run_setup_audio_test(paths, tools, credentials, settings, model)).await?;
    verification_credentials.mark_gemini_key_verified()?;
    state.mark_api_verified(true);
    Ok(result)
}

fn run_setup_audio_test(
    paths: lecturescribe_adapters::AppPaths,
    tools: ToolResolver,
    credentials: CredentialStore,
    settings: AppSettings,
    model: String,
) -> Result<SetupTestResult, AppError> {
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
}

fn selected_setup_model(requested: Option<String>, settings: &AppSettings) -> String {
    requested
        .map(|model| model.trim().trim_start_matches("models/").to_string())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| settings.model.clone())
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
    let ffmpeg_ready = resolved.ffmpeg.status.ready();
    let ffprobe_ready = resolved.ffprobe.status.ready();
    let downloader_ready = resolved.downloader.status.ready();
    let capabilities = build_capabilities(
        api_key_configured,
        verified,
        ffmpeg_ready,
        ffprobe_ready,
        downloader_ready,
        output_writable,
        database_ok,
    );
    let problems = build_problems(
        api_key_configured,
        verified,
        ffmpeg_ready,
        ffprobe_ready,
        downloader_ready,
        output_writable,
        database_ok,
    );

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
        // Retained for frontend compatibility; capability readiness is authoritative.
        setup_complete: capabilities.transcribe_links.ready,
        capabilities,
        problems,
    })
}

fn build_capabilities(
    api_key_configured: bool,
    api_key_verified: bool,
    ffmpeg_ready: bool,
    ffprobe_ready: bool,
    downloader_ready: bool,
    output_writable: bool,
    database_ok: bool,
) -> SetupCapabilities {
    let common = common_blockers(output_writable, database_ok);
    let mut download_links = common.clone();
    if !downloader_ready {
        download_links.push(downloader_problem());
    }

    let mut transcribe_local = common.clone();
    if !api_key_configured || !api_key_verified {
        transcribe_local.push(api_key_problem(api_key_configured));
    }
    if !ffmpeg_ready || !ffprobe_ready {
        transcribe_local.push(ffmpeg_suite_problem());
    }

    let mut transcribe_links = transcribe_local.clone();
    if !downloader_ready {
        transcribe_links.push(downloader_problem());
    }

    SetupCapabilities {
        download_links: capability_status(download_links),
        transcribe_local: capability_status(transcribe_local),
        transcribe_links: capability_status(transcribe_links),
    }
}

fn capability_status(blockers: Vec<AppError>) -> CapabilityStatus {
    CapabilityStatus {
        ready: blockers.is_empty(),
        blockers,
    }
}

fn build_problems(
    api_key_configured: bool,
    api_key_verified: bool,
    ffmpeg_ready: bool,
    ffprobe_ready: bool,
    downloader_ready: bool,
    output_writable: bool,
    database_ok: bool,
) -> Vec<AppError> {
    let mut problems = Vec::new();

    if !api_key_configured || !api_key_verified {
        problems.push(api_key_problem(api_key_configured));
    }
    if !ffmpeg_ready || !ffprobe_ready {
        problems.push(ffmpeg_suite_problem());
    }
    if !downloader_ready {
        problems.push(downloader_problem());
    }
    problems.extend(common_blockers(output_writable, database_ok));
    problems
}

fn common_blockers(output_writable: bool, database_ok: bool) -> Vec<AppError> {
    let mut blockers = Vec::new();
    if !output_writable {
        blockers.push(output_not_writable_problem());
    }
    if !database_ok {
        blockers.push(database_integrity_problem());
    }
    blockers
}

fn api_key_problem(configured: bool) -> AppError {
    if configured {
        AppError::new(
            "api_key_unverified",
            ErrorCategory::Authentication,
            "Test the saved Gemini API key before transcribing media.",
            "A credential is stored, but it has not passed model validation.",
        )
        .with_action("open_setup_api", "Test API key", "open_setup_api")
    } else {
        AppError::new(
            "api_key_missing",
            ErrorCategory::Authentication,
            "Add a Gemini API key to transcribe media.",
            "No credential is configured.",
        )
        .with_action("open_setup_api", "Add API key", "open_setup_api")
    }
}

fn ffmpeg_suite_problem() -> AppError {
    AppError::new(
        "ffmpeg_suite_missing",
        ErrorCategory::Setup,
        "Install or choose FFmpeg and FFprobe to process media.",
        "The verified FFmpeg tool pair was incomplete.",
    )
    .with_action("open_setup_ffmpeg", "Fix FFmpeg", "open_setup_ffmpeg")
}

fn downloader_problem() -> AppError {
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
    )
}

fn output_not_writable_problem() -> AppError {
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
    )
}

fn database_integrity_problem() -> AppError {
    let mut problem = AppError::new(
        "database_integrity_failed",
        ErrorCategory::Database,
        "The local run database needs attention.",
        "SQLite quick_check did not return ok.",
    );
    problem.severity = ErrorSeverity::Fatal;
    problem
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

#[cfg(test)]
mod tests {
    use super::{build_capabilities, selected_setup_model};
    use lecturescribe_core::AppSettings;
    use lecturescribe_core::CapabilityStatus;

    fn blocker_codes(status: &CapabilityStatus) -> Vec<&str> {
        status
            .blockers
            .iter()
            .map(|blocker| blocker.code.as_str())
            .collect()
    }

    #[test]
    fn missing_api_key_does_not_block_downloading_links() {
        let capabilities = build_capabilities(false, false, true, true, true, true, true);

        assert!(capabilities.download_links.ready);
        assert!(capabilities.download_links.blockers.is_empty());
        assert_eq!(
            blocker_codes(&capabilities.transcribe_local),
            vec!["api_key_missing"]
        );
    }

    #[test]
    fn missing_downloader_does_not_block_transcribing_local_media() {
        let capabilities = build_capabilities(true, true, true, true, false, true, true);

        assert!(capabilities.transcribe_local.ready);
        assert!(capabilities.transcribe_local.blockers.is_empty());
        assert_eq!(
            blocker_codes(&capabilities.download_links),
            vec!["downloader_missing"]
        );
    }

    #[test]
    fn combined_capabilities_report_only_their_exact_blockers() {
        let capabilities = build_capabilities(false, false, false, true, false, false, false);

        assert_eq!(
            blocker_codes(&capabilities.download_links),
            vec![
                "output_not_writable",
                "database_integrity_failed",
                "downloader_missing",
            ]
        );
        assert_eq!(
            blocker_codes(&capabilities.transcribe_local),
            vec![
                "output_not_writable",
                "database_integrity_failed",
                "api_key_missing",
                "ffmpeg_suite_missing",
            ]
        );
        assert_eq!(
            blocker_codes(&capabilities.transcribe_links),
            vec![
                "output_not_writable",
                "database_integrity_failed",
                "api_key_missing",
                "ffmpeg_suite_missing",
                "downloader_missing",
            ]
        );
    }

    #[test]
    fn saved_but_unverified_key_blocks_transcription_with_a_test_action() {
        let capabilities = build_capabilities(true, false, true, true, true, true, true);

        assert!(capabilities.download_links.ready);
        assert_eq!(
            blocker_codes(&capabilities.transcribe_local),
            vec!["api_key_unverified"]
        );
    }

    #[test]
    fn setup_test_uses_an_explicit_model_without_changing_saved_settings() {
        let settings = AppSettings {
            model: "gemini-3.1-flash-lite".to_string(),
            ..AppSettings::default()
        };

        let selected = selected_setup_model(Some("models/gemini-3.5-flash".to_string()), &settings);

        assert_eq!(selected, "gemini-3.5-flash");
        assert_eq!(settings.model, "gemini-3.1-flash-lite");
    }
}
