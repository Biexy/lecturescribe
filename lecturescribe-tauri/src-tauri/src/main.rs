use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const DEFAULT_MODEL: &str = "gemini-3.1-flash-lite";
const API_KEY_SERVICE: &str = "LectureScribe";
const API_KEY_USER: &str = "gemini_api_key";
const YT_DLP_WINDOWS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const CHUNK_CACHE_VERSION: &str = "audio-v2";
const INLINE_AUDIO_LIMIT_BYTES: u64 = 18 * 1024 * 1024;
const MEDIA_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "mp4", "webm", "wav", "aac", "flac", "ogg", "opus", "mov", "mkv",
];
const SKIP_MEDIA_FOLDER_NAMES: &[&str] = &[
    ".git",
    "__pycache__",
    "node_modules",
    "target",
    "dist",
    "_work",
    "audio_chunks",
    "chunk_text",
];
const CANCELLED_MESSAGE: &str = "Transcription cancelled by user.";

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueueItem {
    id: String,
    number: usize,
    source_type: String,
    title: String,
    source: String,
    url: String,
    media_path: String,
    thumbnail_path: String,
    transcript_path: String,
    markdown_path: String,
    downloaded_media_path: String,
    estimated_chunks: usize,
    duplicate_of: Option<String>,
    selected: bool,
    status: String,
    error: Option<String>,
    fix_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ToolStatus {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct EnvironmentStatus {
    ffmpeg: ToolStatus,
    yt_dlp: ToolStatus,
    native_engine: ToolStatus,
    api_key_ok: bool,
    default_output_dir: String,
    default_download_dir: String,
    legacy_root: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AppSettings {
    output_dir: String,
    download_dir: String,
    work_dir: String,
    model: String,
    #[serde(default)]
    run_mode: String,
    #[serde(default)]
    transcript_format: String,
    #[serde(default)]
    prompt_preset: String,
    #[serde(default)]
    ffmpeg_path: String,
    #[serde(default)]
    downloader_path: String,
    chunk_minutes: u32,
    request_delay_seconds: f64,
    #[serde(default)]
    cookies_from_browser: String,
    #[serde(default)]
    cookies_file: String,
    skip_download: bool,
    force: bool,
}

#[derive(Debug, Clone, Serialize)]
struct EngineDone {
    code: Option<i32>,
    success: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SetupTestResult {
    ok: bool,
    message: String,
    transcript_preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct EngineProgress {
    phase: String,
    message: String,
    status: String,
    current_item: Option<usize>,
    total_items: usize,
    completed_items: usize,
    chunk_current: usize,
    chunk_total: usize,
    download_speed: String,
    percent: f64,
}

#[derive(Debug, Clone)]
struct SourceBundle {
    urls: Vec<String>,
    media_sources: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct MediaItem {
    number: usize,
    title: String,
    source: String,
    media_path: PathBuf,
    item_id: String,
}

#[derive(Debug, Clone)]
struct RunState {
    total_items: usize,
    completed_items: usize,
}

#[derive(Debug, Clone)]
struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
}

#[tauri::command]
fn preview_inputs(inputs: Vec<String>) -> Result<Vec<QueueItem>, String> {
    let root = legacy_root();
    let settings = load_settings_from_root(&root);
    let sources = resolve_sources(&root, &inputs, true)?;
    build_preview_queue(&root, &settings, &sources)
}

#[tauri::command]
fn check_environment() -> EnvironmentStatus {
    let root = legacy_root();
    let settings = load_settings_from_root(&root);
    EnvironmentStatus {
        ffmpeg: check_ffmpeg_with_settings(&settings),
        yt_dlp: check_yt_dlp(&root, &settings),
        native_engine: ToolStatus {
            name: "Native engine".to_string(),
            ok: true,
            detail: "Rust engine ready; Python is not required".to_string(),
        },
        api_key_ok: has_api_key(&root),
        default_output_dir: settings.output_dir,
        default_download_dir: settings.download_dir,
        legacy_root: root.to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn load_settings() -> AppSettings {
    load_settings_from_root(&legacy_root())
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<AppSettings, String> {
    let root = legacy_root();
    let settings = sanitize_settings(&root, settings);
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|error| format!("Failed to serialize settings: {error}"))?;
    fs::write(settings_path(&root), text)
        .map_err(|error| format!("Failed to save settings: {error}"))?;
    Ok(settings)
}

#[tauri::command]
fn save_api_key(api_key: String) -> Result<(), String> {
    let value = api_key.trim();
    if !valid_api_key(value) {
        return Err("Enter a valid Gemini API key first.".to_string());
    }

    let entry = keyring::Entry::new(API_KEY_SERVICE, API_KEY_USER)
        .map_err(|error| format!("Could not open secure credential store: {error}"))?;
    entry
        .set_password(value)
        .map_err(|error| format!("Could not save API key securely: {error}"))?;
    Ok(())
}

#[tauri::command]
fn check_downloader() -> ToolStatus {
    let root = legacy_root();
    let settings = load_settings_from_root(&root);
    check_yt_dlp(&root, &settings)
}

#[tauri::command]
fn install_downloader() -> Result<ToolStatus, String> {
    install_or_update_downloader()
}

#[tauri::command]
fn update_downloader() -> Result<ToolStatus, String> {
    install_or_update_downloader()
}

#[tauri::command]
fn choose_downloader(path: String) -> Result<AppSettings, String> {
    let root = legacy_root();
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return Err("Downloader file was not found.".to_string());
    }
    let mut settings = load_settings_from_root(&root);
    settings.downloader_path = target.to_string_lossy().to_string();
    save_settings(settings)
}

#[tauri::command]
fn check_ffmpeg() -> ToolStatus {
    let settings = load_settings();
    check_ffmpeg_with_settings(&settings)
}

#[tauri::command]
fn install_ffmpeg() -> Result<ToolStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("winget")
            .args([
                "install",
                "--id",
                "Gyan.FFmpeg",
                "-e",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ])
            .status()
            .map_err(|error| {
                format!(
                    "Could not start winget. Install FFmpeg manually or choose ffmpeg.exe: {error}"
                )
            })?;
        if !status.success() {
            return Err(
                "winget could not install FFmpeg. Install FFmpeg manually, then choose ffmpeg.exe."
                    .to_string(),
            );
        }
        Ok(check_ffmpeg())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Automatic FFmpeg install is only available on Windows for now.".to_string())
    }
}

#[tauri::command]
fn choose_ffmpeg(path: String) -> Result<AppSettings, String> {
    let root = legacy_root();
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return Err("FFmpeg file was not found.".to_string());
    }
    let mut settings = load_settings_from_root(&root);
    settings.ffmpeg_path = target.to_string_lossy().to_string();
    save_settings(settings)
}

#[tauri::command]
fn api_key_ready() -> bool {
    has_api_key(&legacy_root())
}

#[tauri::command]
fn count_links_in_file(path: String) -> Result<usize, String> {
    let path = PathBuf::from(path);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    Ok(extract_urls(&text).len())
}

#[tauri::command]
fn start_transcription(
    app: AppHandle,
    inputs: Vec<String>,
    settings: AppSettings,
) -> Result<(), String> {
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    let root = legacy_root();
    let settings = sanitize_settings(&root, settings);

    let sources = resolve_sources(&root, &inputs, true)?;
    if sources.urls.is_empty() && sources.media_sources.is_empty() {
        return Err(
            "No sources found. Add links, a .txt link file, or media files first.".to_string(),
        );
    }

    let mode = normalized_run_mode(&settings);
    let needs_transcription = mode != "download_only";
    let needs_downloader = !sources.urls.is_empty() && mode != "transcribe_existing";

    let api_key = if needs_transcription {
        Some(api_key_from_env_or_file(&root).ok_or_else(|| {
            "API key missing. Add your Gemini API key in Setup, then start again.".to_string()
        })?)
    } else {
        None
    };

    if needs_transcription && !check_ffmpeg_with_settings(&settings).ok {
        return Err(
            "FFmpeg is missing. Install FFmpeg or choose ffmpeg.exe in Setup before transcription."
                .to_string(),
        );
    }

    if needs_downloader && !check_yt_dlp(&root, &settings).ok {
        return Err(
            "Downloader is missing. Install or update the Downloader in Setup before using links."
                .to_string(),
        );
    }

    let preview = build_preview_queue(&root, &settings, &sources)?;
    if preview.is_empty() {
        return Err(
            "Preview found 0 transcriptable items. Add valid links or local audio/video files."
                .to_string(),
        );
    }

    thread::spawn(move || {
        let result = run_native_engine(app.clone(), root, api_key, sources, settings);
        if let Err(error) = result {
            if is_cancelled_error(&error) {
                emit_line(&app, "[~] Transcription cancelled.");
                emit_progress(
                    &app,
                    EngineProgress {
                        phase: "Cancelled".to_string(),
                        message: CANCELLED_MESSAGE.to_string(),
                        status: "Cancelled".to_string(),
                        current_item: None,
                        total_items: 0,
                        completed_items: 0,
                        chunk_current: 0,
                        chunk_total: 0,
                        download_speed: "0 KB/s".to_string(),
                        percent: 0.0,
                    },
                );
            } else {
                emit_line(&app, format!("[X] {error}"));
            }
            emit_done(&app, false, None);
        }
    });

    Ok(())
}

#[tauri::command]
fn cancel_transcription() -> Result<(), String> {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn run_setup_test() -> Result<SetupTestResult, String> {
    let root = legacy_root();
    let settings = load_settings_from_root(&root);
    let api_key = api_key_from_env_or_file(&root).ok_or_else(|| {
        "API key missing. Save your Gemini API key in Settings, then run the setup test again."
            .to_string()
    })?;

    if !check_ffmpeg_with_settings(&settings).ok {
        return Err("FFmpeg is missing. Install FFmpeg and make sure it is available on PATH before running the setup test.".to_string());
    }

    let work_dir = PathBuf::from(&settings.work_dir).join("setup_test");
    fs::create_dir_all(&work_dir)
        .map_err(|error| format!("Failed to create setup test folder: {error}"))?;
    let sample = work_dir.join("setup-test.mp3");

    let mut command = Command::new(ffmpeg_command(&settings));
    command
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("anullsrc=channel_layout=mono:sample_rate=16000")
        .arg("-t")
        .arg("1")
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("64k")
        .arg(&sample);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|error| format!("Failed to create setup test audio with FFmpeg: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg setup test failed: {}", stderr.trim()));
    }

    let client = GeminiClient::new(api_key, settings.model)?;
    let prompt = "This is a LectureScribe setup test. The attached audio may be silent. Reply with exactly SETUP_TEST_OK if the API key, model, and audio request pipeline work.";
    let transcript = client.generate_audio_transcript(&sample, prompt)?;
    Ok(SetupTestResult {
        ok: true,
        message:
            "Setup test passed. FFmpeg created audio and Gemini responded to one test request."
                .to_string(),
        transcript_preview: shorten(&collapse_whitespace(&transcript), 180),
    })
}

#[tauri::command]
fn open_output_folder(path: String) -> Result<(), String> {
    open_output_dir(path)
}

#[tauri::command]
fn open_transcript(path: String) -> Result<(), String> {
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return Err(
            "Transcript file was not found yet. Finish or retry the item first.".to_string(),
        );
    }
    open_path(&target)
}

#[tauri::command]
fn reveal_media(path: String) -> Result<(), String> {
    let target = PathBuf::from(path.trim());
    if target.as_os_str().is_empty() {
        return Err("No media path is available yet.".to_string());
    }
    let folder = if target.is_dir() {
        target
    } else {
        target
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "Could not find the media folder.".to_string())?
    };
    if !folder.exists() {
        return Err("Media folder was not found yet.".to_string());
    }
    open_path(&folder)
}

