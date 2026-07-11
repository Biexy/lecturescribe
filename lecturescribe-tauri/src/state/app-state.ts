import type {
  AppError,
  AppSettings,
  EnvironmentSnapshot,
  HistoryEntry,
  ItemSnapshot,
  JobSnapshot,
  PreviewItem,
  PreviewSnapshot,
  RunMode,
  RunPlan,
  SourceFileSummary,
} from "../types/contracts";

export type QueueFilter = "all" | "selected" | "ready" | "active" | "done" | "failed";
export type WorkspaceView = "queue" | "history";
export type DialogName = "sources" | "preflight" | "setup" | "settings" | "about" | "summary" | null;

export interface ToastMessage {
  id: string;
  tone: "info" | "success" | "warning" | "error";
  title: string;
  message: string;
  action_label?: string;
  action_id?: string;
}

export interface RemovedSource {
  summary: SourceFileSummary;
  index: number;
}

export interface UiState {
  booting: boolean;
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  sources: SourceFileSummary[];
  removedSource: RemovedSource | null;
  preview: PreviewSnapshot | null;
  previewLoading: boolean;
  previewError: AppError | null;
  selected: Record<string, boolean>;
  mode: RunMode;
  plan: RunPlan | null;
  planLoading: boolean;
  job: JobSnapshot | null;
  history: HistoryEntry[];
  workspaceView: WorkspaceView;
  filter: QueueFilter;
  search: string;
  dialog: DialogName;
  detailItemId: string | null;
  activityExpanded: boolean;
  setupStep: number;
  toasts: ToastMessage[];
}

export const initialState: UiState = {
  booting: true,
  settings: null,
  environment: null,
  sources: [],
  removedSource: null,
  preview: null,
  previewLoading: false,
  previewError: null,
  selected: {},
  mode: "transcribe",
  plan: null,
  planLoading: false,
  job: null,
  history: [],
  workspaceView: "queue",
  filter: "all",
  search: "",
  dialog: null,
  detailItemId: null,
  activityExpanded: false,
  setupStep: 0,
  toasts: [],
};

export type Action =
  | { type: "booted"; settings: AppSettings; environment: EnvironmentSnapshot; sources: SourceFileSummary[]; unfinished: JobSnapshot | null }
  | { type: "boot_failed"; error: AppError }
  | { type: "environment"; environment: EnvironmentSnapshot }
  | { type: "settings"; settings: AppSettings }
  | { type: "add_sources"; sources: SourceFileSummary[] }
  | { type: "remove_source"; id: string }
  | { type: "undo_remove_source" }
  | { type: "clear_sources" }
  | { type: "preview_loading" }
  | { type: "preview_ready"; preview: PreviewSnapshot }
  | { type: "preview_failed"; error: AppError }
  | { type: "toggle_item"; id: string; selected?: boolean }
  | { type: "select_items"; ids: string[]; selected: boolean }
  | { type: "mode"; mode: RunMode }
  | { type: "plan_loading" }
  | { type: "plan_ready"; plan: RunPlan }
  | { type: "plan_clear" }
  | { type: "job"; job: JobSnapshot | null }
  | { type: "history"; history: HistoryEntry[] }
  | { type: "workspace"; view: WorkspaceView }
  | { type: "filter"; filter: QueueFilter }
  | { type: "search"; search: string }
  | { type: "dialog"; dialog: DialogName }
  | { type: "detail"; id: string | null }
  | { type: "activity"; expanded: boolean }
  | { type: "setup_step"; step: number }
  | { type: "toast"; toast: ToastMessage }
  | { type: "dismiss_toast"; id: string };

