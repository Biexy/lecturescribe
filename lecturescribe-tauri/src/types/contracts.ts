export type Theme = "light" | "dark";
export type RunMode = "transcribe" | "download";
export type TranscriptFormat = "text" | "markdown" | "srt" | "vtt";
export type OutputPackage = "readable" | "subtitles" | "complete" | "custom";

export interface LanguagePreferences {
  mode: "auto" | "hints";
  hints: string[];
}

export interface ModelOption {
  id: string;
  display_name: string;
  description: string;
  recommended: boolean;
  quality_label: string;
}

export interface ModelValidation {
  model_id: string;
  availability: "available" | "unavailable" | "unknown";
  status: "valid" | "invalid" | "unverified";
  message: string;
  checked_at: string | null;
}

export interface RunOverrides {
  batch_name: string | null;
  model_id: string | null;
}
export type SourceKind =
  | "pasted_link"
  | "text_file"
  | "local_media"
  | "directory"
  | "automatic_file";
export type ProviderKind = "local" | "you_tube" | "google_drive" | "generic";
export type ItemState =
  | "inspecting"
  | "ready"
  | "queued"
  | "downloading"
  | "verifying"
  | "preparing"
  | "segmenting"
  | "transcribing"
  | "validating"
  | "merging"
  | "saving"
  | "waiting"
  | "reused"
  | "complete"
  | "failed"
  | "cancelled"
  | "excluded"
  | "blocked";
export type JobState =
  | "planned"
  | "running"
  | "paused"
  | "waiting"
  | "cancelling"
  | "complete"
  | "failed"
  | "cancelled"
  | "interrupted";
export type TaskState =
  | "pending"
  | "ready"
  | "running"
  | "waiting"
  | "paused"
  | "succeeded"
  | "reused"
  | "skipped"
  | "failed"
  | "cancelled"
  | "interrupted";
export type TaskKind =
  | "inspect"
  | "download"
  | "verify"
  | "prepare"
  | "segment"
  | "transcribe"
  | "validate"
  | "merge"
  | "save"
  | "reuse";
export type ArtifactKind =
  | "downloaded_media"
  | "verified_media"
  | "normalized_audio"
  | "segment_manifest"
  | "segment_transcript"
  | "canonical_transcript"
  | "text_transcript"
  | "markdown_transcript"
  | "srt_transcript"
  | "vtt_transcript"
  | "index"
  | "batch_manifest";

export interface RecoveryAction {
  id: string;
  label: string;
  action: string;
}

export interface AppError {
  code: string;
  category: string;
  severity: "info" | "warning" | "error" | "fatal";
  user_message: string;
  technical_detail: string;
  retryable: boolean;
  preserved_work: string;
  recovery_actions: RecoveryAction[];
}

export interface AppSettings {
  output_dir: string;
  download_dir: string;
  work_dir: string;
  model: string;
  theme: Theme;
  output_formats: TranscriptFormat[];
  language: LanguagePreferences;
  output_package: OutputPackage;
  prompt_preset: string;
  additional_prompt: string;
  ffmpeg_path: string;
  ffprobe_path: string;
  downloader_path: string;
  cookies_from_browser: string;
  cookies_file: string;
  keep_downloaded_media: boolean;
  force: boolean;
  segment_minutes: number;
  overlap_seconds: number;
  request_delay_ms: number;
  cache_limit_gib: number;
  cache_max_age_days: number;
  update_channel: string;
}

export interface SourceInput {
  id: string;
  kind: SourceKind;
  value: string;
  label: string;
  automatic: boolean;
}

export interface SourceFileSummary {
  source: SourceInput;
  link_count: number;
}

export interface PreviewItem {
  id: string;
  source_id: string;
  source_kind: SourceKind;
  provider: ProviderKind;
  source_group: string;
  title: string;
  source: string;
  canonical_source: string;
  url: string | null;
  media_path: string | null;
  existing_media_path: string | null;
  existing_transcript_path: string | null;
  thumbnail_url: string | null;
  duration_seconds: number | null;
  expected_media_name: string | null;
  selected: boolean;
  status: ItemState;
  duplicate_of: string | null;
  error: AppError | null;
}

export interface PreviewSnapshot {
  id: string;
  created_at: string;
  items: PreviewItem[];
  duplicate_count: number;
  source_count: number;
  warnings: string[];
}

export type PlannedAction =
  | "download_and_transcribe"
  | "reuse_media_and_transcribe"
  | "transcribe_local"
  | "reuse_transcript"
  | "download_only"
  | "excluded"
  | "blocked";

export interface TaskSpec {
  id: string;
  item_id: string;
  kind: TaskKind;
  resource: string;
  depends_on: string[];
  idempotency_key: string;
  max_attempts: number;
  weight: number;
}