#[tauri::command]
fn copy_output_path() -> Result<String, String> {
    Ok(load_settings().output_dir)
}

#[tauri::command]
fn open_output_dir(path: String) -> Result<(), String> {
    let target = if path.trim().is_empty() {
        legacy_root().join("Transcripts").join("organized")
    } else {
        PathBuf::from(path)
    };

    fs::create_dir_all(&target)
        .map_err(|error| format!("Failed to create {}: {error}", target.display()))?;
    open_path(&target)
}

fn open_path(target: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(target)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", target.display()))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", target.display()))?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", target.display()))?;
    }

    Ok(())
}

fn run_native_engine(
    app: AppHandle,
    root: PathBuf,
    api_key: Option<String>,
    sources: SourceBundle,
    settings: AppSettings,
) -> Result<(), String> {
    ensure_not_cancelled()?;
    fs::create_dir_all(&settings.output_dir)
        .map_err(|error| format!("Failed to create output folder: {error}"))?;
    fs::create_dir_all(&settings.download_dir)
        .map_err(|error| format!("Failed to create download folder: {error}"))?;
    fs::create_dir_all(&settings.work_dir)
        .map_err(|error| format!("Failed to create work folder: {error}"))?;

    emit_progress(
        &app,
        EngineProgress {
            phase: "Preparing".to_string(),
            message: "Checking sources and cached media...".to_string(),
            status: "Running".to_string(),
            current_item: None,
            total_items: 0,
            completed_items: 0,
            chunk_current: 0,
            chunk_total: 0,
            download_speed: "0 KB/s".to_string(),
            percent: 0.0,
        },
    );

    let mode = normalized_run_mode(&settings);
    if !sources.urls.is_empty() {
        if mode == "transcribe_existing" || settings.skip_download {
            emit_line(
                &app,
                "[~] Skip download is enabled; using existing media only.",
            );
        } else {
            ensure_not_cancelled()?;
            download_urls(&app, &root, &settings, &sources.urls)?;
        }
    }

    if mode == "download_only" {
        emit_line(&app, "[OK] Download run complete.");
        emit_progress(
            &app,
            EngineProgress {
                phase: "Complete".to_string(),
                message: "Download run complete".to_string(),
                status: "Done".to_string(),
                current_item: None,
                total_items: sources.urls.len(),
                completed_items: sources.urls.len(),
                chunk_current: 0,
                chunk_total: 0,
                download_speed: "0 KB/s".to_string(),
                percent: 100.0,
            },
        );
        emit_done(&app, true, Some(0));
        return Ok(());
    }

    ensure_not_cancelled()?;
    let items = build_media_items(&root, &settings, &sources)?;
    if items.is_empty() {
        return Err("No media files found after preview/download. Check the links or add local media files.".to_string());
    }

    write_index(Path::new(&settings.output_dir), &items)?;
    emit_line(
        &app,
        format!(
            "[+] Found {} media item(s). Output: {}",
            items.len(),
            settings.output_dir
        ),
    );

    let mut run_state = RunState {
        total_items: items.len(),
        completed_items: 0,
    };
    let api_key =
        api_key.ok_or_else(|| "API key missing. Add your Gemini API key in Setup.".to_string())?;
    let client = GeminiClient::new(api_key, settings.model.clone())?;
    let chunk_seconds = settings.chunk_minutes * 60;

    for item in &items {
        ensure_not_cancelled()?;
        transcribe_item(
            &app,
            &client,
            &settings,
            Path::new(&settings.output_dir),
            Path::new(&settings.work_dir),
            chunk_seconds,
            item,
            &mut run_state,
        )?;
        write_index(Path::new(&settings.output_dir), &items)?;
    }

    write_index(Path::new(&settings.output_dir), &items)?;
    emit_line(&app, "[OK] Transcription run complete.");
    emit_progress(
        &app,
        EngineProgress {
            phase: "Complete".to_string(),
            message: "Transcription run complete".to_string(),
            status: "Done".to_string(),
            current_item: None,
            total_items: run_state.total_items,
            completed_items: run_state.completed_items,
            chunk_current: 0,
            chunk_total: 0,
            download_speed: "0 KB/s".to_string(),
            percent: 100.0,
        },
    );
    emit_done(&app, true, Some(0));
    Ok(())
}

