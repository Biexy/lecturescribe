use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Input,
    Setup,
    Network,
    Authentication,
    Quota,
    Download,
    Media,
    Transcription,
    Filesystem,
    Database,
    Cancelled,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub id: String,
    pub label: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{user_message}")]
pub struct AppError {
    pub code: String,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub user_message: String,
    pub technical_detail: String,
    pub retryable: bool,
    pub preserved_work: String,
    pub recovery_actions: Vec<RecoveryAction>,
}

impl AppError {
    pub fn new(
        code: impl Into<String>,
        category: ErrorCategory,
        user_message: impl Into<String>,
        technical_detail: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            category,
            severity: ErrorSeverity::Error,
            user_message: user_message.into(),
            technical_detail: technical_detail.into(),
            retryable: false,
            preserved_work: String::new(),
            recovery_actions: Vec::new(),
        }
    }

    pub fn retryable(mut self, preserved_work: impl Into<String>) -> Self {
        self.retryable = true;
        self.preserved_work = preserved_work.into();
        self
    }

    pub fn with_action(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        self.recovery_actions.push(RecoveryAction {
            id: id.into(),
            label: label.into(),
            action: action.into(),
        });
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Indeterminate,
    Fraction,
    Bytes,
    Duration,
    Segments,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMetric {
    pub kind: ProgressKind,
    pub current: f64,
    pub total: Option<f64>,
    pub unit: String,
    pub rate: Option<f64>,
    pub eta_seconds: Option<u64>,
}

impl ProgressMetric {
    pub fn indeterminate(unit: impl Into<String>) -> Self {
        Self {
            kind: ProgressKind::Indeterminate,
            current: 0.0,
            total: None,
            unit: unit.into(),
            rate: None,
            eta_seconds: None,
        }
    }

    pub fn fraction(current: f64, total: f64, unit: impl Into<String>) -> Self {
        Self {
            kind: ProgressKind::Fraction,
            current,
            total: Some(total.max(0.0)),
            unit: unit.into(),
            rate: None,
            eta_seconds: None,
        }
    }

    pub fn percent(&self) -> Option<f64> {
        self.total
            .filter(|total| *total > 0.0)
            .map(|total| (self.current / total * 100.0).clamp(0.0, 100.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    JobState,
    ItemState,
    TaskState,
    Progress,
    Artifact,
    Problem,
    Summary,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEvent {
    pub schema_version: u16,
    pub sequence: i64,
    pub occurred_at: DateTime<Utc>,
    pub job_id: String,
    pub item_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: EventType,
    pub state: Option<String>,
    pub progress: Option<ProgressMetric>,
    pub attempt: Option<u32>,
    pub message: String,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolReadiness {
    Ready,
    Missing,
    Outdated,
    Invalid,
    Unverified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatus {
    pub name: String,
    pub readiness: ToolReadiness,
    pub version: Option<String>,
    pub path: Option<String>,
    pub detail: String,
    pub fix_action: Option<String>,
}

impl ToolStatus {
    pub fn ready(&self) -> bool {
        self.readiness == ToolReadiness::Ready
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSnapshot {
    pub api_key_configured: bool,
    pub api_key_verified: bool,
    pub ffmpeg: ToolStatus,
    pub ffprobe: ToolStatus,
    pub downloader: ToolStatus,
    pub output_writable: bool,
    pub free_disk_bytes: Option<u64>,
    pub database_ok: bool,
    pub network_online: Option<bool>,
    pub app_version: String,
    pub setup_complete: bool,
    pub problems: Vec<AppError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupTestResult {
    pub ok: bool,
    pub message: String,
    pub model: String,
    pub transcript_preview: String,
}
