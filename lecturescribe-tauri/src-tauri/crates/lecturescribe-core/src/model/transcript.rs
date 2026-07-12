use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    DownloadedMedia,
    VerifiedMedia,
    NormalizedAudio,
    SegmentManifest,
    SegmentTranscript,
    CanonicalTranscript,
    TextTranscript,
    MarkdownTranscript,
    SrtTranscript,
    VttTranscript,
    Index,
    BatchManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub job_id: String,
    pub item_id: String,
    pub task_id: String,
    pub kind: ArtifactKind,
    pub path: String,
    pub checksum: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
    pub text: String,
    #[serde(default)]
    pub language_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptDocument {
    pub schema_version: u16,
    pub item_id: String,
    pub title: String,
    pub source: String,
    pub language: String,
    #[serde(default)]
    pub languages: Vec<String>,
    pub model: String,
    pub generated_at: DateTime<Utc>,
    pub segments: Vec<TranscriptSegment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_v1_canonical_transcript_remains_readable() {
        let document: TranscriptDocument = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "item_id": "item-1",
                "title": "Legacy transcript",
                "source": "https://example.test/video",
                "language": "en",
                "model": "gemini-3.1-flash-lite",
                "generated_at": "2026-01-01T00:00:00Z",
                "segments": [{"start_seconds": 0.0, "end_seconds": 1.0, "text": "Hello"}]
            }"#,
        )
        .unwrap();

        assert_eq!(document.schema_version, 1);
        assert!(document.languages.is_empty());
        assert_eq!(document.segments[0].language_code, None);
    }
}
