import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  AppEvent,
  AppSettings,
  ArtifactKind,
  EnvironmentSnapshot,
  HistoryEntry,
  JobSnapshot,
  PreviewSnapshot,
  RunMode,
  RunPlan,
  SetupTestResult,
  SourceFileSummary,
  SourceInput,
  StartPlanResponse,
  ToolStatus,
} from "../types/contracts";

export interface PlanRequest {
  preview_id: string;
  selected_item_ids: string[];
  mode: RunMode;
  settings: AppSettings;
}

export function normalizeError(error: unknown): AppError {
  if (typeof error === "object" && error && "user_message" in error) {
    return error as AppError;
  }
  const message = error instanceof Error ? error.message : String(error);
  return {
    code: "frontend_command_failed",
    category: "internal",
    severity: "error",
    user_message: message || "LectureScribe could not complete that action.",
    technical_detail: message,
    retryable: false,
    preserved_work: "",
    recovery_actions: [],
  };
}

export const backend = {
  discoverAutomaticSources: () =>
    invoke<SourceFileSummary[]>("discover_automatic_sources"),

  inspectLinkFile: (path: string) =>
    invoke<SourceFileSummary>("inspect_link_file", { path }),

  inspectSources: (sources: SourceInput[], confirmLargePlaylists = false) =>
    invoke<PreviewSnapshot>("inspect_sources", {
      request: {
        sources,
        confirm_large_playlists: confirmLargePlaylists,
        playlist_limit: 200,
      },
    }),

  buildPlan: (request: PlanRequest) =>
    invoke<RunPlan>("build_plan", { request }),

  startPlan: (planId: string) =>
    invoke<StartPlanResponse>("start_plan", { planId }),

  pauseJob: (jobId: string) => invoke<void>("pause_job", { jobId }),
  resumeJob: (jobId: string) => invoke<JobSnapshot>("resume_job", { jobId }),
  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  retryItems: (jobId: string) =>
    invoke<{ job_id: string; reset_items: number; snapshot: JobSnapshot }>("retry_items", {
      jobId,
    }),

  getJobSnapshot: (jobId: string) =>
    invoke<JobSnapshot>("get_job_snapshot", { jobId }),

  eventsSince: (jobId: string, sequence: number) =>
    invoke<AppEvent[]>("events_since", { jobId, sequence }),

  listHistory: (limit = 50) =>
    invoke<HistoryEntry[]>("list_history", { limit }),

  unfinishedJobs: () => invoke<JobSnapshot[]>("unfinished_jobs"),

  loadSettings: () => invoke<AppSettings>("load_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),
  saveApiKey: (apiKey: string) => invoke<void>("save_api_key", { apiKey }),
  deleteApiKey: () => invoke<void>("delete_api_key"),
  checkEnvironment: () => invoke<EnvironmentSnapshot>("check_environment"),
  installDownloader: () => invoke<ToolStatus>("install_downloader"),
  runSetupTest: () => invoke<SetupTestResult>("run_setup_test"),

  openOutputFolder: () => invoke<string>("open_output_folder"),
  openKnownLink: (target: "ai_studio" | "github" | "ffmpeg" | "yt_dlp") =>
    invoke<string>("open_known_link", { target }),
  openArtifact: (
    jobId: string,
    itemId: string,
    kind: ArtifactKind,
    reveal = false,
  ) => invoke<string>("open_artifact", { jobId, itemId, kind, reveal }),
  previewDiagnostics: () => invoke<unknown>("preview_diagnostics"),
  exportDiagnostics: (destination: string) =>
    invoke<{ path: string; report: unknown }>("export_diagnostics", { destination }),

  onEvent: (handler: (event: AppEvent) => void): Promise<UnlistenFn> =>
    listen<AppEvent>("lecturescribe-event", ({ payload }) => handler(payload)),
};

export function selectedTranscriptArtifact(formats: AppSettings["output_formats"]): ArtifactKind {
  if (formats.includes("markdown")) return "markdown_transcript";
  if (formats.includes("text")) return "text_transcript";
  if (formats.includes("srt")) return "srt_transcript";
  return "vtt_transcript";
}

export function progressPercent(snapshot: JobSnapshot | null): number {
  if (!snapshot) return 0;
  const { current, total } = snapshot.overall_progress;
  if (!total || total <= 0) return snapshot.state === "complete" ? 100 : 0;
  return Math.max(0, Math.min(100, (current / total) * 100));
}

export function formatBytes(value: number | null | undefined): string {
  if (!value || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return `${(value / 1024 ** index).toFixed(index > 1 ? 1 : 0)} ${units[index]}`;
}

export function formatDuration(seconds: number | null | undefined): string {
  if (!seconds || seconds <= 0) return "--";
  const rounded = Math.round(seconds);
  const hours = Math.floor(rounded / 3600);
  const minutes = Math.floor((rounded % 3600) / 60);
  const remainder = rounded % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`
    : `${minutes}:${String(remainder).padStart(2, "0")}`;
}