export interface PlannedItem {
  item: PreviewItem;
  output_stem: string;
  ordinal: number;
  action: PlannedAction;
  reason: string;
  estimated_segments: number;
  estimated_requests: number;
  tasks: TaskSpec[];
}

export interface RunPlan {
  id: string;
  preview_id: string;
  created_at: string;
  mode: RunMode;
  batch_name: string;
  batch_output_dir: string;
  settings: AppSettings;
  items: PlannedItem[];
  selected_count: number;
  runnable_count: number;
  excluded_count: number;
  blocked_count: number;
  estimated_requests: number;
  blocking_errors: AppError[];
}

export interface ProgressMetric {
  kind: "indeterminate" | "fraction" | "bytes" | "duration" | "segments";
  current: number;
  total: number | null;
  unit: string;
  rate: number | null;
  eta_seconds: number | null;
}

export interface ArtifactRecord {
  id: string;
  job_id: string;
  item_id: string;
  kind: ArtifactKind;
  path: string;
  checksum: string;
  size_bytes: number;
  created_at: string;
  metadata: Record<string, string>;
}

export interface TranscriptSegment {
  start_seconds: number;
  end_seconds: number | null;
  text: string;
  language_code: string | null;
}

export interface SegmentTranscript {
  language: string;
  segments: TranscriptSegment[];
}

export interface TranscriptDocument {
  schema_version: number;
  item_id: string;
  title: string;
  source: string;
  language: string;
  languages: string[];
  model: string;
  generated_at: string;
  segments: TranscriptSegment[];
}

export interface TaskSnapshot {
  id: string;
  item_id: string;
  kind: TaskKind;
  resource: string;
  state: TaskState;
  depends_on: string[];
  attempt: number;
  max_attempts: number;
  weight: number;
  progress: ProgressMetric | null;
  message: string;
  error: AppError | null;
  started_at: string | null;
  finished_at: string | null;
}

export interface ItemSnapshot {
  item: PlannedItem;
  state: ItemState;
  outcome: "complete" | "reused" | "skipped" | "failed" | "cancelled" | null;
  tasks: TaskSnapshot[];
  progress: ProgressMetric;
  message: string;
  error: AppError | null;
  artifacts: ArtifactRecord[];
}

export interface JobCounts {
  planned: number;
  running: number;
  complete: number;
  reused: number;
  skipped: number;
  failed: number;
  cancelled: number;
}

export interface RunSummary {
  job_id: string;
  outcome: JobState;
  counts: JobCounts;
  output_dir: string;
  downloaded_media: number;
  saved_transcripts: number;
  gemini_requests: number;
  processed_seconds: number;
  elapsed_seconds: number;
  completed_at: string;
}

export interface JobSnapshot {
  id: string;
  plan_id: string;
  state: JobState;
  sequence: number;
  started_at: string;
  finished_at: string | null;
  items: ItemSnapshot[];
  counts: JobCounts;
  overall_progress: ProgressMetric;
  current_item_id: string | null;
  current_task_id: string | null;
  message: string;
  summary: RunSummary | null;
}

export interface HistoryEntry {
  job_id: string;
  started_at: string;
  completed_at: string | null;
  mode: RunMode;
  title: string;
  counts: JobCounts;
  output_dir: string;
  state: JobState;
}

export interface ToolStatus {
  name: string;
  readiness: "ready" | "missing" | "outdated" | "invalid" | "unverified";
  version: string | null;
  path: string | null;
  detail: string;
  fix_action: string | null;
}

export interface CapabilityStatus {
  ready: boolean;
  blockers: AppError[];
}

export interface SetupCapabilities {
  download_links: CapabilityStatus;
  transcribe_local: CapabilityStatus;
  transcribe_links: CapabilityStatus;
}

export type SetupCapability = keyof SetupCapabilities;

export interface EnvironmentSnapshot {
  api_key_configured: boolean;
  api_key_verified: boolean;
  ffmpeg: ToolStatus;
  ffprobe: ToolStatus;
  downloader: ToolStatus;
  output_writable: boolean;
  free_disk_bytes: number | null;
  database_ok: boolean;
  network_online: boolean | null;
  app_version: string;
  capabilities: SetupCapabilities;
  setup_complete: boolean;
  problems: AppError[];
}

export interface AppEvent {
  schema_version: number;
  sequence: number;
  occurred_at: string;
  job_id: string;
  item_id: string | null;
  task_id: string | null;
  event_type: string;
  state: string | null;
  progress: ProgressMetric | null;
  attempt: number | null;
  message: string;
  error: AppError | null;
}

export interface StartPlanResponse {
  job_id: string;
  snapshot: JobSnapshot;
}

export interface SetupTestResult {
  ok: boolean;
  message: string;
  model: string;
  transcript_preview: string;
}
