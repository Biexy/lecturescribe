use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{AppError, ItemState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    PastedLink,
    TextFile,
    LocalMedia,
    Directory,
    AutomaticFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Local,
    YouTube,
    GoogleDrive,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInput {
    #[serde(default)]
    pub id: String,
    pub kind: SourceKind,
    pub value: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub automatic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectSourcesRequest {
    pub sources: Vec<SourceInput>,
    #[serde(default)]
    pub confirm_large_playlists: bool,
    #[serde(default = "default_playlist_limit")]
    pub playlist_limit: usize,
}

fn default_playlist_limit() -> usize {
    200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewItem {
    pub id: String,
    pub source_id: String,
    pub source_kind: SourceKind,
    pub provider: ProviderKind,
    pub source_group: String,
    pub title: String,
    pub source: String,
    pub canonical_source: String,
    pub url: Option<String>,
    pub media_path: Option<String>,
    pub existing_media_path: Option<String>,
    pub existing_transcript_path: Option<String>,
    pub thumbnail_url: Option<String>,
    pub duration_seconds: Option<f64>,
    pub expected_media_name: Option<String>,
    pub selected: bool,
    pub status: ItemState,
    pub duplicate_of: Option<String>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSnapshot {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub items: Vec<PreviewItem>,
    pub duplicate_count: usize,
    pub source_count: usize,
    pub warnings: Vec<String>,
}