export function reducer(state: UiState, action: Action): UiState {
  switch (action.type) {
    case "booted":
      return {
        ...state,
        booting: false,
        settings: action.settings,
        environment: action.environment,
        sources: dedupeSources(action.sources),
        job: action.unfinished,
        dialog: action.environment.setup_complete ? null : "setup",
      };
    case "boot_failed":
      return { ...state, booting: false, previewError: action.error };
    case "environment":
      return { ...state, environment: action.environment };
    case "settings":
      return { ...state, settings: action.settings, plan: null };
    case "add_sources":
      return {
        ...state,
        sources: dedupeSources([...state.sources, ...action.sources]),
        removedSource: null,
        plan: null,
      };
    case "remove_source": {
      const index = state.sources.findIndex(({ source }) => source.id === action.id);
      if (index < 0) return state;
      return {
        ...state,
        sources: state.sources.filter(({ source }) => source.id !== action.id),
        removedSource: { summary: state.sources[index], index },
        plan: null,
      };
    }
    case "undo_remove_source": {
      if (!state.removedSource) return state;
      const sources = [...state.sources];
      sources.splice(state.removedSource.index, 0, state.removedSource.summary);
      return { ...state, sources: dedupeSources(sources), removedSource: null };
    }
    case "clear_sources":
      return {
        ...state,
        sources: [],
        preview: null,
        previewError: null,
        selected: {},
        plan: null,
      };
    case "preview_loading":
      return { ...state, previewLoading: true, previewError: null, plan: null };
    case "preview_ready": {
      const selected: Record<string, boolean> = {};
      for (const item of action.preview.items) {
        selected[item.id] = state.selected[item.id] ??
          (item.selected && !item.duplicate_of && !item.error && item.status !== "blocked");
      }
      return {
        ...state,
        preview: action.preview,
        previewLoading: false,
        previewError: null,
        selected,
        plan: null,
      };
    }
    case "preview_failed":
      return { ...state, previewLoading: false, previewError: action.error, plan: null };
    case "toggle_item":
      return {
        ...state,
        selected: {
          ...state.selected,
          [action.id]: action.selected ?? !state.selected[action.id],
        },
        plan: null,
      };
    case "select_items": {
      const selected = { ...state.selected };
      for (const id of action.ids) selected[id] = action.selected;
      return { ...state, selected, plan: null };
    }
    case "mode":
      return { ...state, mode: action.mode, plan: null };
    case "plan_loading":
      return { ...state, planLoading: true, plan: null };
    case "plan_ready":
      return { ...state, planLoading: false, plan: action.plan, dialog: "preflight" };
    case "plan_clear":
      return { ...state, planLoading: false, plan: null };
    case "job":
      return { ...state, job: action.job, plan: null };
    case "history":
      return { ...state, history: action.history };
    case "workspace":
      return { ...state, workspaceView: action.view };
    case "filter":
      return { ...state, filter: action.filter };
    case "search":
      return { ...state, search: action.search };
    case "dialog":
      return { ...state, dialog: action.dialog };
    case "detail":
      return { ...state, detailItemId: action.id };
    case "activity":
      return { ...state, activityExpanded: action.expanded };
    case "setup_step":
      return { ...state, setupStep: Math.max(0, Math.min(5, action.step)) };
    case "toast":
      return { ...state, toasts: [...state.toasts.slice(-3), action.toast] };
    case "dismiss_toast":
      return { ...state, toasts: state.toasts.filter(({ id }) => id !== action.id) };
    default:
      return state;
  }
}

function dedupeSources(sources: SourceFileSummary[]): SourceFileSummary[] {
  const seen = new Set<string>();
  return sources.filter(({ source }) => {
    if (seen.has(source.id)) return false;
    seen.add(source.id);
    return true;
  });
}

export function selectedItemIds(state: UiState): string[] {
  return state.preview?.items
    .filter((item) => state.selected[item.id] && !item.duplicate_of && !item.error)
    .map((item) => item.id) ?? [];
}

export function itemSnapshots(job: JobSnapshot | null): Map<string, ItemSnapshot> {
  return new Map(job?.items.map((snapshot) => [snapshot.item.item.id, snapshot]) ?? []);
}

export function visibleItems(state: UiState): PreviewItem[] {
  const query = state.search.trim().toLocaleLowerCase();
  const snapshots = itemSnapshots(state.job);
  return (state.preview?.items ?? []).filter((item) => {
    const snapshot = snapshots.get(item.id);
    const status = snapshot?.state ?? item.status;
    const matchesQuery = !query || [item.title, item.source, item.expected_media_name ?? ""]
      .some((value) => value.toLocaleLowerCase().includes(query));
    if (!matchesQuery) return false;
    switch (state.filter) {
      case "selected":
        return Boolean(state.selected[item.id]);
      case "ready":
        return status === "ready" || status === "queued";
      case "active":
        return ["downloading", "verifying", "preparing", "segmenting", "transcribing", "validating", "merging", "saving", "waiting"].includes(status);
      case "done":
        return ["complete", "reused"].includes(status);
      case "failed":
        return ["failed", "blocked"].includes(status);
      default:
        return true;
    }
  });
}

export function isJobActive(job: JobSnapshot | null): boolean {
  return Boolean(job && ["planned", "running", "paused", "waiting", "cancelling", "interrupted"].includes(job.state));
}

export function makeToast(
  tone: ToastMessage["tone"],
  title: string,
  message: string,
  action?: Pick<ToastMessage, "action_id" | "action_label">,
): ToastMessage {
  return {
    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    tone,
    title,
    message,
    ...action,
  };
}