fn download_urls(
    app: &AppHandle,
    root: &Path,
    settings: &AppSettings,
    urls: &[String],
) -> Result<(), String> {
    let yt_dlp = yt_dlp_command(root, settings)?;
    let download_dir = PathBuf::from(&settings.download_dir);
    fs::create_dir_all(&download_dir)
        .map_err(|error| format!("Failed to create download folder: {error}"))?;
    let output_template = download_dir.join("%(title).200B [%(id)s].%(ext)s");

    emit_line(
        app,
        format!(
            "[+] Downloading {} URL(s) to {}",
            urls.len(),
            download_dir.display()
        ),
    );

    for (index, original_url) in urls.iter().enumerate() {
        ensure_not_cancelled()?;
        let url = normalize_google_drive_url(original_url).unwrap_or_else(|| original_url.clone());
        emit_line(app, format!("[+] Download {}/{}", index + 1, urls.len()));
        emit_progress(
            app,
            EngineProgress {
                phase: "Downloading".to_string(),
                message: format!("Downloading {}/{}", index + 1, urls.len()),
                status: "Running".to_string(),
                current_item: None,
                total_items: urls.len(),
                completed_items: index,
                chunk_current: 0,
                chunk_total: 0,
                download_speed: "0 KB/s".to_string(),
                percent: progress_percent(index, urls.len()),
            },
        );

        let mut command = Command::new(&yt_dlp);
        command
            .current_dir(root)
            .arg("--ignore-errors")
            .arg("--no-overwrites")
            .arg("--newline")
            .arg("--extract-audio")
            .arg("--audio-format")
            .arg("mp3")
            .arg("--audio-quality")
            .arg("0")
            .arg("--output")
            .arg(&output_template);

        if !settings.cookies_from_browser.trim().is_empty() {
            command
                .arg("--cookies-from-browser")
                .arg(settings.cookies_from_browser.trim());
        }
        if !settings.cookies_file.trim().is_empty() {
            command.arg("--cookies").arg(settings.cookies_file.trim());
        }
        command.arg(&url);

        let status = stream_command(command, |line| {
            emit_line(app, line);
            if let Some((percent, speed)) = parse_download_progress(line) {
                emit_progress(
                    app,
                    EngineProgress {
                        phase: "Downloading".to_string(),
                        message: format!("Downloading {}/{}", index + 1, urls.len()),
                        status: "Running".to_string(),
                        current_item: None,
                        total_items: urls.len(),
                        completed_items: index,
                        chunk_current: 0,
                        chunk_total: 0,
                        download_speed: speed,
                        percent,
                    },
                );
            }
        })?;

        ensure_not_cancelled()?;
        if !status.success() {
            let mut message = format!("Download failed: {original_url}");
            if is_google_drive_url(&url) {
                message.push_str(
                    ". If this is a private Drive file, set browser cookies in Settings.",
                );
            }
            return Err(message);
        }
    }

    Ok(())
}

fn transcribe_item(
    app: &AppHandle,
    client: &GeminiClient,
    settings: &AppSettings,
    output_dir: &Path,
    work_dir: &Path,
    chunk_seconds: u32,
    item: &MediaItem,
    run_state: &mut RunState,
) -> Result<(), String> {
    ensure_not_cancelled()?;
    let target = final_path(output_dir, item);
    if target.exists() && !settings.force {
        emit_line(
            app,
            format!(
                "[~] Skipping {:02}: {} (already complete)",
                item.number, item.title
            ),
        );
        run_state.completed_items += 1;
        emit_progress(
            app,
            EngineProgress {
                phase: "Transcribing".to_string(),
                message: format!("Skipped existing transcript: {}", item.title),
                status: "Done".to_string(),
                current_item: Some(item.number),
                total_items: run_state.total_items,
                completed_items: run_state.completed_items,
                chunk_current: 0,
                chunk_total: 0,
                download_speed: "0 KB/s".to_string(),
                percent: progress_percent(run_state.completed_items, run_state.total_items),
            },
        );
        return Ok(());
    }

    let work_key = safe_filename(&format!(
        "{:02}_{}_{}s_{}",
        item.number, item.item_id, chunk_seconds, CHUNK_CACHE_VERSION
    ));
    let item_chunk_dir = work_dir.join("audio_chunks").join(&work_key);
    let item_text_dir = work_dir.join("chunk_text").join(&work_key);
    fs::create_dir_all(&item_text_dir)
        .map_err(|error| format!("Failed to create chunk text folder: {error}"))?;

    let chunks = split_audio(&item.media_path, &item_chunk_dir, chunk_seconds, settings)?;
    if chunks.is_empty() {
        return Err(format!(
            "No chunks were created for: {}",
            item.media_path.display()
        ));
    }

    emit_line(
        app,
        format!(
            "[+] Transcribing {:02}: {} ({} chunks)",
            item.number,
            item.title,
            chunks.len()
        ),
    );
    emit_progress(
        app,
        EngineProgress {
            phase: "Transcribing".to_string(),
            message: item.title.clone(),
            status: "Running".to_string(),
            current_item: Some(item.number),
            total_items: run_state.total_items,
            completed_items: run_state.completed_items,
            chunk_current: 0,
            chunk_total: chunks.len(),
            download_speed: "0 KB/s".to_string(),
            percent: progress_percent(run_state.completed_items, run_state.total_items),
        },
    );

    let mut chunk_texts = Vec::new();
    for (chunk_index, chunk_path) in chunks.iter().enumerate() {
        ensure_not_cancelled()?;
        let chunk_number = chunk_index + 1;
        let offset = chunk_index as u32 * chunk_seconds;
        let chunk_text_path = item_text_dir.join(format!("chunk_{chunk_number:03}.txt"));
        emit_line(
            app,
            format!(
                "    chunk {}/{} @ {}",
                chunk_number,
                chunks.len(),
                seconds_to_stamp(offset)
            ),
        );
        emit_progress(
            app,
            EngineProgress {
                phase: "Transcribing".to_string(),
                message: item.title.clone(),
                status: "Running".to_string(),
                current_item: Some(item.number),
                total_items: run_state.total_items,
                completed_items: run_state.completed_items,
                chunk_current: chunk_number,
                chunk_total: chunks.len(),
                download_speed: "0 KB/s".to_string(),
                percent: progress_percent(run_state.completed_items, run_state.total_items),
            },
        );

        let text = transcribe_chunk(
            app,
            client,
            settings,
            chunk_path,
            &chunk_text_path,
            &item.title,
            chunk_number,
            chunks.len(),
            offset,
        )?;
        ensure_not_cancelled()?;
        chunk_texts.push((chunk_number, offset, text.trim().to_string()));
    }

    write_transcript(output_dir, item, &chunk_texts)?;
    run_state.completed_items += 1;
    emit_line(app, format!("[OK] Saved transcript: {}", target.display()));
    emit_progress(
        app,
        EngineProgress {
            phase: "Transcribing".to_string(),
            message: format!("Saved transcript: {}", item.title),
            status: "Done".to_string(),
            current_item: Some(item.number),
            total_items: run_state.total_items,
            completed_items: run_state.completed_items,
            chunk_current: chunks.len(),
            chunk_total: chunks.len(),
            download_speed: "0 KB/s".to_string(),
            percent: progress_percent(run_state.completed_items, run_state.total_items),
        },
    );

    Ok(())
}

fn transcribe_chunk(
    app: &AppHandle,
    client: &GeminiClient,
    settings: &AppSettings,
    chunk_path: &Path,
    chunk_text_path: &Path,
    title: &str,
    chunk_number: usize,
    chunk_count: usize,
    offset_seconds: u32,
) -> Result<String, String> {
    ensure_not_cancelled()?;
    if chunk_text_path.exists() && !settings.force {
        return fs::read_to_string(chunk_text_path)
            .map_err(|error| format!("Failed to read cached chunk text: {error}"));
    }

    let prompt = build_prompt(
        title,
        chunk_number,
        chunk_count,
        offset_seconds,
        &settings.prompt_preset,
    );
    let mut last_text = String::new();
    for attempt in 1..=2 {
        ensure_not_cancelled()?;
        let response = client.generate_audio_transcript(chunk_path, &prompt)?;
        let report = repetition_report(&response);
        let (text, trimmed) = trim_looping_text(&response);
        let trimmed_report = repetition_report(&text);
        last_text = text.clone();

        if !text.trim().is_empty() && !looks_repetitive(&text) {
            fs::write(chunk_text_path, format!("{}\n", text.trim()))
                .map_err(|error| format!("Failed to write chunk transcript: {error}"))?;
            if trimmed {
                emit_line(
                    app,
                    format!(
                        "[~] Trimmed model loop: before words={} max_run={} max_5gram={}; after words={}",
                        report.words, report.max_run, report.max_5gram, trimmed_report.words
                    ),
                );
            }
            if settings.request_delay_seconds > 0.0 {
                thread::sleep(Duration::from_secs_f64(settings.request_delay_seconds));
            }
            ensure_not_cancelled()?;
            return Ok(text);
        }

        emit_line(
            app,
            format!(
                "[!] Repetitive output attempt {attempt}: words={} max_run={} max_5gram={}",
                report.words, report.max_run, report.max_5gram
            ),
        );
    }

    let bad_path = chunk_text_path.with_extension("rejected.txt");
    let _ = fs::write(&bad_path, format!("{}\n", last_text));
    Err(format!(
        "Chunk still looked repetitive after retry: {}",
        chunk_path.display()
    ))
}

