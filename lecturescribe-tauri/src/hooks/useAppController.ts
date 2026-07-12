import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useReducer, useRef, useState } from "react";
import { backend, normalizeError, type PlanRequest } from "../lib/backend";
import {
  initialState,
  isJobActive,
  makeToast,
  reducer,
  selectedItemIds,
  type QueueFilter,
  type ToastMessage,
  type WorkspaceView,
} from "../state/app-state";
import type {
  AppError,
  AppSettings,
  ArtifactKind,
  HistoryEntry,
  ModelOption,
  ModelValidation,
  RunMode,
  RunOverrides,
  SetupTestResult,
  SourceFileSummary,
  SourceInput,
  SourceKind,
} from "../types/contracts";
import { countLinks } from "../components/dialogs/PasteLinksModal";
import { CURATED_MODEL_OPTIONS } from "../components/settings/ModelChoice";
import {
  humanizePreviewWarnings,
  playlistConfirmationMessage,
  playlistConfirmations,
} from "../lib/preview";

const MEDIA_EXTENSIONS = new Set(["mp3", "m4a", "mp4", "webm", "wav", "aac", "flac", "ogg", "opus", "mov", "mkv"]);

export function useAppController() {
  const [state, dispatch] = useReducer(reducer, initialState);
  const [pastedText, setPastedText] = useState("");
  const [previewNonce, setPreviewNonce] = useState(0);
  const [starting, setStarting] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [setupBusy, setSetupBusy] = useState<string | null>(null);
  const [setupTest, setSetupTest] = useState<SetupTestResult | null>(null);
  const [setupError, setSetupError] = useState<AppError | null>(null);
  const [modelOptions, setModelOptions] = useState<ModelOption[]>(CURATED_MODEL_OPTIONS);
  const [modelValidation, setModelValidation] = useState<ModelValidation | null>(null);
  const [modelBusy, setModelBusy] = useState(false);
  const previewRequest = useRef(0);
  const jobRef = useRef(state.job);
  const snapshotTimer = useRef<number | null>(null);
  const completedJob = useRef<string | null>(null);
  const modelCatalogLoaded = useRef(false);

  useEffect(() => {
    jobRef.current = state.job;
  }, [state.job]);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const [settings, environment, automatic, unfinished, history] = await Promise.all([
          backend.loadSettings(),
          backend.checkEnvironment(),
          backend.discoverAutomaticSources(),
          backend.unfinishedJobs(),
          backend.listHistory(),
        ]);
        if (!alive) return;
        dispatch({
          type: "booted",
          settings,
          environment,
          sources: automatic,
          unfinished: unfinished[0] ?? null,
        });
        if (!environment.setup_complete && !setupIntroSeen()) {
          dispatch({ type: "dialog", dialog: "setup" });
        }
        dispatch({ type: "history", history });
        if (automatic.length > 0) {
          dispatch({
            type: "toast",
            toast: makeToast(
              "info",
              "Automatic source found",
              `${automatic.reduce((sum, item) => sum + item.link_count, 0)} links loaded from ${automatic.map((item) => item.source.label).join(", ")}.`,
            ),
          });
        }
      } catch (error) {
        if (!alive) return;
        const problem = normalizeError(error);
        dispatch({ type: "boot_failed", error: problem });
        dispatch({ type: "toast", toast: errorToast(problem) });
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!state.settings) return;
    document.documentElement.dataset.theme = state.settings.theme;
    document.documentElement.style.colorScheme = state.settings.theme;
  }, [state.settings?.theme]);

  useEffect(() => {
    if (
      state.dialog !== "settings"
      || !state.environment?.api_key_configured
      || modelCatalogLoaded.current
    ) return;
    void loadTranscriptionModels(state.settings?.model ?? null);
  }, [state.dialog, state.environment?.api_key_configured, state.settings?.model]);

  useEffect(() => {
    if (state.booting || state.sources.length === 0) return;
    const requestId = ++previewRequest.current;
    const timer = window.setTimeout(() => {
      dispatch({ type: "preview_loading" });
      void backend.inspectSources(state.sources.map(({ source }) => source))
        .then(async (preview) => {
          if (previewRequest.current !== requestId) return;
          const confirmations = playlistConfirmations(preview.warnings);
          if (confirmations.length > 0) {
            const confirmed = window.confirm(playlistConfirmationMessage(confirmations));
            if (previewRequest.current !== requestId) return;
            if (confirmed) {
              preview = await backend.inspectSources(
                state.sources.map(({ source }) => source),
                true,
              );
            } else {
              preview = { ...preview, warnings: humanizePreviewWarnings(preview.warnings) };
            }
          }
          if (previewRequest.current === requestId) dispatch({ type: "preview_ready", preview });
        })
        .catch((error) => {
          if (previewRequest.current !== requestId) return;
          const problem = normalizeError(error);
          dispatch({ type: "preview_failed", error: problem });
          dispatch({ type: "toast", toast: errorToast(problem) });
        });
    }, 300);
    return () => window.clearTimeout(timer);
  }, [state.sources, state.booting, previewNonce]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void backend.onEvent((event) => {
      const active = jobRef.current;
      if (!active || active.id !== event.job_id || event.sequence <= active.sequence) return;
      if (snapshotTimer.current !== null) return;
      snapshotTimer.current = window.setTimeout(() => {
        snapshotTimer.current = null;
        void refreshJob(event.job_id);
      }, 100);
    }).then((dispose) => {
      unlisten = dispose;
    });
    return () => {
      unlisten?.();
      if (snapshotTimer.current !== null) window.clearTimeout(snapshotTimer.current);
    };
  }, []);

  useEffect(() => {
    const job = state.job;
    if (!job || !isJobActive(job)) return;
    const timer = window.setInterval(() => void refreshJob(job.id), 1000);
    return () => window.clearInterval(timer);
  }, [state.job?.id, state.job?.state]);

  useEffect(() => {
    const job = state.job;
    if (!job?.summary || isJobActive(job) || completedJob.current === job.id) return;
    completedJob.current = job.id;
    const failed = job.summary.counts.failed;
    dispatch({ type: "dialog", dialog: "summary" });
    dispatch({
      type: "toast",
      toast: makeToast(
        failed ? "warning" : job.state === "cancelled" ? "info" : "success",
        failed ? "Run finished with issues" : job.state === "cancelled" ? "Run cancelled safely" : "Run complete",
        failed
          ? `${failed} item${failed === 1 ? "" : "s"} failed. Successful work was kept.`
          : `${job.summary.saved_transcripts || job.summary.downloaded_media} outputs saved.`,
        { action_id: "open_output", action_label: "Open output" },
      ),
    });
    void backend.listHistory().then((history) => dispatch({ type: "history", history }));
  }, [state.job?.state, state.job?.summary]);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    try {
      void getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "drop") void ingestPaths(event.payload.paths);
      }).then((unlisten) => {
        dispose = unlisten;
      });
    } catch {
      // Browser previews do not expose the Tauri drag-and-drop API.
    }
    return () => dispose?.();
  }, []);

  async function refreshJob(jobId: string) {
    try {
      const snapshot = await backend.getJobSnapshot(jobId);
      dispatch({ type: "job", job: snapshot });
    } catch (error) {
      dispatch({ type: "toast", toast: errorToast(normalizeError(error)) });
    }
  }

  function notify(error: unknown) {
    dispatch({ type: "toast", toast: errorToast(normalizeError(error)) });
  }

  async function addTextFiles() {
    try {
      const result = await open({ multiple: true, filters: [{ name: "Link files", extensions: ["txt"] }] });
      const paths = normalizePaths(result);
      if (paths.length === 0) return;
      const summaries = await Promise.all(paths.map(backend.inspectLinkFile));
      dispatch({ type: "add_sources", sources: summaries });
      const count = summaries.reduce((sum, item) => sum + item.link_count, 0);
      dispatch({ type: "toast", toast: makeToast(count ? "success" : "warning", count ? "Link file added" : "No links found", `${count} link${count === 1 ? "" : "s"} detected in ${summaries.length} file${summaries.length === 1 ? "" : "s"}.`) });
    } catch (error) {
      notify(error);
    }
  }

  async function addMedia() {
    try {
      const result = await open({
        multiple: true,
        filters: [{ name: "Audio and video", extensions: [...MEDIA_EXTENSIONS] }],
      });
      const paths = normalizePaths(result);
      if (paths.length === 0) return;
      addMediaPaths(paths, "local_media");
    } catch (error) {
      notify(error);
    }
  }

  async function addFolder() {
    try {
      const result = await open({ directory: true, multiple: true, recursive: true });
      const paths = normalizePaths(result);
      if (paths.length === 0) return;
      addMediaPaths(paths, "directory");
    } catch (error) {
      notify(error);
    }
  }

  async function ingestPaths(paths: string[]) {
    const textFiles = paths.filter((path) => extension(path) === "txt");
    const media = paths.filter((path) => MEDIA_EXTENSIONS.has(extension(path)));
    const directories = paths.filter((path) => extension(path) === "");
    try {
      if (textFiles.length) {
        const summaries = await Promise.all(textFiles.map(backend.inspectLinkFile));
        dispatch({ type: "add_sources", sources: summaries });
      }
      if (media.length) addMediaPaths(media, "local_media");
      if (directories.length) addMediaPaths(directories, "directory");
      const accepted = textFiles.length + media.length + directories.length;
      if (accepted === 0) {
        dispatch({ type: "toast", toast: makeToast("warning", "Unsupported files", "Drop a supported audio/video file, folder, or .txt link list.") });
      }
    } catch (error) {
      notify(error);
    }
  }

  function addMediaPaths(paths: string[], kind: SourceKind) {
    const summaries = paths.map((path): SourceFileSummary => ({
      source: {
        id: sourceId(kind, path),
        kind,
        value: path,
        label: fileName(path),
        automatic: false,
      },
      link_count: 0,
    }));
    dispatch({ type: "add_sources", sources: summaries });
    dispatch({ type: "toast", toast: makeToast("success", "Media added", `${paths.length} ${kind === "directory" ? "folder" : "file"}${paths.length === 1 ? "" : "s"} added to automatic preview.`) });
  }

  function addPastedLinks() {
    const count = countLinks(pastedText);
    if (count === 0) return;
    const source: SourceInput = {
      id: `paste-${crypto.randomUUID()}`,
      kind: "pasted_link",
      value: pastedText,
      label: count === 1 ? "Pasted link" : `${count} pasted links`,
      automatic: false,
    };
    dispatch({ type: "add_sources", sources: [{ source, link_count: count }] });
    dispatch({ type: "dialog", dialog: null });
    setPastedText("");
  }

  function removeSource(id: string) {
    const source = state.sources.find((item) => item.source.id === id);
    dispatch({ type: "remove_source", id });
    if (source) {
      dispatch({
        type: "toast",
        toast: makeToast("info", "Source removed", source.source.label, {
          action_id: "undo_source",
          action_label: "Undo",
        }),
      });
    }
  }

  function clearSources() {
    if (!window.confirm("Clear all source groups from this queue? Saved outputs are not deleted.")) return;
    dispatch({ type: "clear_sources" });
  }

  async function reviewPlan(overrides: RunOverrides = { batch_name: null, model_id: null }) {
    if (!state.preview || !state.settings) return;
    const ids = selectedItemIds(state);
    if (ids.length === 0) {
      dispatch({ type: "toast", toast: makeToast("warning", "Nothing selected", "Select at least one ready queue item.") });
      return;
    }
    const previousPlan = state.plan;
    dispatch({ type: "plan_loading" });
    const request: PlanRequest = {
      preview_id: state.preview.id,
      selected_item_ids: ids,
      mode: state.mode,
      settings: state.settings,
      overrides,
    };
    try {
      dispatch({ type: "plan_ready", plan: await backend.buildPlan(request) });
    } catch (error) {
      if (previousPlan) dispatch({ type: "plan_ready", plan: previousPlan });
      else dispatch({ type: "plan_clear" });
      notify(error);
    }
  }

  async function startPlan() {
    if (!state.plan) return;
    setStarting(true);
    try {
      const response = await backend.startPlan(state.plan.id);
      completedJob.current = null;
      dispatch({ type: "job", job: response.snapshot });
      dispatch({ type: "dialog", dialog: null });
      dispatch({ type: "toast", toast: makeToast("info", "Run started", `${response.snapshot.counts.planned} selected items are queued.`) });
    } catch (error) {
      notify(error);
    } finally {
      setStarting(false);
    }
  }

  async function pauseJob() {
    if (!state.job) return;
    try {
      await backend.pauseJob(state.job.id);
      dispatch({ type: "toast", toast: makeToast("info", "Pause requested", "Current work will pause at the next safe point.") });
    } catch (error) { notify(error); }
  }

  async function resumeJob() {
    if (!state.job) return;
    try { dispatch({ type: "job", job: await backend.resumeJob(state.job.id) }); } catch (error) { notify(error); }
  }

  async function cancelJob() {
    if (!state.job || !window.confirm("Cancel this run at the next safe point? Completed work will remain cached.")) return;
    try {
      await backend.cancelJob(state.job.id);
      dispatch({ type: "toast", toast: makeToast("warning", "Cancellation requested", "LectureScribe is stopping active work safely.") });
    } catch (error) { notify(error); }
  }

  async function retryFailed() {
    if (!state.job) return;
    try {
      const result = await backend.retryItems(state.job.id);
      completedJob.current = null;
      dispatch({ type: "job", job: result.snapshot });
      dispatch({ type: "dialog", dialog: null });
      dispatch({ type: "toast", toast: makeToast("info", "Retry started", `${result.reset_items} failed item${result.reset_items === 1 ? "" : "s"} queued. Verified cache will be reused.`) });
    } catch (error) { notify(error); }
  }

  async function saveSettings(settings: AppSettings) {
    setSettingsSaving(true);
    try {
      const saved = await backend.saveSettings(settings);
      dispatch({ type: "settings", settings: saved });
      dispatch({ type: "dialog", dialog: null });
      dispatch({ type: "toast", toast: makeToast("success", "Settings saved", "New plans will use these settings.") });
      await refreshEnvironment();
    } catch (error) { notify(error); } finally { setSettingsSaving(false); }
  }

  async function chooseOutput() {
    if (!state.settings) return;
    try {
      const result = await open({ directory: true, multiple: false });
      const path = normalizePaths(result)[0];
      if (!path) return;
      const saved = await backend.saveSettings({ ...state.settings, output_dir: path });
      dispatch({ type: "settings", settings: saved });
      await refreshEnvironment();
    } catch (error) { notify(error); }
  }

  async function chooseFfmpeg() {
    if (!state.settings) return;
    try {
      const result = await open({ multiple: false, filters: [{ name: "FFmpeg", extensions: ["exe"] }] });
      const path = normalizePaths(result)[0];
      if (!path) return;
      const ffprobe = path.replace(/ffmpeg\.exe$/i, "ffprobe.exe");
      const saved = await backend.saveSettings({ ...state.settings, ffmpeg_path: path, ffprobe_path: ffprobe });
      dispatch({ type: "settings", settings: saved });
      await refreshEnvironment();
    } catch (error) { notify(error); }
  }

  async function saveApiKey(apiKey: string) {
    setSetupBusy("key");
    setSetupError(null);
    setSetupTest(null);
    try {
      await backend.saveApiKey(apiKey);
      const validation = await backend.validateTranscriptionModel(
        state.settings?.model ?? "gemini-3.1-flash-lite",
      );
      setModelValidation(validation);
      await refreshEnvironment();
      dispatch({ type: "toast", toast: makeToast("success", "API key verified", "Saved securely in Windows Credential Manager.") });
    } catch (error) {
      const problem = normalizeError(error);
      setSetupError(problem);
      await refreshEnvironment();
      notify(error);
    } finally { setSetupBusy(null); }
  }

  async function deleteApiKey() {
    if (!window.confirm("Remove the Gemini API key from Windows Credential Manager?")) return;
    setSetupBusy("key");
    try { await backend.deleteApiKey(); setSetupError(null); await refreshEnvironment(); } catch (error) { notify(error); } finally { setSetupBusy(null); }
  }

  async function installDownloader() {
    setSetupBusy("downloader");
    try { await backend.installDownloader(); await refreshEnvironment(); dispatch({ type: "toast", toast: makeToast("success", "Downloader ready", "The official pinned yt-dlp build passed checksum verification.") }); } catch (error) { notify(error); } finally { setSetupBusy(null); }
  }

  async function loadTranscriptionModels(customModel: string | null = null) {
    setModelBusy(true);
    try {
      const options = await backend.listTranscriptionModels(customModel);
      if (options.length > 0) setModelOptions(options);
      modelCatalogLoaded.current = true;
    } catch (error) {
      const problem = normalizeError(error);
      setModelValidation({
        model_id: customModel ?? state.settings?.model ?? "",
        availability: "unknown",
        status: "unverified",
        message: problem.user_message,
        checked_at: null,
      });
    } finally {
      setModelBusy(false);
    }
  }

  async function validateTranscriptionModel(model: string, runAudioTest = false) {
    setModelBusy(true);
    try {
      const validation = await backend.validateTranscriptionModel(model, runAudioTest);
      setModelValidation(validation);
      if (validation.status === "valid") {
        await loadTranscriptionModels(model);
      }
      return validation;
    } catch (error) {
      const problem = normalizeError(error);
      setModelValidation({
        model_id: model,
        availability: "unknown",
        status: "invalid",
        message: problem.user_message,
        checked_at: null,
      });
      notify(error);
      return null;
    } finally {
      setModelBusy(false);
    }
  }

  async function runSetupTest(model: string | null = state.settings?.model ?? null) {
    setSetupBusy("test");
    setSetupError(null);
    setSetupTest(null);
    try { const result = await backend.runSetupTest(model); setSetupTest(result); await refreshEnvironment(); dispatch({ type: "toast", toast: makeToast("success", "Setup test passed", result.message) }); } catch (error) { const problem = normalizeError(error); setSetupError(problem); notify(error); } finally { setSetupBusy(null); }
  }

  async function refreshEnvironment() {
    try { dispatch({ type: "environment", environment: await backend.checkEnvironment() }); } catch (error) { notify(error); }
  }

  async function openArtifact(itemId: string, kind: ArtifactKind, reveal = false) {
    if (!state.job) return;
    try { await backend.openArtifact(state.job.id, itemId, kind, reveal); } catch (error) { notify(error); }
  }

  async function openHistory(entry: HistoryEntry) {
    try {
      const job = await backend.getJobSnapshot(entry.job_id);
      if (job.summary) completedJob.current = job.id;
      dispatch({ type: "job", job });
      dispatch({ type: "dialog", dialog: job.summary ? "summary" : null });
      dispatch({ type: "activity", expanded: !job.summary });
    } catch (error) { notify(error); }
  }

  async function exportDiagnostics() {
    try {
      await backend.previewDiagnostics();
      const destination = await save({ defaultPath: "lecturescribe-diagnostics.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!destination) return;
      const result = await backend.exportDiagnostics(destination);
      dispatch({ type: "toast", toast: makeToast("success", "Diagnostic report exported", `Sanitized report saved to ${result.path}.`) });
    } catch (error) { notify(error); }
  }

  function setTheme() {
    if (!state.settings) return;
    const settings = { ...state.settings, theme: state.settings.theme === "light" ? "dark" as const : "light" as const };
    dispatch({ type: "settings", settings });
    void backend.saveSettings(settings).catch(notify);
  }

  function closeSetup() {
    try {
      window.localStorage.setItem("lecturescribe.setup.intro-seen", "1");
    } catch {
      // The setup center still closes if WebView storage is unavailable.
    }
    dispatch({ type: "dialog", dialog: null });
  }

  function handleToastAction(toast: ToastMessage) {
    if (toast.action_id === "undo_source") dispatch({ type: "undo_remove_source" });
    if (toast.action_id === "open_output") {
      const job = jobRef.current;
      void (job?.summary ? backend.openJobOutput(job.id) : backend.openOutputFolder());
    }
    dispatch({ type: "dismiss_toast", id: toast.id });
  }

  return {
    state,
    dispatch,
    pastedText,
    setPastedText,
    starting,
    settingsSaving,
    setupBusy,
    setupTest,
    setupError,
    modelOptions,
    modelValidation,
    modelBusy,
    actions: {
      addTextFiles,
      addMedia,
      addFolder,
      addPastedLinks,
      removeSource,
      clearSources,
      refreshPreview: () => setPreviewNonce((value) => value + 1),
      reviewPlan,
      startPlan,
      pauseJob,
      resumeJob,
      cancelJob,
      retryFailed,
      saveSettings,
      chooseOutput,
      chooseFfmpeg,
      saveApiKey,
      deleteApiKey,
      installDownloader,
      loadTranscriptionModels,
      validateTranscriptionModel,
      runSetupTest,
      refreshEnvironment,
      openArtifact,
      openHistory,
      exportDiagnostics,
      setTheme,
      closeSetup,
      handleToastAction,
      openOutput: (jobId?: string) => (jobId ? backend.openJobOutput(jobId) : backend.openOutputFolder()).catch(notify),
      openLink: (target: "ai_studio" | "github" | "ffmpeg" | "yt_dlp") => backend.openKnownLink(target).catch(notify),
      setMode: (mode: RunMode) => dispatch({ type: "mode", mode }),
      setWorkspace: (view: WorkspaceView) => dispatch({ type: "workspace", view }),
      setFilter: (filter: QueueFilter) => dispatch({ type: "filter", filter }),
      setSearch: (search: string) => dispatch({ type: "search", search }),
      toggleItem: (id: string, selected: boolean) => dispatch({ type: "toggle_item", id, selected }),
      selectItems: (ids: string[], selected: boolean) => dispatch({ type: "select_items", ids, selected }),
    },
  };
}

function setupIntroSeen(): boolean {
  try {
    return window.localStorage.getItem("lecturescribe.setup.intro-seen") === "1";
  } catch {
    return false;
  }
}

function normalizePaths(value: string | string[] | null): string[] {
  return value ? (Array.isArray(value) ? value : [value]) : [];
}

function extension(path: string): string {
  const name = fileName(path);
  const index = name.lastIndexOf(".");
  return index < 0 ? "" : name.slice(index + 1).toLocaleLowerCase();
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function sourceId(kind: SourceKind, value: string): string {
  let hash = 2166136261;
  for (const character of `${kind}:${value}`) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return `source-${(hash >>> 0).toString(16)}`;
}

function errorToast(error: ReturnType<typeof normalizeError>): ToastMessage {
  const action = error.recovery_actions[0];
  return makeToast(
    error.severity === "warning" ? "warning" : "error",
    "Action needs attention",
    error.user_message,
    action ? { action_id: action.action, action_label: action.label } : undefined,
  );
}
