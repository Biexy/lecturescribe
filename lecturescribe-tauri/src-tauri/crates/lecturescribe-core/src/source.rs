use crate::{
    AppError, ErrorCategory, ErrorSeverity, ItemState, PreviewItem, ProviderKind, SourceInput,
    SourceKind,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;

pub fn stable_id(namespace: &str, value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(namespace.as_bytes());
    digest.update([0]);
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())[..20].to_string()
}

pub fn extract_urls(text: &str) -> Vec<String> {
    let regex = Regex::new(r#"https?://[^\s<>\"']+"#).expect("URL regex is valid");
    regex
        .find_iter(text.trim_start_matches('\u{feff}'))
        .map(|value| {
            value
                .as_str()
                .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | ')' | ']' | '}'))
                .to_string()
        })
        .filter(|value| Url::parse(value).is_ok())
        .collect()
}

pub fn canonicalize_source(value: &str) -> Result<(ProviderKind, String), AppError> {
    let value = value.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return Err(input_error("source_empty", "The source is empty."));
    }

    if !value.starts_with("http://") && !value.starts_with("https://") {
        let path = canonical_local_path(Path::new(value));
        let key = if cfg!(target_os = "windows") {
            path.to_string_lossy().to_lowercase()
        } else {
            path.to_string_lossy().to_string()
        };
        return Ok((ProviderKind::Local, key));
    }

    let mut url = Url::parse(value).map_err(|error| {
        AppError::new(
            "source_invalid_url",
            ErrorCategory::Input,
            "This link is not a valid URL.",
            error.to_string(),
        )
    })?;
    url.set_fragment(None);
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();

    if is_youtube_host(&host) {
        let id = youtube_id(&url).ok_or_else(|| {
            input_error(
                "youtube_unsupported_url",
                "This YouTube link does not identify a supported video.",
            )
        })?;
        return Ok((ProviderKind::YouTube, format!("youtube:{id}")));
    }

    if host == "drive.google.com" || host == "docs.google.com" {
        let id = drive_file_id(&url).ok_or_else(|| {
            input_error(
                "drive_unsupported_url",
                "This Google Drive link does not identify a file.",
            )
        })?;
        return Ok((ProviderKind::GoogleDrive, format!("drive:{id}")));
    }

    let scheme = url.scheme().to_ascii_lowercase();
    let port = url
        .port()
        .map(|value| format!(":{value}"))
        .unwrap_or_default();
    let mut pairs = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_parameter(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    pairs.sort();
    url.set_query(None);
    let query = if pairs.is_empty() {
        String::new()
    } else {
        format!(
            "?{}",
            pairs
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("&")
        )
    };
    Ok((
        ProviderKind::Generic,
        format!("{scheme}://{host}{port}{}{query}", url.path()),
    ))
}

pub fn inspect_source_values(values: &[(SourceInput, String)]) -> (Vec<PreviewItem>, usize) {
    let mut first_by_canonical = HashMap::<String, String>::new();
    let mut duplicates = 0usize;
    let mut items = Vec::new();

    for (source, value) in values {
        let source_id = if source.id.trim().is_empty() {
            stable_id("source", &format!("{:?}:{}", source.kind, source.value))
        } else {
            source.id.clone()
        };
        match canonicalize_source(value) {
            Ok((provider, canonical)) => {
                let id = stable_id("item", &canonical);
                let duplicate_of = first_by_canonical.get(&canonical).cloned();
                if duplicate_of.is_some() {
                    duplicates += 1;
                } else {
                    first_by_canonical.insert(canonical.clone(), id.clone());
                }
                let is_local = provider == ProviderKind::Local;
                items.push(PreviewItem {
                    id,
                    source_id,
                    source_kind: source.kind,
                    provider,
                    source_group: source_label(source),
                    title: source_title(value, provider),
                    source: value.clone(),
                    canonical_source: canonical,
                    url: (!is_local).then(|| value.clone()),
                    media_path: is_local.then(|| value.clone()),
                    existing_media_path: None,
                    existing_transcript_path: None,
                    thumbnail_url: None,
                    duration_seconds: None,
                    expected_media_name: None,
                    selected: duplicate_of.is_none(),
                    status: if duplicate_of.is_some() {
                        ItemState::Excluded
                    } else {
                        ItemState::Ready
                    },
                    duplicate_of,
                    error: None,
                });
            }
            Err(mut error) => {
                error.severity = ErrorSeverity::Warning;
                items.push(PreviewItem {
                    id: stable_id("invalid-item", value),
                    source_id,
                    source_kind: source.kind,
                    provider: ProviderKind::Generic,
                    source_group: source_label(source),
                    title: source_title(value, ProviderKind::Generic),
                    source: value.clone(),
                    canonical_source: value.clone(),
                    url: value.starts_with("http").then(|| value.clone()),
                    media_path: None,
                    existing_media_path: None,
                    existing_transcript_path: None,
                    thumbnail_url: None,
                    duration_seconds: None,
                    expected_media_name: None,
                    selected: false,
                    status: ItemState::Blocked,
                    duplicate_of: None,
                    error: Some(error),
                });
            }
        }
    }

    (items, duplicates)
}