impl GeminiClient {
    fn new(api_key: String, model: String) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|error| format!("Failed to create Gemini HTTP client: {error}"))?;
        Ok(Self {
            api_key,
            model: normalize_model(&model),
            http,
        })
    }

    fn generate_audio_transcript(&self, chunk_path: &Path, prompt: &str) -> Result<String, String> {
        let audio_len = fs::metadata(chunk_path)
            .map_err(|error| format!("Failed to inspect audio chunk: {error}"))?
            .len();
        let mime_type = mime_for_audio_path(chunk_path);
        let audio_part = if audio_len <= INLINE_AUDIO_LIMIT_BYTES {
            let bytes = fs::read(chunk_path)
                .map_err(|error| format!("Failed to read audio chunk: {error}"))?;
            json!({
                "inline_data": {
                    "mime_type": mime_type,
                    "data": BASE64.encode(bytes),
                }
            })
        } else {
            let file = self.upload_file(chunk_path, &mime_type)?;
            json!({
                "file_data": {
                    "mime_type": file.mime_type,
                    "file_uri": file.uri,
                }
            })
        };

        let payload = json!({
            "contents": [{
                "parts": [
                    { "text": prompt },
                    audio_part
                ]
            }],
            "generationConfig": {
                "temperature": 0.0,
                "candidateCount": 1
            }
        });

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/{}:generateContent",
            self.model
        );
        let value = self.post_json_with_retries(&url, payload)?;
        extract_text_response(&value)
    }

    fn post_json_with_retries(&self, url: &str, payload: Value) -> Result<Value, String> {
        let mut last_error = String::new();
        for attempt in 1..=3 {
            let response = self
                .http
                .post(url)
                .header("x-goog-api-key", &self.api_key)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send();

            match response {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().unwrap_or_default();
                    if status.is_success() {
                        return serde_json::from_str(&body)
                            .map_err(|error| format!("Gemini returned invalid JSON: {error}"));
                    }
                    last_error = format!("Gemini request failed ({status}): {body}");
                    if !is_retryable_gemini_error(&last_error) || attempt == 3 {
                        return Err(friendly_gemini_error(&last_error));
                    }
                }
                Err(error) => {
                    last_error = format!("Gemini request failed: {error}");
                    if attempt == 3 {
                        return Err(last_error);
                    }
                }
            }

            thread::sleep(Duration::from_secs(retry_delay_seconds(&last_error)));
        }

        Err(last_error)
    }

    fn upload_file(&self, path: &Path, mime_type: &str) -> Result<UploadedFile, String> {
        let bytes =
            fs::read(path).map_err(|error| format!("Failed to read upload file: {error}"))?;
        let display_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("audio-chunk")
            .to_string();
        let start_response = self
            .http
            .post("https://generativelanguage.googleapis.com/upload/v1beta/files")
            .header("x-goog-api-key", &self.api_key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header(
                "X-Goog-Upload-Header-Content-Length",
                bytes.len().to_string(),
            )
            .header("X-Goog-Upload-Header-Content-Type", mime_type)
            .json(&json!({ "file": { "display_name": display_name } }))
            .send()
            .map_err(|error| format!("Failed to start Gemini file upload: {error}"))?;

        if !start_response.status().is_success() {
            let status = start_response.status();
            let body = start_response.text().unwrap_or_default();
            return Err(format!(
                "Failed to start Gemini file upload ({status}): {body}"
            ));
        }

        let upload_url = start_response
            .headers()
            .get("x-goog-upload-url")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| "Gemini did not return an upload URL.".to_string())?
            .to_string();

        let upload_response = self
            .http
            .post(upload_url)
            .header("Content-Length", bytes.len().to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(bytes)
            .send()
            .map_err(|error| format!("Failed to upload audio file to Gemini: {error}"))?;

        let status = upload_response.status();
        let body = upload_response.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("Gemini file upload failed ({status}): {body}"));
        }

        let value: Value = serde_json::from_str(&body)
            .map_err(|error| format!("Gemini upload returned invalid JSON: {error}"))?;
        let file = &value["file"];
        let uri = file["uri"]
            .as_str()
            .ok_or_else(|| "Gemini upload response did not include file.uri.".to_string())?;
        let mime = file["mimeType"]
            .as_str()
            .or_else(|| file["mime_type"].as_str())
            .unwrap_or(mime_type);
        Ok(UploadedFile {
            uri: uri.to_string(),
            mime_type: mime.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
struct UploadedFile {
    uri: String,
    mime_type: String,
}

fn build_preview_queue(
    root: &Path,
    settings: &AppSettings,
    sources: &SourceBundle,
) -> Result<Vec<QueueItem>, String> {
    let media_index = collect_downloaded_media(root, Path::new(&settings.download_dir));
    let media_by_id = media_index_by_id(&media_index);
    let mut items = Vec::new();
    let mut seen_items = HashSet::new();

    for url in &sources.urls {
        let normalized = normalize_source_url(url);
        if !seen_items.insert(format!("url:{}", normalized.to_lowercase())) {
            continue;
        }
        let item_id = extract_source_id(&normalized).unwrap_or_else(|| stable_id(&normalized));
        let media_key = item_id.to_lowercase();
        let media_path = media_by_id.get(&media_key).cloned();
        let title = media_path
            .as_deref()
            .map(title_from_media_path)
            .unwrap_or_else(|| title_from_url(&normalized));
        let status = if media_path.is_some() {
            "Ready"
        } else {
            "Will download"
        };
        let number = items.len() + 1;
        let transcript_path = preview_transcript_path(
            Path::new(&settings.output_dir),
            number,
            &title,
            &normalized,
            media_path.as_deref(),
            &item_id,
        );
        items.push(QueueItem {
            id: format!("url:{item_id}"),
            number,
            source_type: "link".to_string(),
            title,
            source: normalized.clone(),
            url: normalized,
            media_path: media_path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            thumbnail_path: String::new(),
            markdown_path: markdown_path_from_transcript(&transcript_path),
            downloaded_media_path: String::new(),
            transcript_path,
            estimated_chunks: 0,
            duplicate_of: None,
            selected: true,
            status: status.to_string(),
            error: None,
            fix_action: if status == "Will download" {
                Some("download".to_string())
            } else {
                None
            },
        });
    }

    for path in &sources.media_sources {
        if !seen_items.insert(format!("media:{}", path_key(path))) {
            continue;
        }
        let item_id = extract_bracket_id(path).unwrap_or_else(|| stable_id(&path_key(path)));
        let title = title_from_media_path(path);
        let number = items.len() + 1;
        let transcript_path = preview_transcript_path(
            Path::new(&settings.output_dir),
            number,
            &title,
            &path.to_string_lossy(),
            Some(path),
            &item_id,
        );
        items.push(QueueItem {
            id: format!("media:{item_id}"),
            number,
            source_type: "media".to_string(),
            title,
            source: path.to_string_lossy().to_string(),
            url: String::new(),
            media_path: path.to_string_lossy().to_string(),
            thumbnail_path: String::new(),
            markdown_path: markdown_path_from_transcript(&transcript_path),
            downloaded_media_path: path.to_string_lossy().to_string(),
            transcript_path,
            estimated_chunks: 0,
            duplicate_of: None,
            selected: true,
            status: "Ready".to_string(),
            error: None,
            fix_action: None,
        });
    }

    Ok(items)
}

fn preview_transcript_path(
    output_dir: &Path,
    number: usize,
    title: &str,
    source: &str,
    media_path: Option<&Path>,
    item_id: &str,
) -> String {
    let item = MediaItem {
        number,
        title: title.to_string(),
        source: source.to_string(),
        media_path: media_path.unwrap_or_else(|| Path::new("")).to_path_buf(),
        item_id: item_id.to_string(),
    };
    final_path(output_dir, &item).to_string_lossy().to_string()
}

fn build_media_items(
    root: &Path,
    settings: &AppSettings,
    sources: &SourceBundle,
) -> Result<Vec<MediaItem>, String> {
    let downloaded_media = collect_downloaded_media(root, Path::new(&settings.download_dir));
    let media_by_id = media_index_by_id(&downloaded_media);
    let mut items = Vec::new();
    let mut used_paths = HashSet::new();
    let mut missing_urls = Vec::new();

    for url in &sources.urls {
        let normalized = normalize_source_url(url);
        let item_id = extract_source_id(&normalized).unwrap_or_else(|| stable_id(&normalized));
        let media_path = media_by_id.get(&item_id.to_lowercase()).cloned();
        let Some(media_path) = media_path else {
            missing_urls.push(normalized);
            continue;
        };
        if !used_paths.insert(path_key(&media_path)) {
            continue;
        }
        items.push(MediaItem {
            number: items.len() + 1,
            title: title_from_media_path(&media_path),
            source: url.clone(),
            media_path,
            item_id,
        });
    }

    for media_path in &sources.media_sources {
        if !media_path.exists() {
            return Err(format!("Media file not found: {}", media_path.display()));
        }
        if !used_paths.insert(path_key(media_path)) {
            continue;
        }
        items.push(MediaItem {
            number: items.len() + 1,
            title: title_from_media_path(media_path),
            source: media_path.to_string_lossy().to_string(),
            media_path: media_path.clone(),
            item_id: extract_bracket_id(media_path)
                .unwrap_or_else(|| stable_id(&path_key(media_path))),
        });
    }

    if items.is_empty() && !missing_urls.is_empty() {
        return Err(format!(
            "No downloaded media matched {} link(s). Check that the links are public or add browser cookies in Settings.",
            missing_urls.len()
        ));
    }

    Ok(items)
}

fn resolve_sources(
    root: &Path,
    inputs: &[String],
    use_defaults: bool,
) -> Result<SourceBundle, String> {
    let mut raw_inputs = inputs
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    if raw_inputs.is_empty() && use_defaults {
        for candidate in ["Drive links.txt", "links.txt"] {
            let path = root.join(candidate);
            if path.exists() {
                raw_inputs.push(path.to_string_lossy().to_string());
                break;
            }
        }
    }

    let mut urls = Vec::new();
    let mut media_sources = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut seen_paths = HashSet::new();

    for raw in raw_inputs {
        let path = PathBuf::from(&raw);
        if path.exists() {
            if path.is_dir() {
                let mut media_files = Vec::new();
                collect_media_files(&path, &mut media_files);
                for media in media_files {
                    push_media_source(&mut media_sources, &mut seen_paths, media);
                }
            } else if is_text_file(&path) {
                let text = fs::read_to_string(&path)
                    .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
                for url in extract_urls(&text) {
                    push_url_source(&mut urls, &mut seen_urls, url);
                }
            } else if is_media_file(&path) {
                push_media_source(&mut media_sources, &mut seen_paths, path);
            }
            continue;
        }

        for url in extract_urls(&raw) {
            push_url_source(&mut urls, &mut seen_urls, url);
        }
    }

    Ok(SourceBundle {
        urls,
        media_sources,
    })
}

fn push_url_source(urls: &mut Vec<String>, seen_urls: &mut HashSet<String>, url: String) {
    let normalized = normalize_source_url(&url);
    if seen_urls.insert(normalized.to_lowercase()) {
        urls.push(normalized);
    }
}

fn push_media_source(
    media_sources: &mut Vec<PathBuf>,
    seen_paths: &mut HashSet<String>,
    path: PathBuf,
) {
    let key = path_key(&path);
    if seen_paths.insert(key) {
        media_sources.push(path);
    }
}

fn collect_downloaded_media(root: &Path, configured_download_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![
        configured_download_dir.to_path_buf(),
        root.join("downloads"),
        root.join("Drive links downloads"),
    ];
    folders.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    folders.dedup_by(|a, b| path_key(a) == path_key(b));
    for folder in folders {
        if folder.exists() {
            collect_media_files(&folder, &mut files);
        }
    }
    files.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    files.dedup_by(|a, b| path_key(a) == path_key(b));
    files
}

fn collect_media_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if should_skip_media_path(&entry_path) {
            continue;
        }
        if entry_path.is_dir() {
            collect_media_files(&entry_path, files);
        } else if is_media_file(&entry_path) {
            files.push(entry_path);
        }
    }
}

