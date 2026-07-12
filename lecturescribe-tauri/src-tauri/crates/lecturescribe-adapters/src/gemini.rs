use crate::credentials::CredentialStore;
use crate::media::SegmentDescriptor;
use crate::models::{
    curated_model_options, normalize_model_id, supported_model_validation, unavailable_model_error,
    GeminiModelMetadata,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use lecturescribe_core::{
    AppError, AppSettings, ErrorCategory, ModelOption, ModelValidation, ProgressKind,
    ProgressMetric, TranscriptSegment,
};
use lecturescribe_engine::{JobControl, ProgressReporter};
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const INLINE_RAW_LIMIT: u64 = 12 * 1024 * 1024;
static MODEL_METADATA_CACHE: OnceLock<Mutex<Option<CachedModelMetadata>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentTranscript {
    pub language: String,
    #[serde(default)]
    pub languages: Vec<String>,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone)]
pub struct GeminiClient {
    credentials: CredentialStore,
    http: Client,
}

impl GeminiClient {
    pub fn new(credentials: CredentialStore) -> Result<Self, AppError> {
        let http = Client::builder()
            .user_agent("LectureScribe/0.2")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(240))
            .build()
            .map_err(network_error)?;
        Ok(Self { credentials, http })
    }

    pub fn list_transcription_models(
        &self,
        custom_model: Option<&str>,
    ) -> Result<Vec<ModelOption>, AppError> {
        let key = self.api_key()?;
        let metadata = self.model_metadata(&key)?;
        curated_model_options(&metadata, custom_model)
    }

    pub fn validate_transcription_model(&self, model: &str) -> Result<ModelValidation, AppError> {
        let model = normalize_model_id(model);
        if model.is_empty() {
            return Err(unavailable_model_error(model.as_str(), false));
        }
        let key = self.api_key()?;
        let metadata = self.model_metadata(&key)?;
        let Some(entry) = metadata.iter().find(|entry| entry.id == model) else {
            return Err(unavailable_model_error(&model, false));
        };
        if !entry.supports_generate_content() {
            return Err(unavailable_model_error(&model, true));
        }
        supported_model_validation(&model)
    }

    fn api_key(&self) -> Result<String, AppError> {
        self.credentials.gemini_key()?.ok_or_else(|| {
            AppError::new(
                "api_key_missing",
                ErrorCategory::Authentication,
                "Add a Gemini API key before using transcription models.",
                "No Gemini credential was present in Windows Credential Manager.",
            )
            .with_action("open_setup_api", "Add API key", "open_setup_api")
        })
    }

    fn model_metadata(&self, key: &str) -> Result<Vec<GeminiModelMetadata>, AppError> {
        const MODEL_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
        let cache = MODEL_METADATA_CACHE.get_or_init(|| Mutex::new(None));
        let key_fingerprint = hex::encode(Sha256::digest(key.as_bytes()));
        if let Ok(cache) = cache.lock() {
            if let Some(cache) = cache.as_ref().filter(|cache| {
                cache.loaded_at.elapsed() < MODEL_CACHE_TTL
                    && cache.key_fingerprint == key_fingerprint
            }) {
                return Ok(cache.models.clone());
            }
        }

        let mut page_token = None::<String>;
        let mut models = Vec::new();
        loop {
            let mut request = self
                .http
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", key);
            if let Some(token) = &page_token {
                request = request.query(&[("pageToken", token)]);
            }
            let response = request.send().map_err(network_error)?;
            if !response.status().is_success() {
                return Err(gemini_http_error(response).0);
            }
            let page: Value = response.json().map_err(network_error)?;
            models.extend(parse_model_metadata(&page));
            page_token = page["nextPageToken"].as_str().map(str::to_string);
            if page_token.as_deref().map(str::is_empty).unwrap_or(true) {
                break;
            }
        }

        if let Ok(mut cache) = cache.lock() {
            *cache = Some(CachedModelMetadata {
                loaded_at: Instant::now(),
                key_fingerprint,
                models: models.clone(),
            });
        }
        Ok(models)
    }

    pub fn transcribe_segment(
        &self,
        descriptor: &SegmentDescriptor,
        title: &str,
        settings: &AppSettings,
        control: &JobControl,
        reporter: &dyn ProgressReporter,
    ) -> Result<SegmentTranscript, AppError> {
        control.checkpoint()?;
        let path = Path::new(&descriptor.path);
        let key = self.api_key()?;
        let mime_type = mime_for_audio(path);
        let mut remote_file = None;
        let audio_part = if fs::metadata(path).map_err(filesystem_error)?.len() <= INLINE_RAW_LIMIT
        {
            let bytes = fs::read(path).map_err(filesystem_error)?;
            json!({
                "inline_data": {
                    "mime_type": mime_type,
                    "data": BASE64.encode(bytes),
                }
            })
        } else {
            reporter.report(
                ProgressMetric::indeterminate("upload"),
                "Uploading audio segment",
            );
            let file = self.upload_file(&key, path, &mime_type, control, reporter)?;
            let part = json!({
                "file_data": {
                    "mime_type": file.mime_type,
                    "file_uri": file.uri,
                }
            });
            remote_file = Some(file.name);
            part
        };

        let prompt = build_prompt(descriptor, title, settings);
        let payload = json!({
            "contents": [{
                "parts": [
                    { "text": prompt },
                    audio_part
                ]
            }],
            "generationConfig": {
                "temperature": 0.0,
                "candidateCount": 1,
                "responseMimeType": "application/json",
                "responseSchema": {
                    "type": "OBJECT",
                    "properties": {
                        "language": { "type": "STRING" },
                        "languages": {
                            "type": "ARRAY",
                            "items": { "type": "STRING" }
                        },
                        "segments": {
                            "type": "ARRAY",
                            "items": {
                                "type": "OBJECT",
                                "properties": {
                                    "start_seconds": { "type": "NUMBER" },
                                    "end_seconds": { "type": "NUMBER" },
                                    "text": { "type": "STRING" },
                                    "language_code": { "type": "STRING" }
                                },
                                "required": ["start_seconds", "text"]
                            }
                        }
                    },
                    "required": ["segments"]
                }
            }
        });
        let url = generate_content_url(&settings.model);
        reporter.report(
            ProgressMetric::indeterminate("request"),
            "Waiting for Gemini",
        );
        let result = self.post_with_retries(&key, &url, &payload, control, reporter);
        if let Some(name) = remote_file {
            let _ = self.delete_file(&key, &name);
        }
        let value = result?;
        parse_transcript_response(&value, descriptor)
    }

    pub fn setup_test(
        &self,
        model: &str,
        audio_path: &Path,
        control: &JobControl,
    ) -> Result<String, AppError> {
        let settings = AppSettings {
            model: model.to_string(),
            prompt_preset: "setup_test".to_string(),
            ..AppSettings::default()
        };
        let descriptor = SegmentDescriptor {
            index: 1,
            path: audio_path.to_string_lossy().to_string(),
            start_seconds: 0.0,
            end_seconds: 1.0,
            checksum: String::new(),
            size_bytes: fs::metadata(audio_path)
                .map(|value| value.len())
                .unwrap_or_default(),
        };
        let transcript = self.transcribe_segment(
            &descriptor,
            "LectureScribe setup test",
            &settings,
            control,
            &NoopReporter,
        )?;
        Ok(transcript
            .segments
            .first()
            .map(|segment| segment.text.clone())
            .unwrap_or_else(|| "Gemini accepted the silent test audio.".to_string()))
    }

    fn post_with_retries(
        &self,
        key: &str,
        url: &str,
        payload: &Value,
        control: &JobControl,
        reporter: &dyn ProgressReporter,
    ) -> Result<Value, AppError> {
        let mut last_error = None;
        for attempt in 1..=3 {
            control.checkpoint()?;
            match self
                .http
                .post(url)
                .header("x-goog-api-key", key)
                .json(payload)
                .send()
            {
                Ok(response) if response.status().is_success() => {
                    return response.json().map_err(|error| {
                        AppError::new(
                            "gemini_response_invalid",
                            ErrorCategory::Transcription,
                            "Gemini returned an invalid response.",
                            error.to_string(),
                        )
                        .retryable("The audio segment remains cached.")
                    });
                }
                Ok(response) => {
                    let (error, delay) = gemini_http_error(response);
                    if !error.retryable || attempt == 3 {
                        return Err(error);
                    }
                    last_error = Some(error);
                    reporter.report(
                        ProgressMetric {
                            kind: ProgressKind::Indeterminate,
                            current: 0.0,
                            total: None,
                            unit: "retry".to_string(),
                            rate: None,
                            eta_seconds: Some(delay.as_secs()),
                        },
                        &format!(
                            "Gemini is temporarily unavailable; retrying in {}s",
                            delay.as_secs()
                        ),
                    );
                    sleep_with_cancel(delay, control)?;
                }
                Err(error) => {
                    let app_error = network_error(error)
                        .retryable("The audio segment remains cached for retry.");
                    if attempt == 3 {
                        return Err(app_error);
                    }
                    last_error = Some(app_error);
                    let delay = Duration::from_secs(2u64.pow(attempt));
                    sleep_with_cancel(delay, control)?;
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            AppError::new(
                "gemini_request_failed",
                ErrorCategory::Transcription,
                "Gemini could not transcribe this audio segment.",
                "Retry loop ended without a response.",
            )
        }))
    }

    fn upload_file(
        &self,
        key: &str,
        path: &Path,
        mime_type: &str,
        control: &JobControl,
        reporter: &dyn ProgressReporter,
    ) -> Result<UploadedFile, AppError> {
        let bytes = fs::read(path).map_err(filesystem_error)?;
        let display_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("audio-segment");
        control.checkpoint()?;
        let start = self
            .http
            .post("https://generativelanguage.googleapis.com/upload/v1beta/files")
            .header("x-goog-api-key", key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header(
                "X-Goog-Upload-Header-Content-Length",
                bytes.len().to_string(),
            )
            .header("X-Goog-Upload-Header-Content-Type", mime_type)
            .json(&json!({ "file": { "display_name": display_name } }))
            .send()
            .map_err(network_error)?;
        if !start.status().is_success() {
            return Err(gemini_http_error(start).0);
        }
        let upload_url = start
            .headers()
            .get("x-goog-upload-url")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                AppError::new(
                    "gemini_upload_url_missing",
                    ErrorCategory::Transcription,
                    "Gemini did not prepare the audio upload.",
                    "Upload response omitted x-goog-upload-url.",
                )
            })?
            .to_string();
        control.checkpoint()?;
        let response = self
            .http
            .post(upload_url)
            .header("Content-Length", bytes.len().to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(bytes)
            .send()
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(gemini_http_error(response).0);
        }
        let value: Value = response.json().map_err(network_error)?;
        let file = &value["file"];
        let name = file["name"].as_str().ok_or_else(|| {
            AppError::new(
                "gemini_file_name_missing",
                ErrorCategory::Transcription,
                "Gemini did not return the uploaded file identifier.",
                value.to_string(),
            )
        })?;
        let uri = file["uri"].as_str().ok_or_else(|| {
            AppError::new(
                "gemini_file_uri_missing",
                ErrorCategory::Transcription,
                "Gemini did not return the uploaded file URI.",
                value.to_string(),
            )
        })?;
        let mime = file["mimeType"]
            .as_str()
            .or_else(|| file["mime_type"].as_str())
            .unwrap_or(mime_type);
        let uploaded = UploadedFile {
            name: name.to_string(),
            uri: uri.to_string(),
            mime_type: mime.to_string(),
        };
        self.wait_until_active(key, uploaded, control, reporter)
    }

    fn wait_until_active(
        &self,
        key: &str,
        mut file: UploadedFile,
        control: &JobControl,
        reporter: &dyn ProgressReporter,
    ) -> Result<UploadedFile, AppError> {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            control.checkpoint()?;
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/{}",
                file.name
            );
            let response = self
                .http
                .get(url)
                .header("x-goog-api-key", key)
                .send()
                .map_err(network_error)?;
            if !response.status().is_success() {
                return Err(gemini_http_error(response).0);
            }
            let value: Value = response.json().map_err(network_error)?;
            let state = value["state"].as_str().unwrap_or("PROCESSING");
            if state == "ACTIVE" {
                file.uri = value["uri"].as_str().unwrap_or(&file.uri).to_string();
                return Ok(file);
            }
            if state == "FAILED" {
                return Err(AppError::new(
                    "gemini_file_processing_failed",
                    ErrorCategory::Transcription,
                    "Gemini could not process the uploaded audio.",
                    value.to_string(),
                )
                .retryable("The local audio segment remains cached."));
            }
            if Instant::now() >= deadline {
                return Err(AppError::new(
                    "gemini_file_processing_timeout",
                    ErrorCategory::Transcription,
                    "Gemini took too long to prepare the uploaded audio.",
                    value.to_string(),
                )
                .retryable("The local audio segment remains cached."));
            }
            reporter.report(
                ProgressMetric::indeterminate("upload"),
                "Gemini is preparing the uploaded audio",
            );
            sleep_with_cancel(Duration::from_secs(2), control)?;
        }
    }

    fn delete_file(&self, key: &str, name: &str) -> Result<(), AppError> {
        let url = format!("https://generativelanguage.googleapis.com/v1beta/{name}");
        let response = self
            .http
            .delete(url)
            .header("x-goog-api-key", key)
            .send()
            .map_err(network_error)?;
        if response.status().is_success() || response.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(gemini_http_error(response).0)
        }
    }
}

