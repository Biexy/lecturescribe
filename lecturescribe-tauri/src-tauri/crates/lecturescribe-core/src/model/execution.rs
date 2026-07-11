use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{AppError, AppSettings, ArtifactRecord, PreviewItem, ProgressMetric, RunMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannedAction {
    DownloadAndTranscribe,
    ReuseMediaAndTranscribe,
    TranscribeLocal,
    ReuseTranscript,
    DownloadOnly,
    Excluded,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRequest {
    pub preview_id: String,
    pub selected_item_ids: Vec<String>,
    pub mode: RunMode,
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedItem {
    pub item: PreviewItem,
    pub ordinal: usize,
    pub action: PlannedAction,
    pub reason: String,
    pub estimated_segments: usize,
    pub estimated_requests: usize,
    pub tasks: Vec<TaskSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlan {
    pub id: String,
    pub preview_id: String,
    pub created_at: DateTime<Utc>,
    pub mode: RunMode,
    pub settings: AppSettings,
    pub items: Vec<PlannedItem>,
    pub selected_count: usize,
    pub runnable_count: usize,
    pub excluded_count: usize,
    pub blocked_count: usize,
    pub estimated_requests: usize,
    pub blocking_errors: Vec<AppError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Inspect,
    Download,
    Verify,
    Prepare,
    Segment,
    Transcribe,
    Validate,
    Merge,
    Save,
    Reuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    Metadata,
    Download,
    Filesystem,
    Ffmpeg,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub item_id: String,
    pub kind: TaskKind,
    pub resource: ResourceClass,
    pub depends_on: Vec<String>,
    pub idempotency_key: String,
    pub max_attempts: u32,
    pub weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Planned,
    Running,
    Paused,
    Waiting,
    Cancelling,
    Complete,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Inspecting,
    Ready,
    Queued,
    Downloading,
    Verifying,
    Preparing,
    Segmenting,
    Transcribing,
    Validating,
    Merging,
    Saving,
    Waiting,
    Reused,
    Complete,
    Failed,
    Cancelled,
    Excluded,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Ready,
    Running,
    Waiting,
    Paused,
    Succeeded,
    Reused,
    Skipped,
    Failed,
    Cancelled,
    Interrupted,
}

impl TaskState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Reused | Self::Skipped | Self::Failed | Self::Cancelled
        )
    }

    pub fn successful(self) -> bool {
        matches!(self, Self::Succeeded | Self::Reused | Self::Skipped)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    Complete,
    Reused,
    Skipped,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub item_id: String,
    pub kind: TaskKind,
    pub resource: ResourceClass,
    pub state: TaskState,
    pub depends_on: Vec<String>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub weight: f64,
    pub progress: Option<ProgressMetric>,
    pub message: String,
    pub error: Option<AppError>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSnapshot {
    pub item: PlannedItem,
    pub state: ItemState,
    pub outcome: Option<TerminalOutcome>,
    pub tasks: Vec<TaskSnapshot>,
    pub progress: ProgressMetric,
    pub message: String,
    pub error: Option<AppError>,
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobCounts {
    pub planned: usize,
    pub running: usize,
    pub complete: usize,
    pub reused: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub id: String,
    pub plan_id: String,
    pub state: JobState,
    pub sequence: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub items: Vec<ItemSnapshot>,
    pub counts: JobCounts,
    pub overall_progress: ProgressMetric,
    pub current_item_id: Option<String>,
    pub current_task_id: Option<String>,
    pub message: String,
    pub summary: Option<RunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub job_id: String,
    pub outcome: JobState,
    pub counts: JobCounts,
    pub output_dir: String,
    pub downloaded_media: usize,
    pub saved_transcripts: usize,
    pub gemini_requests: usize,
    pub processed_seconds: f64,
    pub elapsed_seconds: u64,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub job_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub mode: RunMode,
    pub title: String,
    pub counts: JobCounts,
    pub output_dir: String,
    pub state: JobState,
}