fn should_skip_media_path(path: &Path) -> bool {
    path.components().any(|component| {
        let part = component.as_os_str().to_string_lossy().to_lowercase();
        SKIP_MEDIA_FOLDER_NAMES.iter().any(|skip| *skip == part)
    })
}

fn media_index_by_id(paths: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    for path in paths {
        if let Some(id) = extract_bracket_id(path) {
            map.entry(id.to_lowercase()).or_insert_with(|| path.clone());
        }
    }
    map
}

fn split_audio(
    media_path: &Path,
    item_chunk_dir: &Path,
    chunk_seconds: u32,
    settings: &AppSettings,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(item_chunk_dir)
        .map_err(|error| format!("Failed to create audio chunk folder: {error}"))?;
    let existing = sorted_chunk_files(item_chunk_dir);
    if !existing.is_empty() {
        return Ok(existing);
    }

    let pattern = item_chunk_dir.join("chunk_%03d.mp3");
    let mut command = Command::new(ffmpeg_command(settings));
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(media_path)
        .arg("-f")
        .arg("segment")
        .arg("-segment_time")
        .arg(chunk_seconds.to_string())
        .arg("-reset_timestamps")
        .arg("1")
        .arg("-map")
        .arg("0:a:0")
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("64k")
        .arg(pattern);

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .map_err(|error| format!("Failed to run FFmpeg: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "FFmpeg failed for {}: {}",
            media_path.display(),
            stderr.trim()
        ));
    }

    Ok(sorted_chunk_files(item_chunk_dir))
}

fn sorted_chunk_files(item_chunk_dir: &Path) -> Vec<PathBuf> {
    let mut chunks = fs::read_dir(item_chunk_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| {
                    name.starts_with("chunk_")
                        && path
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("mp3"))
                            .unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    chunks.sort();
    chunks
}

fn write_transcript(
    output_dir: &Path,
    item: &MediaItem,
    chunk_texts: &[(usize, u32, String)],
) -> Result<(), String> {
    fs::create_dir_all(output_dir)
        .map_err(|error| format!("Failed to create output folder: {error}"))?;
    let mut markdown = vec![
        format!("# {:02} - {}", item.number, item.title),
        String::new(),
        format!("Source: {}", item.source),
        format!(
            "Media: {}",
            item.media_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
        ),
        String::new(),
    ];
    let mut plain = vec![
        format!("{:02} - {}", item.number, item.title),
        format!("Source: {}", item.source),
        format!(
            "Media: {}",
            item.media_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
        ),
        String::new(),
    ];

    for (chunk_index, offset, text) in chunk_texts {
        let stamp = seconds_to_stamp(*offset);
        markdown.extend([
            format!("## Chunk {:02} [{}]", chunk_index, stamp),
            String::new(),
            text.trim().to_string(),
            String::new(),
        ]);
        plain.extend([
            format!("[{}]", stamp),
            text.trim().to_string(),
            String::new(),
        ]);
    }

    let target = final_path(output_dir, item);
    fs::write(&target, format!("{}\n", plain.join("\n").trim()))
        .map_err(|error| format!("Failed to write transcript {}: {error}", target.display()))?;
    let markdown_target = markdown_path(output_dir, item);
    fs::write(
        &markdown_target,
        format!("{}\n", markdown.join("\n").trim()),
    )
    .map_err(|error| {
        format!(
            "Failed to write Markdown transcript {}: {error}",
            markdown_target.display()
        )
    })?;
    Ok(())
}

fn write_index(output_dir: &Path, items: &[MediaItem]) -> Result<(), String> {
    fs::create_dir_all(output_dir)
        .map_err(|error| format!("Failed to create output folder: {error}"))?;
    let mut lines = vec![
        "# Transcripts".to_string(),
        String::new(),
        "| # | Status | Title | TXT | Markdown | Source |".to_string(),
        "|---:|---|---|---|---|---|".to_string(),
    ];

    for item in items {
        let target = final_path(output_dir, item);
        let md_target = markdown_path(output_dir, item);
        let status = if target.exists() { "done" } else { "pending" };
        let transcript = if target.exists() {
            format!(
                "`{}`",
                target
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
            )
        } else {
            String::new()
        };
        let markdown = if md_target.exists() {
            format!(
                "`{}`",
                md_target
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
            )
        } else {
            String::new()
        };
        let source = item.source.replace('|', "\\|");
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            item.number, status, item.title, transcript, markdown, source
        ));
    }

    fs::write(
        output_dir.join("00_index.md"),
        format!("{}\n", lines.join("\n")),
    )
    .map_err(|error| format!("Failed to write transcript index: {error}"))
}

fn final_path(output_dir: &Path, item: &MediaItem) -> PathBuf {
    output_dir.join(format!(
        "{:02} - {} [{}].txt",
        item.number,
        safe_filename(&item.title),
        safe_filename(&item.item_id)
    ))
}

fn markdown_path(output_dir: &Path, item: &MediaItem) -> PathBuf {
    output_dir.join(format!(
        "{:02} - {} [{}].md",
        item.number,
        safe_filename(&item.title),
        safe_filename(&item.item_id)
    ))
}

