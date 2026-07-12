use crate::gemini::SegmentTranscript;
use crate::media::{atomic_write, sha256_file};
use crate::paths::safe_component;
use lecturescribe_core::{
    AppError, AppSettings, ArtifactKind, ErrorCategory, TranscriptDocument, TranscriptFormat,
    TranscriptSegment, TRANSCRIPT_SCHEMA_VERSION,
};
use lecturescribe_engine::JobControl;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WrittenOutput {
    pub kind: ArtifactKind,
    pub path: PathBuf,
    pub checksum: String,
    pub size_bytes: u64,
}

pub fn merge_transcripts(
    item_id: &str,
    title: &str,
    source: &str,
    model: &str,
    transcripts: Vec<SegmentTranscript>,
) -> Result<TranscriptDocument, AppError> {
    let mut languages = transcripts
        .iter()
        .flat_map(|value| {
            value
                .languages
                .iter()
                .chain(std::iter::once(&value.language))
        })
        .filter_map(|language| normalized_language(language))
        .collect::<BTreeSet<_>>();
    let mut incoming = transcripts
        .into_iter()
        .flat_map(|value| value.segments)
        .collect::<Vec<_>>();
    incoming.sort_by(|left, right| {
        left.start_seconds
            .partial_cmp(&right.start_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut segments = Vec::<TranscriptSegment>::new();
    for mut segment in incoming {
        segment.text = segment.text.trim().to_string();
        if segment.text.is_empty() {
            continue;
        }
        if let Some(language) = segment
            .language_code
            .as_deref()
            .and_then(normalized_language)
        {
            languages.insert(language);
        }
        if let Some(previous) = segments.last() {
            let overlap = boundary_overlap_words(&previous.text, &segment.text);
            if overlap > 0 {
                segment.text = remove_prefix_words(&segment.text, overlap)
                    .trim()
                    .to_string();
            }
        }
        if segment.text.is_empty() {
            continue;
        }
        if let Some(previous) = segments.last() {
            if segment.start_seconds < previous.start_seconds {
                return Err(AppError::new(
                    "transcript_timestamp_order_invalid",
                    ErrorCategory::Transcription,
                    "Transcript timestamps were not in chronological order.",
                    format!(
                        "{} followed {}",
                        segment.start_seconds, previous.start_seconds
                    ),
                ));
            }
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        return Err(AppError::new(
            "transcript_contains_no_speech",
            ErrorCategory::Transcription,
            "No speech was detected in this media.",
            "All validated segment responses were empty.",
        ));
    }
    Ok(TranscriptDocument {
        schema_version: TRANSCRIPT_SCHEMA_VERSION,
        item_id: item_id.to_string(),
        title: title.to_string(),
        source: source.to_string(),
        language: languages
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        languages: languages.into_iter().collect(),
        model: model.to_string(),
        generated_at: chrono::Utc::now(),
        segments,
    })
}

fn normalized_language(language: &str) -> Option<String> {
    let language = language.trim();
    (!language.is_empty() && !language.eq_ignore_ascii_case("unknown"))
        .then(|| language.to_ascii_lowercase())
}

fn safe_output_stem(output_stem: &str, item_id: &str) -> String {
    let stem = output_stem.trim();
    if stem.is_empty()
        || stem.contains(":\\")
        || stem.contains(":/")
        || stem.starts_with("\\\\")
        || stem.starts_with('/')
        || stem.contains("://")
    {
        return format!("Transcript [{}]", safe_component(item_id));
    }
    let stem = stem
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let stem = stem
        .trim_matches([' ', '.'])
        .chars()
        .take(100)
        .collect::<String>();
    if stem.is_empty() {
        format!("Transcript [{}]", safe_component(item_id))
    } else {
        stem
    }
}

pub fn write_outputs(
    document: &TranscriptDocument,
    output_dir: &Path,
    output_stem: &str,
    settings: &AppSettings,
    control: &JobControl,
) -> Result<Vec<WrittenOutput>, AppError> {
    fs::create_dir_all(output_dir).map_err(filesystem_error)?;
    let base = safe_output_stem(output_stem, &document.item_id);
    let mut outputs = Vec::new();
    for format in &settings.output_formats {
        let (kind, extension, content) = match format {
            TranscriptFormat::Text => (ArtifactKind::TextTranscript, "txt", render_text(document)),
            TranscriptFormat::Markdown => (
                ArtifactKind::MarkdownTranscript,
                "md",
                render_markdown(document),
            ),
            TranscriptFormat::Srt => (ArtifactKind::SrtTranscript, "srt", render_srt(document)),
            TranscriptFormat::Vtt => (ArtifactKind::VttTranscript, "vtt", render_vtt(document)),
        };
        let path = output_dir.join(format!("{base}.{extension}"));
        atomic_write(&path, content.as_bytes())?;
        let size_bytes = fs::metadata(&path).map_err(filesystem_error)?.len();
        let checksum = sha256_file(&path, control)?;
        outputs.push(WrittenOutput {
            kind,
            path,
            checksum,
            size_bytes,
        });
    }
    Ok(outputs)
}

pub fn canonical_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("canonical-transcript.json")
}

fn render_text(document: &TranscriptDocument) -> String {
    let mut lines = vec![
        document.title.clone(),
        format!("Detected languages: {}", language_summary(document)),
        String::new(),
    ];
    for segment in &document.segments {
        lines.push(format!(
            "[{}] {}",
            compact_timestamp(segment.start_seconds),
            segment.text
        ));
    }
    format!("{}\n", lines.join("\n").trim())
}

fn render_markdown(document: &TranscriptDocument) -> String {
    let mut lines = vec![
        format!("# {}", document.title),
        String::new(),
        format!("Detected languages: {}", language_summary(document)),
        String::new(),
    ];
    for segment in &document.segments {
        lines.push(format!(
            "**[{}]** {}",
            compact_timestamp(segment.start_seconds),
            segment.text
        ));
    }
    format!("{}\n", lines.join("\n").trim())
}

fn language_summary(document: &TranscriptDocument) -> String {
    let mut languages = document
        .languages
        .iter()
        .filter(|language| !language.trim().is_empty() && language.as_str() != "unknown")
        .cloned()
        .collect::<Vec<_>>();
    languages.sort();
    languages.dedup();
    if languages.is_empty() {
        "Unknown".to_string()
    } else {
        languages.join(", ")
    }
}

fn compact_timestamp(seconds: f64) -> String {
    let whole_seconds = seconds.max(0.0).floor() as u64;
    let hours = whole_seconds / 3_600;
    let minutes = whole_seconds % 3_600 / 60;
    let secs = whole_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn render_srt(document: &TranscriptDocument) -> String {
    document
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let end = segment
                .end_seconds
                .unwrap_or_else(|| next_end(document, index));
            format!(
                "{}\n{} --> {}\n{}",
                index + 1,
                timestamp(segment.start_seconds, true),
                timestamp(end, true),
                segment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
        + "\n"
}

fn render_vtt(document: &TranscriptDocument) -> String {
    let body = document
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let end = segment
                .end_seconds
                .unwrap_or_else(|| next_end(document, index));
            format!(
                "{} --> {}\n{}",
                timestamp_vtt(segment.start_seconds),
                timestamp_vtt(end),
                segment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("WEBVTT\n\n{body}\n")
}

fn next_end(document: &TranscriptDocument, index: usize) -> f64 {
    document
        .segments
        .get(index + 1)
        .map(|segment| segment.start_seconds)
        .unwrap_or(document.segments[index].start_seconds + 5.0)
        .max(document.segments[index].start_seconds + 0.5)
}

fn boundary_overlap_words(previous: &str, current: &str) -> usize {
    let previous_words = normalized_words(previous);
    let current_words = normalized_words(current);
    let max = previous_words.len().min(current_words.len()).min(80);
    for count in (5..=max).rev() {
        if previous_words[previous_words.len() - count..] == current_words[..count] {
            return count;
        }
    }
    0
}

fn normalized_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|character| character.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn remove_prefix_words(text: &str, count: usize) -> &str {
    let mut consumed = 0usize;
    let mut in_word = false;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if in_word {
                consumed += 1;
                in_word = false;
                if consumed == count {
                    return &text[index..];
                }
            }
        } else {
            in_word = true;
        }
    }
    if in_word && consumed + 1 == count {
        ""
    } else {
        text
    }
}

fn timestamp(seconds: f64, comma: bool) -> String {
    let milliseconds = (seconds.max(0.0) * 1000.0).round() as u64;
    let hours = milliseconds / 3_600_000;
    let minutes = milliseconds % 3_600_000 / 60_000;
    let secs = milliseconds % 60_000 / 1000;
    let millis = milliseconds % 1000;
    let separator = if comma { ',' } else { '.' };
    format!("{hours:02}:{minutes:02}:{secs:02}{separator}{millis:03}")
}

fn timestamp_vtt(seconds: f64) -> String {
    timestamp(seconds, false)
}

fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "transcript_write_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not write the transcript output.",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lecturescribe_core::OutputPackage;

    fn document() -> TranscriptDocument {
        TranscriptDocument {
            schema_version: TRANSCRIPT_SCHEMA_VERSION,
            item_id: "item-1".to_string(),
            title: "Network Security Lecture".to_string(),
            source: "https://drive.google.com/file/d/private-token/C:\\Users\\Alice\\private.mp4"
                .to_string(),
            language: "en".to_string(),
            languages: vec!["en".to_string(), "es".to_string()],
            model: "gemini-private-model".to_string(),
            generated_at: chrono::Utc::now(),
            segments: vec![TranscriptSegment {
                start_seconds: 65.2,
                end_seconds: Some(68.0),
                text: "Welcome to the course.".to_string(),
                language_code: Some("en".to_string()),
            }],
        }
    }

    fn temporary_output_dir() -> PathBuf {
        std::env::temp_dir().join(format!("lecturescribe-output-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn removes_only_exact_boundary_overlap() {
        let overlap = boundary_overlap_words(
            "This is a repeated boundary with five exact words",
            "boundary with five exact words and then new content",
        );
        assert_eq!(overlap, 5);
        let trimmed = remove_prefix_words(
            "boundary with five exact words and then new content",
            overlap,
        );
        assert_eq!(trimmed.trim(), "and then new content");
    }

    #[test]
    fn unrelated_repetition_is_preserved() {
        assert_eq!(boundary_overlap_words("yes yes yes", "yes yes yes"), 0);
    }

    #[test]
    fn user_transcripts_use_the_frozen_stem_and_exclude_private_metadata() {
        let output_dir = temporary_output_dir();
        let settings = AppSettings {
            output_package: OutputPackage::Readable,
            ..AppSettings::default()
        };
        let outputs = write_outputs(
            &document(),
            &output_dir,
            "Frozen lecture [item-1]",
            &settings.sanitized(),
            &JobControl::default(),
        )
        .unwrap();

        assert_eq!(outputs.len(), 2);
        let text = fs::read_to_string(output_dir.join("Frozen lecture [item-1].txt")).unwrap();
        let markdown = fs::read_to_string(output_dir.join("Frozen lecture [item-1].md")).unwrap();
        for content in [&text, &markdown] {
            assert!(content.contains("Network Security Lecture"));
            assert!(content.contains("Detected languages: en, es"));
            assert!(!content.contains("drive.google.com"));
            assert!(!content.contains("C:\\Users"));
            assert!(!content.contains("gemini-private-model"));
        }
        assert!(text.contains("[01:05] Welcome to the course."));
        assert!(markdown.contains("**[01:05]** Welcome to the course."));
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn each_output_package_writes_its_expected_artifacts() {
        let cases = [
            (OutputPackage::Readable, vec!["txt", "md"]),
            (OutputPackage::Subtitles, vec!["srt", "vtt"]),
            (OutputPackage::Complete, vec!["txt", "md", "srt", "vtt"]),
            (OutputPackage::Custom, vec!["md"]),
        ];
        for (package, expected_extensions) in cases {
            let output_dir = temporary_output_dir();
            let settings = AppSettings {
                output_package: package,
                output_formats: if package == OutputPackage::Custom {
                    vec![TranscriptFormat::Markdown]
                } else {
                    AppSettings::default().output_formats
                },
                ..AppSettings::default()
            };
            let outputs = write_outputs(
                &document(),
                &output_dir,
                "Lecture",
                &settings.sanitized(),
                &JobControl::default(),
            )
            .unwrap();

            assert_eq!(outputs.len(), expected_extensions.len());
            for extension in expected_extensions {
                assert!(output_dir.join(format!("Lecture.{extension}")).is_file());
            }
            let _ = fs::remove_dir_all(output_dir);
        }
    }
}