#[derive(Clone)]
struct CachedModelMetadata {
    loaded_at: Instant,
    key_fingerprint: String,
    models: Vec<GeminiModelMetadata>,
}

#[derive(Debug, Clone)]
struct UploadedFile {
    name: String,
    uri: String,
    mime_type: String,
}

fn parse_model_metadata(value: &Value) -> Vec<GeminiModelMetadata> {
    value["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model["name"].as_str().map(normalize_model_id)?;
            (!id.is_empty()).then(|| GeminiModelMetadata {
                display_name: model["displayName"].as_str().unwrap_or(&id).to_string(),
                description: model["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                supported_generation_methods: model["supportedGenerationMethods"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                id,
            })
        })
        .collect()
}

fn build_prompt(descriptor: &SegmentDescriptor, title: &str, settings: &AppSettings) -> String {
    let preset = match settings.prompt_preset.as_str() {
        "arabic" | "arabic_lecture" => {
            "Arabic is expected. Preserve the spoken dialect, script, and any English or other code-switching exactly; do not translate or rewrite into Modern Standard Arabic."
        }
        "english" | "english_lecture" => {
            "English is expected. Preserve technical terms and any speech in other languages exactly; do not translate."
        }
        "technical" | "technical_math" => {
            "Preserve formulas, variables, units, code, and technical terminology exactly. Do not expand or simplify technical statements."
        }
        "math_science" => {
            "This is math or science content. Preserve the speaker's exact wording, variables, operators, units, and equation order. Use LaTeX only when the spoken expression is unambiguous; otherwise keep the spoken wording and mark it [unclear] instead of inventing notation."
        }
        "technical_code" => {
            "This is technical or programming content. Preserve identifiers, commands, filenames, acronyms, case-sensitive terms, version numbers, punctuation, and units. Never paraphrase code or commands."
        }
        "interview" => {
            "This is a conversation. Preserve turn-taking, interruptions, false starts, and meaningful filler words. Do not infer a person's identity or role when it is not stated."
        }
        "multilingual" => {
            "The audio may switch languages. Preserve every language in its original script and keep code-switching in place. Do not translate, transliterate, or force all speech into one language."
        }
        "setup_test" => {
            "The audio may be silent; return an empty segments array if no speech exists."
        }
        _ => {
            "Preserve the spoken language, wording, false starts, meaningful filler words, and genuine repetition accurately. Add punctuation for readability without paraphrasing."
        }
    };
    let language = language_policy(settings);
    let additional = settings.additional_prompt.trim();
    format!(
        "Transcribe only the attached audio segment as timestamped JSON.\nTitle: {title}\nAbsolute segment range: {start:.3} to {end:.3} seconds.\nLanguage policy: {language}\nContent profile: {preset}\nAdditional user guidance: {additional}\nRules: produce a faithful transcript, not a summary; detect every spoken language; preserve each language's original wording and script, including code-switching; do not translate or transliterate; do not invent missing speech; do not silently remove uncertainty; timestamps must be absolute seconds in the original media; preserve genuine repetition; use an optional BCP-47 language_code for every segment and a languages summary; return an empty segments array for silence; stop at the end of this segment.",
        start = descriptor.start_seconds,
        end = descriptor.end_seconds,
    )
}