fn source_label(source: &SourceInput) -> String {
    if !source.label.trim().is_empty() {
        return source.label.trim().to_string();
    }
    match source.kind {
        SourceKind::PastedLink => "Pasted links",
        SourceKind::TextFile => "Link file",
        SourceKind::LocalMedia => "Local media",
        SourceKind::Directory => "Media folder",
        SourceKind::AutomaticFile => "Automatic link file",
    }
    .to_string()
}

fn source_title(value: &str, provider: ProviderKind) -> String {
    if provider == ProviderKind::Local {
        return Path::new(value)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Local media")
            .to_string();
    }
    match provider {
        ProviderKind::YouTube => "YouTube video",
        ProviderKind::GoogleDrive => "Google Drive video",
        ProviderKind::Generic => "Linked media",
        ProviderKind::Local => "Local media",
    }
    .to_string()
}

fn canonical_local_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    absolute.canonicalize().unwrap_or(absolute)
}

fn is_youtube_host(host: &str) -> bool {
    matches!(
        host,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtu.be" | "music.youtube.com"
    )
}

fn youtube_id(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host == "youtu.be" {
        return url
            .path_segments()?
            .find(|segment| !segment.is_empty())
            .map(ToString::to_string);
    }
    if url.path() == "/watch" {
        return url
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned());
    }
    let mut segments = url.path_segments()?;
    let first = segments.next()?;
    if matches!(first, "shorts" | "embed" | "live") {
        return segments.next().map(ToString::to_string);
    }
    None
}

fn drive_file_id(url: &Url) -> Option<String> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    for window in segments.windows(2) {
        if window[0] == "d" && !window[1].is_empty() {
            return Some(window[1].to_string());
        }
    }
    url.query_pairs()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.into_owned())
}

fn is_tracking_parameter(value: &str) -> bool {
    value.starts_with("utm_") || matches!(value, "fbclid" | "gclid" | "si" | "feature")
}

fn input_error(code: &str, message: &str) -> AppError {
    AppError::new(code, ErrorCategory::Input, message, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_variants_share_the_same_identity() {
        let (_, first) = canonicalize_source("https://youtu.be/AbC123_XyZ?t=10").unwrap();
        let (_, second) =
            canonicalize_source("https://www.youtube.com/watch?v=AbC123_XyZ&utm_source=x").unwrap();
        assert_eq!(first, second);
        assert_eq!(first, "youtube:AbC123_XyZ");
    }

    #[test]
    fn drive_id_remains_case_sensitive() {
        let (_, value) =
            canonicalize_source("https://drive.google.com/file/d/AbCdEf_123/view").unwrap();
        assert_eq!(value, "drive:AbCdEf_123");
    }

    #[test]
    fn url_extraction_trims_sentence_punctuation() {
        let values = extract_urls("One: https://example.com/a. Two (https://youtu.be/abc123). ");
        assert_eq!(
            values,
            vec!["https://example.com/a", "https://youtu.be/abc123"]
        );
    }
}