fn markdown_path_from_transcript(path: &str) -> String {
    let mut target = PathBuf::from(path);
    target.set_extension("md");
    target.to_string_lossy().to_string()
}

fn build_prompt(
    title: &str,
    chunk_number: usize,
    chunk_count: usize,
    offset_seconds: u32,
    prompt_preset: &str,
) -> String {
    let offset = seconds_to_stamp(offset_seconds);
    let preset_guidance = match prompt_preset {
        "arabic_lecture" => {
            "Preset: Arabic lecture. Prefer clear formal Arabic for Arabic speech while keeping spoken English technical terms in English."
        }
        "english_lecture" => {
            "Preset: English lecture. Use clear academic English and preserve technical terms exactly as spoken."
        }
        "technical_math" => {
            "Preset: Technical/math lecture. Preserve formulas, variables, units, code terms, and step-by-step reasoning exactly."
        }
        _ => "Preset: Default lecture transcription.",
    };
    format!(
        r#"You are transcribing one short audio chunk from a lecture, class recording, or educational video.

Lecture title: {title}
Chunk: {chunk_number} of {chunk_count}
Chunk start time in the original lecture: {offset}
{preset_guidance}

Return only the transcript for this chunk.

Strict rules:
1. Preserve the spoken language. If the speaker uses Arabic, write it in clear formal Arabic.
2. Keep English technical terms in English when they are spoken or commonly used that way.
3. If the speaker switches languages, preserve the switch naturally.
4. Write mathematical symbols and equations in standard English notation.
5. Put an accurate timestamp at the start of each short paragraph.
6. Add the chunk start offset to timestamps so they refer to the original lecture time.
7. Do not summarize.
8. Do not repeat any sentence, phrase, number, or word unless it is clearly spoken again.
9. If audio is unclear or silent, write [unclear] once and continue only when speech resumes.
10. Stop when this chunk ends."#
    )
}

#[derive(Debug, Clone, Copy)]
struct RepetitionReport {
    words: usize,
    max_run: usize,
    max_5gram: usize,
}

fn repetition_report(text: &str) -> RepetitionReport {
    let words = words_for_repetition(text);
    if words.is_empty() {
        return RepetitionReport {
            words: 0,
            max_run: 0,
            max_5gram: 0,
        };
    }

    let mut max_run = 1;
    let mut current_run = 1;
    for pair in words.windows(2) {
        if pair[0] == pair[1] {
            current_run += 1;
            max_run = max_run.max(current_run);
        } else {
            current_run = 1;
        }
    }

    let mut fivegram_counts: HashMap<Vec<String>, usize> = HashMap::new();
    for gram in words.windows(5) {
        *fivegram_counts.entry(gram.to_vec()).or_default() += 1;
    }

    RepetitionReport {
        words: words.len(),
        max_run,
        max_5gram: fivegram_counts.values().copied().max().unwrap_or(0),
    }
}

fn looks_repetitive(text: &str) -> bool {
    let report = repetition_report(text);
    report.max_run >= 25 || report.max_5gram >= 25
}

fn trim_looping_text(text: &str) -> (String, bool) {
    let matches = word_matches(text);
    if matches.len() < 30 {
        return (text.to_string(), false);
    }
    let words = matches
        .iter()
        .map(|(_, _, word)| word.clone())
        .collect::<Vec<_>>();

    let mut run_start = 0;
    let mut run_length = 1;
    for index in 1..words.len() {
        if words[index] == words[index - 1] {
            run_length += 1;
            if run_length >= 10 {
                let cut_at = matches[run_start].0;
                return (
                    format!(
                        "{}\n\n[TRANSCRIPTION_STOPPED_MODEL_REPETITION]",
                        text[..cut_at].trim_end()
                    ),
                    true,
                );
            }
        } else {
            run_start = index;
            run_length = 1;
        }
    }

    let mut occurrences: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for index in 0..words.len().saturating_sub(4) {
        let gram = words[index..index + 5].to_vec();
        let seen = occurrences.entry(gram).or_default();
        seen.push(index);
        if seen.len() >= 8 {
            let cut_at = matches[seen[2]].0;
            return (
                format!(
                    "{}\n\n[TRANSCRIPTION_STOPPED_MODEL_REPETITION]",
                    text[..cut_at].trim_end()
                ),
                true,
            );
        }
    }

    (text.to_string(), false)
}

fn words_for_repetition(text: &str) -> Vec<String> {
    word_matches(text)
        .into_iter()
        .map(|(_, _, word)| word)
        .collect()
}

fn word_matches(text: &str) -> Vec<(usize, usize, String)> {
    let mut matches = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_alphanumeric() || ch == '_' {
            start.get_or_insert(index);
        } else if let Some(start_index) = start.take() {
            matches.push((start_index, index, text[start_index..index].to_lowercase()));
        }
    }
    if let Some(start_index) = start {
        matches.push((start_index, text.len(), text[start_index..].to_lowercase()));
    }
    matches
}

fn extract_text_response(value: &Value) -> Result<String, String> {
    let mut parts = Vec::new();
    if let Some(candidates) = value["candidates"].as_array() {
        for candidate in candidates {
            if let Some(content_parts) = candidate["content"]["parts"].as_array() {
                for part in content_parts {
                    if let Some(text) = part["text"].as_str() {
                        parts.push(text.to_string());
                    }
                }
            }
        }
    }
    let text = parts.join("\n").trim().to_string();
    if text.is_empty() {
        return Err(format!("Gemini returned no transcript text: {value}"));
    }
    Ok(text)
}

fn is_retryable_gemini_error(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("429")
        || lowered.contains("resource_exhausted")
        || lowered.contains("temporarily")
        || lowered.contains("unavailable")
        || lowered.contains("retry")
}

fn retry_delay_seconds(message: &str) -> u64 {
    let retry_delay = Regex::new(r#"retryDelay["']?\s*:\s*["']?(\d+)s"#).ok();
    if let Some(regex) = retry_delay {
        if let Some(captures) = regex.captures(message) {
            if let Some(value) = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok())
            {
                return (value + 5).min(180);
            }
        }
    }

    let please_retry = Regex::new(r#"retry in ([\d.]+)s"#).ok();
    if let Some(regex) = please_retry {
        if let Some(captures) = regex.captures(message) {
            if let Some(value) = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<f64>().ok())
            {
                return (value.ceil() as u64 + 5).min(180);
            }
        }
    }

    30
}

fn friendly_gemini_error(message: &str) -> String {
    let lowered = message.to_lowercase();
    if lowered.contains("api_key_invalid") || lowered.contains("api key not valid") {
        return "Gemini API key was rejected. Open Settings and save a valid key from AI Studio."
            .to_string();
    }
    if lowered.contains("quota")
        || lowered.contains("resource_exhausted")
        || lowered.contains("429")
    {
        return "Gemini quota or rate limit was reached. Wait for the limit to reset, then run again; completed chunks stay cached.".to_string();
    }
    message.to_string()
}

fn normalize_model(model: &str) -> String {
    let model = model.trim();
    if model.is_empty() {
        format!("models/{DEFAULT_MODEL}")
    } else if model.starts_with("models/") {
        model.to_string()
    } else {
        format!("models/{model}")
    }
}

fn mime_for_audio_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        _ => "audio/mp3",
    }
    .to_string()
}

fn parse_download_progress(line: &str) -> Option<(f64, String)> {
    let regex = Regex::new(r#"\[download\]\s+([0-9.]+)%.*?at\s+([^\s]+/s)"#).ok()?;
    let captures = regex.captures(line)?;
    let percent = captures.get(1)?.as_str().parse::<f64>().ok()?;
    let speed = captures.get(2)?.as_str().to_string();
    Some((percent, speed))
}

fn stream_command<F>(
    mut command: Command,
    mut on_line: F,
) -> Result<std::process::ExitStatus, String>
where
    F: FnMut(&str),
{
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start command: {error}"))?;
    let (tx, rx) = mpsc::channel::<String>();

    if let Some(stdout) = child.stdout.take() {
        spawn_line_reader(stdout, tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_line_reader(stderr, tx.clone());
    }
    drop(tx);

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => on_line(&line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return child
                    .wait()
                    .map_err(|error| format!("Failed while waiting for command: {error}"));
            }
        }

        if cancel_requested() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CANCELLED_MESSAGE.to_string());
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed while checking command status: {error}"))?
        {
            while let Ok(line) = rx.try_recv() {
                on_line(&line);
            }
            return Ok(status);
        }
    }
}