fn language_policy(settings: &AppSettings) -> String {
    let preferences = serde_json::to_value(&settings.language).unwrap_or(Value::Null);
    let mode = preferences["mode"].as_str().unwrap_or("auto");
    let hints = preferences["hints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|hint| !hint.is_empty())
        .take(5)
        .collect::<Vec<_>>();
    let mode_note = match mode {
        "auto" | "detect" | "" => "Detect every spoken language without assuming one dominant language.",
        _ => "Detect every spoken language; the selected language mode is a preference, not a restriction.",
    };
    if hints.is_empty() {
        format!("{mode_note} No language hints were supplied.")
    } else {
        format!(
            "{mode_note} Soft language hints: {}. These hints never exclude unexpected languages.",
            hints.join(", ")
        )
    }
}

fn parse_transcript_response(
    value: &Value,
    descriptor: &SegmentDescriptor,
) -> Result<SegmentTranscript, AppError> {
    let candidate = value["candidates"]
        .as_array()
        .and_then(|values| values.first());
    let finish_reason = candidate
        .and_then(|candidate| candidate["finishReason"].as_str())
        .unwrap_or("UNKNOWN");
    if finish_reason == "MAX_TOKENS" {
        return Err(AppError::new(
            "transcript_truncated",
            ErrorCategory::Transcription,
            "Gemini reached its output limit for this segment.",
            value.to_string(),
        )
        .retryable("The segment remains cached and can be split into smaller parts."));
    }
    if !matches!(finish_reason, "STOP" | "UNKNOWN") {
        return Err(AppError::new(
            "transcript_finish_rejected",
            ErrorCategory::Transcription,
            "Gemini did not complete this transcript segment.",
            format!("finishReason={finish_reason}"),
        )
        .retryable("The segment remains cached."));
    }
    let text = candidate
        .and_then(|candidate| candidate["content"]["parts"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(AppError::new(
            "transcript_empty_response",
            ErrorCategory::Transcription,
            "Gemini returned no transcript data.",
            value.to_string(),
        )
        .retryable("The segment remains cached."));
    }
    let cleaned = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: Value = serde_json::from_str(cleaned).map_err(|error| {
        AppError::new(
            "transcript_schema_invalid",
            ErrorCategory::Transcription,
            "Gemini returned transcript data in an invalid format.",
            error.to_string(),
        )
        .retryable("The segment remains cached.")
    })?;
    let values = parsed["segments"].as_array().ok_or_else(|| {
        AppError::new(
            "transcript_segments_missing",
            ErrorCategory::Transcription,
            "Gemini returned transcript data without segments.",
            cleaned,
        )
        .retryable("The segment remains cached.")
    })?;
    let mut segments = Vec::new();
    let mut languages = language_codes_from_summary(&parsed);
    if let Some(language) = parsed["language"].as_str() {
        push_language_code(&mut languages, language);
    }
    for segment in values {
        let mut start = segment["start_seconds"]
            .as_f64()
            .unwrap_or(descriptor.start_seconds);
        let mut end = segment["end_seconds"].as_f64();
        if descriptor.start_seconds > 0.0 && start < descriptor.start_seconds - 1.0 {
            start += descriptor.start_seconds;
            end = end.map(|value| value + descriptor.start_seconds);
        }
        if start < descriptor.start_seconds - 5.0 || start > descriptor.end_seconds + 5.0 {
            return Err(AppError::new(
                "transcript_timestamp_out_of_range",
                ErrorCategory::Transcription,
                "Gemini returned timestamps outside the audio segment.",
                segment.to_string(),
            )
            .retryable("The segment remains cached."));
        }
        let text = segment["text"].as_str().unwrap_or_default().trim();
        if text.is_empty() {
            continue;
        }
        if looks_like_model_loop(text) {
            return Err(AppError::new(
                "transcript_repetition_detected",
                ErrorCategory::Transcription,
                "Gemini produced an abnormal repetition loop.",
                "Repeated token or phrase threshold was exceeded.",
            )
            .retryable("Nothing was deleted; the rejected response is not used."));
        }
        let language_code = normalize_language_code(segment["language_code"].as_str());
        push_language_code(&mut languages, &language_code);
        segments.push(TranscriptSegment {
            start_seconds: start.max(0.0),
            end_seconds: end,
            text: text.to_string(),
            language_code: Some(language_code),
        });
    }
    if languages.is_empty() {
        languages.push("und".to_string());
    }
    Ok(SegmentTranscript {
        language: languages
            .first()
            .cloned()
            .unwrap_or_else(|| "und".to_string()),
        languages,
        segments,
    })
}

fn language_codes_from_summary(value: &Value) -> Vec<String> {
    let mut languages = Vec::new();
    for entry in value["languages"].as_array().into_iter().flatten() {
        match entry {
            Value::String(code) => push_language_code(&mut languages, code),
            Value::Object(_) => {
                let code = entry["language_code"]
                    .as_str()
                    .or_else(|| entry["code"].as_str())
                    .or_else(|| entry["language"].as_str());
                if let Some(code) = code {
                    push_language_code(&mut languages, code);
                }
            }
            _ => {}
        }
    }
    languages
}

fn push_language_code(languages: &mut Vec<String>, code: &str) {
    let code = normalize_language_code(Some(code));
    if !languages.contains(&code) {
        languages.push(code);
    }
}

fn normalize_language_code(code: Option<&str>) -> String {
    let code = code.unwrap_or_default().trim();
    if code.is_empty() {
        "und".to_string()
    } else {
        code.to_ascii_lowercase()
    }
}

fn generate_content_url(model: &str) -> String {
    format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        normalize_model_id(model)
    )
}

fn looks_like_model_loop(text: &str) -> bool {
    let words = text
        .split_whitespace()
        .map(normalize_word)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let max_run = words
        .iter()
        .fold(
            (String::new(), 0usize, 0usize),
            |(previous, run, max), word| {
                let next = if *word == previous { run + 1 } else { 1 };
                (word.clone(), next, max.max(next))
            },
        )
        .2;
    if max_run >= 25 {
        return true;
    }
    let mut counts = std::collections::HashMap::<Vec<String>, usize>::new();
    for gram in words.windows(5) {
        *counts.entry(gram.to_vec()).or_default() += 1;
    }
    counts.values().copied().max().unwrap_or_default() >= 25
}

fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn gemini_http_error(response: Response) -> (AppError, Duration) {
    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(30));
    let body = response.text().unwrap_or_default();
    if status.as_u16() == 401 || status.as_u16() == 403 || body.contains("API_KEY_INVALID") {
        (
            AppError::new(
                "api_key_rejected",
                ErrorCategory::Authentication,
                "Gemini rejected the API key.",
                body,
            )
            .with_action("open_setup_api", "Replace API key", "open_setup_api"),
            retry_after,
        )
    } else if status.as_u16() == 429 || body.contains("RESOURCE_EXHAUSTED") {
        (
            AppError::new(
                "gemini_quota_wait",
                ErrorCategory::Quota,
                "Gemini's request limit was reached.",
                body,
            )
            .retryable("Completed segments remain cached."),
            retry_after,
        )
    } else if status.is_server_error() {
        (
            AppError::new(
                "gemini_service_unavailable",
                ErrorCategory::Network,
                "Gemini is temporarily unavailable.",
                body,
            )
            .retryable("Completed segments remain cached."),
            retry_after,
        )
    } else {
        (
            AppError::new(
                "gemini_request_rejected",
                ErrorCategory::Transcription,
                "Gemini rejected this transcription request.",
                format!("HTTP {status}: {body}"),
            ),
            retry_after,
        )
    }
}

