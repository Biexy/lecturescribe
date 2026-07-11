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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptDocument {
    pub schema_version: u16,
    pub item_id: String,
    pub title: String,
    pub source: String,
    pub language: String,
    pub model: String,
    pub generated_at: DateTime<Utc>,
    pub segments: Vec<TranscriptSegment>,
}