fn spawn_line_reader<R: Read + Send + 'static>(reader: R, tx: mpsc::Sender<String>) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
}

fn emit_line(app: &AppHandle, line: impl AsRef<str>) {
    let _ = app.emit("engine-line", line.as_ref().to_string());
}

fn emit_progress(app: &AppHandle, progress: EngineProgress) {
    let _ = app.emit("engine-progress", progress);
}

fn emit_done(app: &AppHandle, success: bool, code: Option<i32>) {
    let _ = app.emit("engine-done", EngineDone { code, success });
}

fn cancel_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::SeqCst)
}

fn ensure_not_cancelled() -> Result<(), String> {
    if cancel_requested() {
        Err(CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

fn is_cancelled_error(message: &str) -> bool {
    message.contains(CANCELLED_MESSAGE)
}

fn progress_percent(done: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        ((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
    }
}

fn extract_urls(text: &str) -> Vec<String> {
    let regex = Regex::new(r#"https?://[^\s<>'"]+"#).expect("valid URL regex");
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    for found in regex.find_iter(text) {
        let cleaned = found
            .as_str()
            .trim()
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\''
                )
            })
            .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ')' | ']' | '}'))
            .to_string();
        if !cleaned.is_empty() && seen.insert(cleaned.clone()) {
            urls.push(cleaned);
        }
    }
    urls
}

fn is_google_drive_url(url: &str) -> bool {
    let lowered = url.to_lowercase();
    lowered.contains("drive.google.com") || lowered.contains("docs.google.com")
}

fn normalize_source_url(url: &str) -> String {
    normalize_google_drive_url(url).unwrap_or_else(|| url.trim().to_string())
}

fn normalize_google_drive_url(url: &str) -> Option<String> {
    if !is_google_drive_url(url) {
        return None;
    }
    extract_drive_file_id(url)
        .map(|file_id| format!("https://drive.google.com/file/d/{file_id}/view"))
}

fn extract_source_id(url: &str) -> Option<String> {
    extract_drive_file_id(url).or_else(|| extract_youtube_id(url))
}