fn sleep_with_cancel(duration: Duration, control: &JobControl) -> Result<(), AppError> {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        control.checkpoint()?;
        thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn mime_for_audio(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        _ => "audio/mpeg",
    }
    .to_string()
}

fn network_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "gemini_network_failed",
        ErrorCategory::Network,
        "LectureScribe could not reach Gemini.",
        error.to_string(),
    )
}

fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "gemini_audio_read_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not read the prepared audio segment.",
        error.to_string(),
    )
}

struct NoopReporter;
impl ProgressReporter for NoopReporter {
    fn report(&self, _progress: ProgressMetric, _message: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use lecturescribe_core::{LanguageMode, LanguagePreferences};

    fn descriptor() -> SegmentDescriptor {
        SegmentDescriptor {
            index: 1,
            path: "segment.wav".to_string(),
            start_seconds: 10.0,
            end_seconds: 20.0,
            checksum: String::new(),
            size_bytes: 0,
        }
    }

    #[test]
    fn genuine_short_repetition_is_not_trimmed() {
        assert!(!looks_like_model_loop("yes yes yes, that is correct"));
    }

    #[test]
    fn abnormal_long_repetition_is_rejected() {
        assert!(looks_like_model_loop(&vec!["loop"; 30].join(" ")));
    }

    #[test]
    fn multilingual_prompt_preserves_scripts_and_soft_hints() {
        let settings = AppSettings {
            language: LanguagePreferences {
                mode: LanguageMode::Hints,
                hints: vec!["ar".to_string(), "en".to_string(), "fr".to_string()],
            },
            ..AppSettings::default()
        };
        let prompt = build_prompt(&descriptor(), "Mixed lecture", &settings);

        assert!(prompt.contains("detect every spoken language"));
        assert!(prompt.contains("original wording and script"));
        assert!(prompt.contains("do not translate or transliterate"));
        assert!(prompt.contains("Soft language hints: ar, en, fr"));
        assert!(prompt.contains("never exclude unexpected languages"));
    }

    #[test]
    fn legacy_and_empty_language_preferences_keep_detection_enabled() {
        let legacy = AppSettings {
            language: LanguagePreferences::from_legacy("ar"),
            ..AppSettings::default()
        };
        let legacy_prompt = build_prompt(&descriptor(), "Lecture", &legacy);
        let default_prompt = build_prompt(&descriptor(), "Lecture", &AppSettings::default());

        assert!(legacy_prompt.contains("Soft language hints: ar"));
        assert!(legacy_prompt.contains("never exclude unexpected languages"));
        assert!(default_prompt.contains("No language hints were supplied"));
    }

    #[test]
    fn parser_keeps_text_when_language_codes_are_missing() {
        let response = json!({
            "candidates": [{
                "finishReason": "STOP",
                "content": { "parts": [{ "text": r#"{
                    "languages": ["en", {"code": "es"}],
                    "segments": [
                        {"start_seconds": 10.0, "end_seconds": 12.0, "text": "hello", "language_code": "en"},
                        {"start_seconds": 12.0, "text": "hola"}
                    ]
                }"# }] }
            }]
        });

        let transcript = parse_transcript_response(&response, &descriptor()).unwrap();

        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transcript.segments[0].language_code.as_deref(), Some("en"));
        assert_eq!(transcript.segments[1].language_code.as_deref(), Some("und"));
        assert_eq!(transcript.languages, vec!["en", "es", "und"]);
    }

    #[test]
    fn selected_model_url_has_no_automatic_fallback() {
        let url = generate_content_url("models/gemini-3.5-flash");

        assert!(url.ends_with("models/gemini-3.5-flash:generateContent"));
        assert!(!url.contains("gemini-3.1-flash-lite"));
    }
}