fn extract_drive_file_id(url: &str) -> Option<String> {
    if let Some(index) = url.find("/file/d/") {
        let rest = &url[index + "/file/d/".len()..];
        let id = rest.split(['/', '?', '&', '#']).next().unwrap_or_default();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    if let Some(index) = url.find("id=") {
        let rest = &url[index + 3..];
        let id = rest.split(['&', '#']).next().unwrap_or_default();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    None
}

fn extract_youtube_id(url: &str) -> Option<String> {
    let lowered = url.to_lowercase();
    if let Some(index) = lowered.find("youtu.be/") {
        let rest = &url[index + "youtu.be/".len()..];
        let id = rest.split(['?', '&', '/', '#']).next().unwrap_or_default();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    if let Some(index) = lowered.find("v=") {
        let rest = &url[index + 2..];
        let id = rest.split(['&', '#']).next().unwrap_or_default();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

fn extract_bracket_id(path: &Path) -> Option<String> {
    let stem = path.file_stem().and_then(|value| value.to_str())?;
    let regex = Regex::new(r#"\[([^\]]+)\]"#).ok()?;
    regex
        .captures_iter(stem)
        .last()
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn title_from_url(url: &str) -> String {
    if let Some(file_id) = extract_drive_file_id(url) {
        return format!("Drive file {}", shorten(&file_id, 10));
    }
    if let Some(video_id) = extract_youtube_id(url) {
        return format!("YouTube video {}", shorten(&video_id, 10));
    }
    "URL item".to_string()
}

fn title_from_media_path(path: &Path) -> String {
    let mut name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled")
        .to_string();

    if let Some(index) = name.find(" [") {
        name.truncate(index);
    }
    if let Some(index) = name.to_lowercase().find(".mp4") {
        name.truncate(index);
    }

    let title = name.replace('_', " ");
    collapse_whitespace(&title)
}

fn safe_filename(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    let cleaned = collapse_whitespace(&cleaned)
        .trim_matches([' ', '.'])
        .to_string();
    if cleaned.is_empty() {
        "Untitled".to_string()
    } else {
        cleaned
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn seconds_to_stamp(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn shorten(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}...", value.chars().take(max).collect::<String>())
    }
}

fn stable_id(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("local-{:016x}", hasher.finish())
}

fn path_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

fn is_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("txt"))
        .unwrap_or(false)
}

fn is_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|ext| {
            MEDIA_EXTENSIONS
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or(false)
}

fn check_yt_dlp(root: &Path, settings: &AppSettings) -> ToolStatus {
    match yt_dlp_command(root, settings) {
        Ok(path) => ToolStatus {
            name: "Downloader".to_string(),
            ok: true,
            detail: tool_detail(&path, "yt-dlp"),
        },
        Err(error) => ToolStatus {
            name: "Downloader".to_string(),
            ok: false,
            detail: error,
        },
    }
}

fn yt_dlp_command(root: &Path, settings: &AppSettings) -> Result<PathBuf, String> {
    if let Some(path) = valid_custom_tool(&settings.downloader_path) {
        return Ok(path);
    }

    let app_tool = tools_root().join(if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    if app_tool.exists() {
        return Ok(app_tool);
    }

    if let Some(bundled) = bundled_downloader(root) {
        if let Some(parent) = app_tool.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(&bundled, &app_tool);
        if app_tool.exists() {
            return Ok(app_tool);
        }
        return Ok(bundled);
    }

    let path = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    if command_exists(path) {
        return Ok(PathBuf::from(path));
    }

    Err(
        "Downloader is missing. Install it from Setup or choose an existing yt-dlp executable."
            .to_string(),
    )
}

fn bundled_downloader(root: &Path) -> Option<PathBuf> {
    let install = install_root();
    [
        root.join("yt-dlp.exe"),
        install.join("yt-dlp.exe"),
        install.join("resources").join("yt-dlp.exe"),
        install.join("resources").join("tools").join("yt-dlp.exe"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn install_or_update_downloader() -> Result<ToolStatus, String> {
    let target = tools_root().join("yt-dlp.exe");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create tools folder: {error}"))?;
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("LectureScribe")
        .build()
        .map_err(|error| format!("Failed to prepare downloader request: {error}"))?;
    let mut response = client
        .get(YT_DLP_WINDOWS_URL)
        .send()
        .map_err(|error| format!("Failed to download yt-dlp.exe: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Downloader download failed with HTTP {}",
            response.status()
        ));
    }

    let temp = target.with_extension("exe.download");
    let mut file = fs::File::create(&temp)
        .map_err(|error| format!("Failed to create download file: {error}"))?;
    response
        .copy_to(&mut file)
        .map_err(|error| format!("Failed to save downloader: {error}"))?;
    file.flush()
        .map_err(|error| format!("Failed to finish downloader download: {error}"))?;
    if target.exists() {
        fs::remove_file(&target)
            .map_err(|error| format!("Failed to replace existing downloader: {error}"))?;
    }
    fs::rename(&temp, &target).map_err(|error| format!("Failed to install downloader: {error}"))?;

    let root = legacy_root();
    let mut settings = load_settings_from_root(&root);
    settings.downloader_path = target.to_string_lossy().to_string();
    let _ = save_settings(settings.clone());
    Ok(check_yt_dlp(&root, &settings))
}

fn check_ffmpeg_with_settings(settings: &AppSettings) -> ToolStatus {
    let command = ffmpeg_command(settings);
    if command_exists_path(&command, "-version") {
        ToolStatus {
            name: "FFmpeg".to_string(),
            ok: true,
            detail: tool_detail(&command, "ffmpeg"),
        }
    } else {
        ToolStatus {
            name: "FFmpeg".to_string(),
            ok: false,
            detail: "Missing. Install FFmpeg or choose ffmpeg.exe in Setup.".to_string(),
        }
    }
}

fn ffmpeg_command(settings: &AppSettings) -> PathBuf {
    valid_custom_tool(&settings.ffmpeg_path).unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn valid_custom_tool(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    path.exists().then_some(path)
}

fn tool_detail(path: &Path, fallback_name: &str) -> String {
    let version = Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            let text = if output.stdout.is_empty() {
                String::from_utf8_lossy(&output.stderr).to_string()
            } else {
                String::from_utf8_lossy(&output.stdout).to_string()
            };
            text.lines().next().map(|line| line.trim().to_string())
        });
    let location = path.to_string_lossy();
    match version {
        Some(version) if !version.is_empty() => format!("{version} - {location}"),
        _ => format!("{fallback_name} ready - {location}"),
    }
}

fn command_exists_path(command: &Path, version_arg: &str) -> bool {
    Command::new(command)
        .arg(version_arg)
        .output()
        .map(|output| {
            output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty()
        })
        .unwrap_or(false)
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|output| {
            output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty()
        })
        .unwrap_or(false)
}

fn has_api_key(root: &Path) -> bool {
    api_key_from_env_or_file(root).is_some()
}

fn api_key_from_env_or_file(root: &Path) -> Option<String> {
    if let Some(value) = api_key_from_keyring() {
        return Some(value);
    }

    for key in ["GEMINI_API_KEY", "GOOGLE_API_KEY"] {
        if let Ok(value) = std::env::var(key) {
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if valid_api_key(&value) {
                return Some(value);
            }
        }
    }

    let env_path = root.join(".env");
    let text = fs::read_to_string(env_path).ok()?;
    for raw_line in text.trim_start_matches('\u{feff}').lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        if matches!(key.trim(), "GEMINI_API_KEY" | "GOOGLE_API_KEY") {
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if valid_api_key(&value) {
                return Some(value);
            }
        }
    }
    None
}

fn api_key_from_keyring() -> Option<String> {
    let entry = keyring::Entry::new(API_KEY_SERVICE, API_KEY_USER).ok()?;
    let value = entry.get_password().ok()?;
    valid_api_key(&value).then_some(value)
}

fn valid_api_key(value: &str) -> bool {
    let value = value.trim().trim_matches('"').trim_matches('\'');
    !value.is_empty() && value != "put-your-gemini-api-key-here"
}

fn load_settings_from_root(root: &Path) -> AppSettings {
    let fallback = default_settings(root);
    let Ok(text) = fs::read_to_string(settings_path(root)) else {
        return fallback;
    };
    let Ok(settings) = serde_json::from_str::<AppSettings>(&text) else {
        return fallback;
    };
    sanitize_settings(root, settings)
}

fn default_settings(root: &Path) -> AppSettings {
    AppSettings {
        output_dir: root
            .join("Transcripts")
            .join("organized")
            .to_string_lossy()
            .to_string(),
        download_dir: root.join("downloads").to_string_lossy().to_string(),
        work_dir: root
            .join("Transcripts")
            .join("_work")
            .to_string_lossy()
            .to_string(),
        model: DEFAULT_MODEL.to_string(),
        run_mode: "download_transcribe".to_string(),
        transcript_format: "txt_markdown".to_string(),
        prompt_preset: "default".to_string(),
        ffmpeg_path: String::new(),
        downloader_path: String::new(),
        chunk_minutes: 2,
        request_delay_seconds: 5.0,
        cookies_from_browser: String::new(),
        cookies_file: String::new(),
        skip_download: false,
        force: false,
    }
}

fn sanitize_settings(root: &Path, settings: AppSettings) -> AppSettings {
    let defaults = default_settings(root);
    AppSettings {
        output_dir: clean_path_setting(settings.output_dir, defaults.output_dir),
        download_dir: clean_path_setting(settings.download_dir, defaults.download_dir),
        work_dir: clean_path_setting(settings.work_dir, defaults.work_dir),
        model: if settings.model.trim().is_empty() {
            defaults.model
        } else {
            settings.model.trim().to_string()
        },
        run_mode: normalize_run_mode_value(&settings.run_mode),
        transcript_format: if settings.transcript_format.trim().is_empty() {
            defaults.transcript_format
        } else {
            settings.transcript_format.trim().to_string()
        },
        prompt_preset: if settings.prompt_preset.trim().is_empty() {
            defaults.prompt_preset
        } else {
            settings.prompt_preset.trim().to_string()
        },
        ffmpeg_path: settings.ffmpeg_path.trim().to_string(),
        downloader_path: settings.downloader_path.trim().to_string(),
        chunk_minutes: settings.chunk_minutes.clamp(1, 30),
        request_delay_seconds: settings.request_delay_seconds.clamp(0.0, 120.0),
        cookies_from_browser: settings.cookies_from_browser.trim().to_string(),
        cookies_file: settings.cookies_file.trim().to_string(),
        skip_download: settings.skip_download,
        force: settings.force,
    }
}

fn clean_path_setting(value: String, fallback: String) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value.to_string()
    }
}

fn normalized_run_mode(settings: &AppSettings) -> String {
    normalize_run_mode_value(&settings.run_mode)
}

fn normalize_run_mode_value(value: &str) -> String {
    match value.trim() {
        "download_only" => "download_only".to_string(),
        "transcribe_existing" => "transcribe_existing".to_string(),
        _ => "download_transcribe".to_string(),
    }
}

fn settings_path(root: &Path) -> PathBuf {
    root.join(".lecturescribe-settings.json")
}

fn legacy_root() -> PathBuf {
    if cfg!(debug_assertions) {
        return source_project_root();
    }

    if let Ok(current_dir) = std::env::current_dir() {
        if current_dir.join("Drive links.txt").exists()
            || current_dir.join("links.txt").exists()
            || current_dir.join(".env").exists()
            || current_dir.join("yt-dlp.exe").exists()
        {
            return current_dir;
        }
    }

    user_data_root()
}

fn source_project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn install_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn tools_root() -> PathBuf {
    user_data_root().join("tools")
}

fn user_data_root() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join("LectureScribe");
    }
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        return PathBuf::from(home).join("LectureScribe");
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            preview_inputs,
            check_environment,
            load_settings,
            save_settings,
            save_api_key,
            api_key_ready,
            check_downloader,
            install_downloader,
            update_downloader,
            choose_downloader,
            check_ffmpeg,
            install_ffmpeg,
            choose_ffmpeg,
            count_links_in_file,
            start_transcription,
            cancel_transcription,
            run_setup_test,
            open_output_dir,
            open_output_folder,
            open_transcript,
            reveal_media,
            copy_output_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running LectureScribe");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preview_defaults_to_drive_links_when_present() {
        let drive_links = legacy_root().join("Drive links.txt");
        if !drive_links.exists() {
            return;
        }

        let queue = preview_inputs(Vec::new()).expect("preview should load default Drive links");
        assert_eq!(queue.len(), 22);
    }

    #[test]
    fn count_links_in_drive_file_when_present() {
        let drive_links = legacy_root().join("Drive links.txt");
        if !drive_links.exists() {
            return;
        }

        let count =
            count_links_in_file(drive_links.to_string_lossy().to_string()).expect("count links");
        assert_eq!(count, 22);
    }

    #[test]
    fn extract_urls_dedupes_messy_text() {
        let text = r#"first (https://drive.google.com/file/d/abc/view), second https://youtu.be/video1. again https://youtu.be/video1"#;
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://drive.google.com/file/d/abc/view");
        assert_eq!(urls[1], "https://youtu.be/video1");
    }

    #[test]
    fn local_media_preview_works() {
        let temp = unique_temp_dir("lecturescribe-preview-test");
        fs::create_dir_all(&temp).expect("temp dir");
        let media = temp.join("Sample Lecture.mp3");
        fs::write(&media, b"not real audio").expect("sample file");
        let queue =
            preview_inputs(vec![media.to_string_lossy().to_string()]).expect("preview local media");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].title, "Sample Lecture");
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn repetitive_text_is_trimmed() {
        let repeated = (0..40).map(|_| "same").collect::<Vec<_>>().join(" ");
        assert!(looks_repetitive(&repeated));
        let (trimmed, did_trim) = trim_looping_text(&repeated);
        assert!(did_trim);
        assert!(trimmed.contains("TRANSCRIPTION_STOPPED_MODEL_REPETITION"));
    }

    #[test]
    fn ffmpeg_chunks_mp4_when_available() {
        if !command_exists("ffmpeg") {
            return;
        }

        let temp = unique_temp_dir("lecturescribe-ffmpeg-test");
        fs::create_dir_all(&temp).expect("temp dir");
        let media = temp.join("sample.mp4");
        let status = Command::new("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg("sine=frequency=1000:duration=3")
            .arg("-c:a")
            .arg("aac")
            .arg(&media)
            .status()
            .expect("run ffmpeg sample");
        if !status.success() {
            let _ = fs::remove_dir_all(temp);
            return;
        }

        let settings = default_settings(&legacy_root());
        let chunks =
            split_audio(&media, &temp.join("chunks"), 1, &settings).expect("split mp4 audio");
        assert!(!chunks.is_empty());
        assert!(chunks
            .iter()
            .all(|path| path.extension().and_then(|ext| ext.to_str()) == Some("mp3")));
        let _ = fs::remove_dir_all(temp);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()))
    }
}
