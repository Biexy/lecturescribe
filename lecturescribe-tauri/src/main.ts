import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";

type SourceKind = "pasted" | "links" | "media";

type QueueItem = {
  id: string;
  number: number;
  source_type: string;
  title: string;
  source: string;
  url: string;
  media_path: string;
  thumbnail_path: string;
  transcript_path: string;
  markdown_path: string;
  downloaded_media_path: string;
  estimated_chunks: number;
  duplicate_of: string | null;
  selected: boolean;
  status: string;
  error: string | null;
  fix_action: string | null;
};

type ToolStatus = {
  name: string;
  ok: boolean;
  detail: string;
};

type EnvironmentStatus = {
  ffmpeg: ToolStatus;
  yt_dlp: ToolStatus;
  native_engine: ToolStatus;
  api_key_ok: boolean;
  default_output_dir: string;
  default_download_dir: string;
  legacy_root: string;
};

type AppSettings = {
  output_dir: string;
  download_dir: string;
  work_dir: string;
  model: string;
  run_mode: string;
  theme: string;
  transcript_format: string;
  prompt_preset: string;
  ffmpeg_path: string;
  downloader_path: string;
  chunk_minutes: number;
  request_delay_seconds: number;
  cookies_from_browser: string;
  cookies_file: string;
  skip_download: boolean;
  force: boolean;
};

type SourceEntry = {
  kind: SourceKind;
  value: string;
  label: string;
  count?: number;
};

type DefaultSourceSummary = {
  path: string;
  label: string;
  link_count: number;
};

type EngineDone = {
  code: number | null;
  success: boolean;
};

type EngineProgress = {
  phase: string;
  message: string;
  status: string;
  current_item: number | null;
  total_items: number;
  completed_items: number;
  chunk_current: number;
  chunk_total: number;
  download_speed: string;
  percent: number;
};

type SetupTestResult = {
  ok: boolean;
  message: string;
  transcript_preview: string;
};

type QueueFilter = "all" | "selected" | "ready" | "downloading" | "done" | "failed";

type Toast = {
  id: number;
  kind: "success" | "warning" | "error" | "info";
  message: string;
};

type RunSummary = {
  title: string;
  saved: number;
  failed: number;
  output: string;
  duration: string;
};

const defaultSettings: AppSettings = {
  output_dir: "",
  download_dir: "",
  work_dir: "",
  model: "gemini-3.1-flash-lite",
  run_mode: "download_transcribe",
  theme: "light",
  transcript_format: "txt_markdown",
  prompt_preset: "default",
  ffmpeg_path: "",
  downloader_path: "",
  chunk_minutes: 2,
  request_delay_seconds: 5,
  cookies_from_browser: "",
  cookies_file: "",
  skip_download: false,
  force: false,
};

const state = {
  sources: [] as SourceEntry[],
  autoSource: null as DefaultSourceSummary | null,
  queue: [] as QueueItem[],
  activeRunIds: [] as string[],
  environment: null as EnvironmentStatus | null,
  settings: { ...defaultSettings },
  logs: [] as string[],
  running: false,
  previewing: false,
  cancelling: false,
  setupTesting: false,
  wizardOpen: false,
  settingsOpen: false,
  logsOpen: false,
  queueFilter: "all" as QueueFilter,
  queueSearch: "",
  toasts: [] as Toast[],
  lastSummary: null as RunSummary | null,
  runStartedAt: 0,
  runTotal: 0,
  settingsMessage: "",
  sourceNotice: "Add links, a .txt file, or local media. The queue updates automatically.",
  previewNotice: "Preview is automatic. If no sources are added, LectureScribe can use Drive links.txt.",
  setupNotice: "",
  phase: "Idle",
  current: "No active job",
  chunks: "0 / 0 chunks",
  itemProgress: "0 / 0 items",
  percent: "0%",
  speed: "0 KB/s",
  completedItems: 0,
  totalItems: 0,
  currentItem: 0,
};

const app = document.querySelector<HTMLDivElement>("#app")!;

const icons: Record<string, string> = {
  file: `<svg viewBox="0 0 24 24"><path d="M14 2H7a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7z"/><path d="M14 2v5h5"/><path d="M9 13h6M9 17h6M9 9h2"/></svg>`,
  folder: `<svg viewBox="0 0 24 24"><path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H10l2 2h6.5A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5z"/></svg>`,
  link: `<svg viewBox="0 0 24 24"><path d="M10 13a5 5 0 0 0 7.1 0l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1"/><path d="M14 11a5 5 0 0 0-7.1 0l-2 2A5 5 0 0 0 12 20.1l1.1-1.1"/></svg>`,
  eye: `<svg viewBox="0 0 24 24"><path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/></svg>`,
  play: `<svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>`,
  stop: `<svg viewBox="0 0 24 24"><rect x="7" y="7" width="10" height="10" rx="1.5"/></svg>`,
  refresh: `<svg viewBox="0 0 24 24"><path d="M21 12a9 9 0 1 1-2.6-6.4"/><path d="M21 3v6h-6"/></svg>`,
  gear: `<svg viewBox="0 0 24 24"><path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z"/><path d="m19.4 15 .2 1.8a2 2 0 0 1-2.8 2l-1.6-.8a8.3 8.3 0 0 1-1.7.7l-.5 1.7a2 2 0 0 1-3.8 0l-.5-1.7a8.3 8.3 0 0 1-1.7-.7l-1.6.8a2 2 0 0 1-2.8-2l.2-1.8a7.8 7.8 0 0 1-.8-1.5L2.5 12l1.3-1.5c.2-.5.5-1 .8-1.5l-.2-1.8a2 2 0 0 1 2.8-2l1.6.8c.5-.3 1.1-.5 1.7-.7l.5-1.7a2 2 0 0 1 3.8 0l.5 1.7c.6.2 1.1.4 1.7.7l1.6-.8a2 2 0 0 1 2.8 2L19.4 9c.3.5.6 1 .8 1.5L21.5 12l-1.3 1.5c-.2.5-.5 1-.8 1.5Z"/></svg>`,
  list: `<svg viewBox="0 0 24 24"><path d="M8 6h13M8 12h13M8 18h13"/><path d="M3 6h.01M3 12h.01M3 18h.01"/></svg>`,
  film: `<svg viewBox="0 0 24 24"><rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 3v18M16 3v18M4 8h4M4 16h4M16 8h4M16 16h4"/></svg>`,
  speed: `<svg viewBox="0 0 24 24"><path d="M21 13a9 9 0 1 0-18 0"/><path d="m12 13 5-5"/><path d="M7 13h.01M17 13h.01M12 8h.01"/></svg>`,
  shield: `<svg viewBox="0 0 24 24"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/><path d="m9 12 2 2 4-4"/></svg>`,
  key: `<svg viewBox="0 0 24 24"><circle cx="7.5" cy="15.5" r="4.5"/><path d="m11 12 9-9"/><path d="m15 8 2 2 3-3"/></svg>`,
  close: `<svg viewBox="0 0 24 24"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`,
  check: `<svg viewBox="0 0 24 24"><path d="m20 6-11 11-5-5"/></svg>`,
  upload: `<svg viewBox="0 0 24 24"><path d="M12 16V4"/><path d="m7 9 5-5 5 5"/><path d="M20 16.5V19a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-2.5"/></svg>`,
  alert: `<svg viewBox="0 0 24 24"><path d="M12 9v4"/><path d="M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/></svg>`,
  open: `<svg viewBox="0 0 24 24"><path d="M14 3h7v7"/><path d="M10 14 21 3"/><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5"/></svg>`,
  retry: `<svg viewBox="0 0 24 24"><path d="M3 12a9 9 0 0 1 15.3-6.4"/><path d="M18 2v4h-4"/><path d="M21 12a9 9 0 0 1-15.3 6.4"/><path d="M6 22v-4h4"/></svg>`,
  sun: `<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>`,
  moon: `<svg viewBox="0 0 24 24"><path d="M21 14.7A8 8 0 0 1 9.3 3a7 7 0 1 0 11.7 11.7Z"/></svg>`,
  monitor: `<svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/></svg>`,
};

function icon(name: string): string {
  return icons[name] ?? "";
}

function render() {
  const selected = selectedQueueItems().length;
  const visibleQueue = filteredQueueItems();
  app.innerHTML = `
    <main class="shell">
      <header class="topbar">
        <div class="brand">
          <div class="brand-mark">${icon("file")}</div>
          <div>
            <h1>LectureScribe</h1>
            <p>Community lecture transcription</p>
          </div>
        </div>
        <div class="top-actions">
          ${setupPillsHtml()}
          ${themeButtonHtml()}
          <button class="ghost" data-action="open-setup" title="Open first-run setup and tool fixes">${icon("shield")} Setup</button>
          <button class="ghost" data-action="open-output" title="Open the transcript output folder">${icon("folder")} Open output</button>
          <button class="ghost" data-action="run-doctor" title="Check API key, FFmpeg, Downloader, and output folder" ${state.running ? "disabled" : ""}>${icon("shield")} Doctor</button>
          <button class="ghost" data-action="open-settings" title="Open advanced settings, history, and logs">${icon("gear")} Settings</button>
        </div>
      </header>

      <section class="workspace">
        <aside class="source-panel">
          ${workflowCard(1, "Add sources", addSourcesContent(), "YouTube, Google Drive, .txt link files, and local audio/video.")}
          ${workflowCard(2, "Preview and choose", previewContent(), "The queue updates automatically. Uncheck anything you do not want to transcribe.")}
          ${workflowCard(3, "Start", startContent(), "Transcripts, cached chunks, and the index are saved in your output folder.", true)}
        </aside>

        <section class="queue-card">
          <div class="queue-head">
            <div>
              <h2>Queue preview <span>(${state.queue.length} items)</span></h2>
              <p>${selected} selected, ${visibleQueue.length} visible${state.previewing ? " - updating preview" : ""}</p>
            </div>
            <div class="queue-actions">
              <button class="small-button secondary" data-action="select-all" title="${state.running ? "Selection is locked while a run is active." : "Select all visible queue items."}" ${state.queue.length === 0 || state.running ? "disabled" : ""}>Select all</button>
              <button class="small-button secondary" data-action="select-none" title="${state.running ? "Selection is locked while a run is active." : "Unselect all visible queue items."}" ${state.queue.length === 0 || state.running ? "disabled" : ""}>Select none</button>
              <button class="icon-button" data-action="preview" aria-label="Refresh preview" title="${previewButtonTitle()}" ${state.previewing || state.running ? "disabled" : ""}>${icon("refresh")}</button>
            </div>
          </div>
          <div class="queue-tools">
            <label class="search-field">
              <span>${icon("eye")}</span>
              <input id="queue-search" value="${escapeHtml(state.queueSearch)}" placeholder="Search queue" aria-label="Search queue" />
            </label>
            <div class="filter-tabs" role="tablist" aria-label="Queue filters">
              ${queueFilterTabsHtml()}
            </div>
          </div>
          <div class="queue-table" aria-live="polite">
            <div class="queue-row queue-header">
              <div class="select-cell"><input type="checkbox" aria-label="Select all queue items" data-action="select-all-checkbox" ${state.queue.length && selected === state.queue.length ? "checked" : ""} ${state.queue.length === 0 || state.running ? "disabled" : ""} /></div>
              <div>Title</div>
              <div>URL or source</div>
              <div>Media</div>
              <div>Status</div>
            </div>
            <div class="queue-body">${queueRowsHtml()}</div>
          </div>
        </section>
      </section>

      <footer class="progress-card">
        <div class="progress-title">Progress</div>
        <div class="phase-dot ${state.running ? "active" : ""}"></div>
        <div class="phase">${escapeHtml(state.phase)}</div>
        <div class="muted current" title="${escapeHtml(state.current)}">${escapeHtml(state.current)}</div>
        <div class="divider"></div>
        <div class="metric" title="Selected run count is separate from the full queue count.">${icon("list")} ${escapeHtml(progressItemText())}</div>
        <div class="divider"></div>
        <div class="metric">${icon("film")} ${escapeHtml(state.chunks)}</div>
        <div class="divider"></div>
        <div class="speed-block">${icon("speed")}<div><span class="muted">Download speed</span><strong>${escapeHtml(state.speed)}</strong></div></div>
        <div class="bar"><div class="bar-fill" style="width: ${safePercent(state.percent)}"></div></div>
        <div class="percent">${escapeHtml(state.percent)}</div>
      </footer>
      ${summaryPanelHtml()}
      ${toastStackHtml()}
    </main>

    ${pasteDialogHtml()}
    ${setupWizardHtml()}
    ${settingsDialogHtml()}
  `;

  wireEvents();
}

function workflowCard(number: number, title: string, body: string, helper: string, shield = false): string {
  return `
    <section class="step-card">
      <div class="step-title"><div class="step-number">${number}</div><h2>${escapeHtml(title)}</h2></div>
      ${body}
      <p class="helper ${shield ? "with-icon" : ""}">${shield ? icon("shield") : ""}${escapeHtml(helper)}</p>
    </section>
  `;
}

function addSourcesContent(): string {
  return `
    <div class="source-dropzone" data-dropzone>
      <div class="action-pair">
        <button class="primary" data-action="paste" title="${state.running ? "Sources are locked while a run is active." : "Paste YouTube, Google Drive, or local media paths."}" ${state.running ? "disabled" : ""}>${icon("link")} Paste links</button>
        <button class="secondary" data-action="add-media" title="${state.running ? "Sources are locked while a run is active." : "Add local audio or video files."}" ${state.running ? "disabled" : ""}>${icon("folder")} Add media</button>
      </div>
      <div class="source-tools">
        <button class="text-button" data-action="add-link-file" title="Add a text file and immediately count its links." ${state.running ? "disabled" : ""}>${icon("upload")} Add .txt link file</button>
        <button class="text-button" data-action="clear-sources" title="${state.running ? "Sources are locked while a run is active." : "Clear manual sources, automatic source preview, and queue."}" ${(state.sources.length === 0 && !state.autoSource) || state.running ? "disabled" : ""}>Clear sources</button>
      </div>
      <p class="drop-hint">Drag files here, or use the buttons above.</p>
    </div>
    ${sourceSummaryHtml()}
    ${sourceNoticeHtml(state.sourceNotice)}
  `;
}

function previewContent(): string {
  const selected = selectedQueueItems().length;
  const previewLabel = state.running ? "Preview locked while running" : state.previewing ? "Updating preview..." : "Preview now";
  return `
    <button class="wide secondary" data-action="preview" title="${previewButtonTitle()}" ${state.previewing || state.running ? "disabled" : ""}>${icon("eye")} ${previewLabel}</button>
    <div class="hint-grid">
      <div><strong>${state.queue.length}</strong><span>items found</span></div>
      <div><strong>${selected}</strong><span>selected</span></div>
      <div><strong>${duplicateHintCount()}</strong><span>duplicates skipped</span></div>
    </div>
    ${sourceNoticeHtml(previewNoticeText())}
  `;
}

function startContent(): string {
  const mode = state.settings.run_mode || "download_transcribe";
  if (state.running) {
    return `
      <div class="run-estimate active">${escapeHtml(modeLabel(mode))} - ${escapeHtml(progressItemText())}</div>
      <button class="wide danger" data-action="cancel" title="Stops after the current safe step. Completed downloads and chunks stay cached." ${state.cancelling ? "disabled" : ""}>${icon("stop")} ${state.cancelling ? "Cancelling..." : "Cancel run"}</button>
      <p class="notice-line">Current work stops at the next safe point. Completed chunks stay cached.</p>
    `;
  }

  const hasFailures = failedQueueItems().length > 0;
  const selected = selectedQueueItems().length;
  const disabledReason = startDisabledReason();
  return `
    <div class="mode-tabs" role="tablist" aria-label="Run mode">
      ${runModeButton("download_transcribe", "Download + transcribe", mode, "Download links, then transcribe with Gemini.")}
      ${runModeButton("download_only", "Download only", mode, "Download link media without using Gemini.")}
      ${runModeButton("transcribe_existing", "Transcribe existing media", mode, "Use already downloaded or local media files.")}
    </div>
    <div class="run-estimate">${escapeHtml(runEstimateText())}</div>
    <button class="wide primary" data-action="start" title="${escapeHtml(disabledReason || `Start ${modeLabel(mode).toLowerCase()} for selected queue items.`)}" ${disabledReason ? "disabled" : ""}>${icon("play")} ${startButtonLabel()}</button>
    <div class="start-actions">
      <button class="small-button secondary" data-action="retry-failed" title="Retries failed rows and reuses cached downloads/chunks when possible." ${hasFailures ? "" : "disabled"}>${icon("retry")} Retry failed</button>
      <button class="small-button secondary" data-action="open-output">${icon("folder")} Output folder</button>
    </div>
  `;
}

function runModeButton(value: string, label: string, active: string, title: string): string {
  return `<button type="button" class="mode-tab ${value === active ? "active" : ""}" data-action="set-run-mode" data-run-mode="${value}" title="${escapeHtml(title)}">${escapeHtml(label)}</button>`;
}

function modeLabel(mode = state.settings.run_mode): string {
  if (mode === "download_only") return "Download only";
  if (mode === "transcribe_existing") return "Transcribe existing media";
  return "Download + transcribe";
}

function outputFormatLabel(): string {
  if (state.settings.transcript_format === "txt") return "TXT only";
  if (state.settings.transcript_format === "markdown") return "Markdown only";
  return "TXT + Markdown";
}

function startButtonLabel(): string {
  if (state.settings.run_mode === "download_only") return "Start download";
  return "Start transcription";
}

function startDisabledReason(): string {
  if (state.previewing) return "Preview is still updating.";
  if (state.queue.length > 0 && selectedQueueItems().length === 0) return "Select at least one queue item first.";
  return "";
}

function runEstimateText(): string {
  const selected = selectedQueueItems();
  if (!state.queue.length) return "Preview runs first. Add sources or use an automatic link file.";
  if (!selected.length) return "No rows selected. Select one or more queue items to start.";
  const estimated = selected.reduce((sum, item) => sum + (item.estimated_chunks || 0), 0);
  const chunks = estimated ? `about ${estimated} chunk${estimated === 1 ? "" : "s"}` : "chunks estimated on start";
  const gemini = state.settings.run_mode === "download_only" ? "Gemini not used" : "Gemini will be used";
  return `${selected.length} selected - ${chunks} - ${outputFormatLabel()} - ${gemini}`;
}

function previewNoticeText(): string {
  if (!state.queue.length) return state.previewNotice;
  return `${state.queue.length} found - ${selectedQueueItems().length} selected - ${duplicateHintCount()} duplicates skipped. ${state.previewNotice}`;
}

function previewButtonTitle(): string {
  if (state.running) return "Preview is locked while a run is active.";
  if (state.previewing) return "Preview is updating.";
  return state.sources.length ? "Refresh the queue from current sources." : "Preview sources or auto-load Drive links.txt / links.txt.";
}

function progressItemText(): string {
  if (state.running) return state.itemProgress;
  if (state.queue.length) return `${selectedQueueItems().length} / ${state.queue.length} selected`;
  return "0 selected";
}

function doctorSummaryText(): string {
  const env = state.environment;
  if (!env) return "Doctor has not checked setup yet.";
  const fixes = [];
  if (!env.api_key_ok) fixes.push("save a Gemini API key");
  if (!env.ffmpeg.ok) fixes.push("install or choose FFmpeg");
  if (!env.yt_dlp.ok) fixes.push("install or choose Downloader for links");
  if (!fixes.length) return "Doctor passed: API key, FFmpeg, Downloader, and output folder are ready.";
  return `Doctor found setup work: ${fixes.join(", ")}.`;
}

function themeButtonHtml(): string {
  const current = themePreference();
  const next = nextThemePreference(current);
  const label = themeLabel(current);
  const title = `Theme: ${label}. Click to switch to ${themeLabel(next)}.`;
  return `
    <button class="ghost theme-toggle" data-action="cycle-theme" title="${escapeHtml(title)}" aria-label="${escapeHtml(title)}">
      ${icon(themeIcon(current))}
    </button>
  `;
}

function themePreference(): "system" | "light" | "dark" {
  const value = state.settings.theme;
  if (value === "system" || value === "dark") return value;
  return "light";
}

function nextThemePreference(value = themePreference()): "system" | "light" | "dark" {
  if (value === "system") return "light";
  if (value === "light") return "dark";
  return "system";
}

function themeLabel(value = themePreference()): string {
  if (value === "light") return "Light";
  if (value === "dark") return "Dark";
  return "System";
}

function themeIcon(value = themePreference()): string {
  if (value === "light") return "sun";
  if (value === "dark") return "moon";
  return "monitor";
}

function setupPillsHtml(): string {
  const env = state.environment;
  if (!env) return `<div class="setup-pills"><span class="setup-pill pending">Checking setup</span></div>`;
  const items = [
    ["API", env.api_key_ok],
    ["FFmpeg", env.ffmpeg.ok],
    ["Downloader", env.yt_dlp.ok],
  ] as Array<[string, boolean]>;
  return `
    <div class="setup-pills" title="${escapeHtml(state.setupNotice || "Setup status")}">
      ${items.map(([label, ok]) => `<span class="setup-pill ${ok ? "ok" : "missing"}">${escapeHtml(label)}</span>`).join("")}
    </div>
  `;
}

function queueFilterTabsHtml(): string {
  const filters: Array<[QueueFilter, string]> = [
    ["all", "All"],
    ["selected", "Selected"],
    ["ready", "Ready"],
    ["downloading", "Downloading"],
    ["done", "Done"],
    ["failed", "Failed"],
  ];
  return filters
    .map(([value, label]) => `<button type="button" class="filter-tab ${state.queueFilter === value ? "active" : ""}" data-action="set-filter" data-filter="${value}">${label}</button>`)
    .join("");
}

function summaryPanelHtml(): string {
  const summary = state.lastSummary;
  if (!summary) return "";
  return `
    <section class="run-summary" role="status" aria-live="polite">
      <div>
        <strong>${escapeHtml(summary.title)}</strong>
        <span>${summary.saved} saved, ${summary.failed} failed, ${escapeHtml(summary.duration)}</span>
      </div>
      <div class="summary-actions">
        <button class="small-button secondary" data-action="copy-output-path">Copy output path</button>
        <button class="small-button secondary" data-action="open-output">${icon("folder")} Open folder</button>
        ${summary.failed ? `<button class="small-button secondary" data-action="retry-failed">${icon("retry")} Retry failed</button>` : ""}
      </div>
    </section>
  `;
}

function toastStackHtml(): string {
  if (!state.toasts.length) return "";
  return `
    <div class="toast-stack" aria-live="polite">
      ${state.toasts
        .map(
          (toast) => `
            <div class="toast ${toast.kind}">
              <span>${escapeHtml(toast.message)}</span>
              <button type="button" class="remove-button" data-action="dismiss-toast" data-toast-id="${toast.id}" aria-label="Dismiss notification">${icon("close")}</button>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}

function sourceSummaryHtml(): string {
  const pasted = state.sources.filter((source) => source.kind === "pasted").reduce((sum, source) => sum + (source.count ?? 1), 0);
  const textFiles = state.sources.filter((source) => source.kind === "links").length;
  const textLinks = state.sources.filter((source) => source.kind === "links").reduce((sum, source) => sum + (source.count ?? 0), 0);
  const media = state.sources.filter((source) => source.kind === "media").length;
  const hasSources = state.sources.length > 0;
  const auto = !hasSources ? state.autoSource : null;
  const title = hasSources ? `${state.sources.length} source groups added` : auto ? "Automatic source loaded" : "No sources yet";
  const detail = hasSources
    ? `${pasted} pasted links, ${textFiles} text files${textLinks ? ` (${textLinks} links)` : ""}, ${media} media files.`
    : auto
      ? `${auto.label}: ${auto.link_count} link${auto.link_count === 1 ? "" : "s"} found automatically.`
      : "Add links, a .txt file, or local media. Preview can also load Drive links.txt.";
  return `
    <div class="source-summary ${hasSources || auto ? "has-sources" : ""}">
      <div>
        <strong>${escapeHtml(title)}</strong>
        <span>${escapeHtml(detail)}</span>
      </div>
      ${hasSources ? `<span class="mini-pill">${state.sources.length}</span>` : auto ? `<span class="mini-pill">${auto.link_count}</span>` : ""}
    </div>
    <div class="source-list main-source-list">${sourceListHtml("main")}</div>
  `;
}

function sourceNoticeHtml(message: string): string {
  return `<p class="notice-line" role="status" aria-live="polite">${escapeHtml(message)}</p>`;
}

function pasteDialogHtml(): string {
  return `
    <dialog id="paste-dialog">
      <form class="dialog-body" id="paste-form">
        <div class="dialog-title">
          <div>
            <h3>Paste links or paths</h3>
            <p class="muted">One YouTube link, Google Drive link, or local file path per line.</p>
          </div>
          <button type="button" class="icon-button" data-action="close-paste" aria-label="Close paste dialog">${icon("close")}</button>
        </div>
        <label class="field-stack">
          <span>Sources</span>
          <textarea id="paste-text" placeholder="https://drive.google.com/file/d/...&#10;https://youtu.be/...&#10;C:\\Lectures\\chapter-01.mp3"></textarea>
        </label>
        <div class="dialog-actions">
          <button type="button" class="compact-button secondary" data-action="close-paste">Cancel</button>
          <button type="submit" class="compact-button primary">Add and preview</button>
        </div>
      </form>
    </dialog>
  `;
}

function setupWizardHtml(): string {
  const env = state.environment;
  const output = state.settings.output_dir || env?.default_output_dir || "";
  return `
    <dialog id="setup-dialog" class="setup-dialog">
      <div class="dialog-body setup-body">
        <div class="settings-header">
          <div>
            <h3>LectureScribe setup</h3>
            <p class="muted">One-time checks for transcription, downloads, and output.</p>
          </div>
          <button type="button" class="icon-button" data-action="close-setup" aria-label="Close setup">${icon("close")}</button>
        </div>

        <div class="wizard-grid">
          <section class="wizard-card privacy-card">
            <strong>Local-first</strong>
            <span>Your files, downloads, transcripts, and cache stay on this computer. Audio chunks are sent to Gemini only when transcription runs.</span>
          </section>

          <section class="wizard-card">
            <div class="wizard-step-head">
              <span class="step-number small">1</span>
              <div><strong>Gemini API key</strong><span>${env?.api_key_ok ? "Saved securely" : "Required for transcription"}</span></div>
            </div>
            <label class="field-stack">
              <span>API key</span>
              <div class="inline-field">
                <input id="setup-api-key-input" type="password" autocomplete="off" placeholder="Paste Gemini API key" />
                <button type="button" class="compact-button secondary" data-action="save-api-key">${icon("key")} Save</button>
              </div>
              <small>Get a key from <a href="https://aistudio.google.com/app/apikey" target="_blank" rel="noreferrer">Google AI Studio</a>. Recommended model: <b>gemini-3.1-flash-lite</b>.</small>
            </label>
          </section>

          <section class="wizard-card">
            <div class="wizard-step-head">
              <span class="step-number small">2</span>
              <div><strong>FFmpeg</strong><span>${env?.ffmpeg.ok ? "Ready" : "Required for audio extraction and chunking"}</span></div>
            </div>
            <p class="notice-line">${escapeHtml(env?.ffmpeg.detail || "Checking FFmpeg...")}</p>
            <div class="wizard-actions">
              <button type="button" class="compact-button secondary" data-action="install-ffmpeg">Install FFmpeg</button>
              <button type="button" class="compact-button secondary" data-action="choose-ffmpeg">${icon("folder")} Choose FFmpeg</button>
              <button type="button" class="compact-button secondary" data-action="refresh-environment">${icon("refresh")} Check again</button>
            </div>
          </section>

          <section class="wizard-card">
            <div class="wizard-step-head">
              <span class="step-number small">3</span>
              <div><strong>Downloader</strong><span>${env?.yt_dlp.ok ? "Ready for YouTube and Drive links" : "Needed only for link downloads"}</span></div>
            </div>
            <p class="notice-line">${escapeHtml(env?.yt_dlp.detail || "Bundled or app-managed yt-dlp is checked here.")}</p>
            <div class="wizard-actions">
              <button type="button" class="compact-button secondary" data-action="install-downloader">Install downloader</button>
              <button type="button" class="compact-button secondary" data-action="update-downloader">Update downloader</button>
              <button type="button" class="compact-button secondary" data-action="choose-downloader">${icon("folder")} Choose downloader</button>
              <button type="button" class="compact-button secondary" data-action="refresh-environment">${icon("refresh")} Check again</button>
            </div>
            <small class="muted">Technical name: yt-dlp. Default app path: %LOCALAPPDATA%\\LectureScribe\\tools\\yt-dlp.exe</small>
          </section>

          <section class="wizard-card">
            <div class="wizard-step-head">
              <span class="step-number small">4</span>
              <div><strong>Output folder</strong><span>${escapeHtml(shortName(output) || "Choose where transcripts are saved")}</span></div>
            </div>
            ${pathField("Output folder", "output_dir", output, "Transcripts, Markdown files, and 00_index.md are saved here.")}
          </section>

          <section class="wizard-card">
            <div class="wizard-step-head">
              <span class="step-number small">5</span>
              <div><strong>Test setup</strong><span>Uses one tiny Gemini request</span></div>
            </div>
            <button type="button" class="compact-button primary" data-action="run-setup-test" ${state.setupTesting || state.running ? "disabled" : ""}>${icon("shield")} Test setup</button>
            <p class="notice-line">${escapeHtml(state.setupNotice || "Confirms the API key, FFmpeg, and the audio request path.")}</p>
          </section>
        </div>

        <div class="settings-footer">
          <span class="settings-message">${escapeHtml(state.settingsMessage)}</span>
          <div class="dialog-actions">
            <button type="button" class="compact-button secondary" data-action="close-setup">Close</button>
            <button type="button" class="compact-button primary" data-action="save-settings">${icon("check")} Save setup</button>
          </div>
        </div>
      </div>
    </dialog>
  `;
}

function settingsDialogHtml(): string {
  const s = state.settings;
  return `
    <dialog id="settings-dialog" class="settings-dialog">
      <form class="dialog-body settings-body" id="settings-form">
        <div class="settings-header">
          <div>
            <h3>Settings</h3>
            <p class="muted">Setup, folders, model, private download options, history, and logs.</p>
          </div>
          <button type="button" class="icon-button" data-action="close-settings" aria-label="Close settings">${icon("close")}</button>
        </div>

        <div class="settings-grid">
          <section class="settings-section">
            <div class="section-head">
              <h4>Doctor</h4>
              <button type="button" class="text-button" data-action="run-doctor">Run doctor</button>
            </div>
            <div class="tool-grid">${toolStatusHtml()}</div>
            <p class="notice-line">${escapeHtml(doctorSummaryText())}</p>
            <button type="button" class="compact-button secondary" data-action="run-setup-test" ${state.setupTesting || state.running ? "disabled" : ""}>${icon("shield")} Run setup test</button>
            <p class="notice-line">${escapeHtml(state.setupNotice || "The setup test uses one Gemini request.")}</p>
          </section>

          <section class="settings-section">
            <h4>Appearance and API key</h4>
            <label class="field-stack">
              <span>Theme</span>
              <select data-setting="theme">
                <option value="system" ${s.theme === "system" ? "selected" : ""}>System</option>
                <option value="light" ${s.theme === "light" ? "selected" : ""}>Light</option>
                <option value="dark" ${s.theme === "dark" ? "selected" : ""}>Dark</option>
              </select>
              <small>System follows your Windows light or dark preference.</small>
            </label>
            <label class="field-stack">
              <span>Gemini API key</span>
              <div class="inline-field">
                <input id="api-key-input" type="password" autocomplete="off" placeholder="Paste key to save locally" />
                <button type="button" class="compact-button secondary" data-action="save-api-key">${icon("key")} Save</button>
              </div>
              <small>Saved in the OS secure credential store. Existing keys are never shown here.</small>
            </label>
          </section>

          <section class="settings-section span-2">
            <h4>Tools</h4>
            <div class="tool-grid">${toolStatusHtml()}</div>
            <div class="wizard-actions">
              <button type="button" class="compact-button secondary" data-action="install-downloader">Install downloader</button>
              <button type="button" class="compact-button secondary" data-action="update-downloader">Update downloader</button>
              <button type="button" class="compact-button secondary" data-action="choose-downloader">Choose downloader</button>
              <button type="button" class="compact-button secondary" data-action="install-ffmpeg">Install FFmpeg</button>
              <button type="button" class="compact-button secondary" data-action="choose-ffmpeg">Choose FFmpeg</button>
            </div>
          </section>

          <section class="settings-section span-2">
            <h4>Folders</h4>
            ${pathField("Output folder", "output_dir", s.output_dir, "Final transcript files and 00_index.md are saved here.")}
            ${pathField("Download folder", "download_dir", s.download_dir, "Downloaded media is cached here.")}
            ${pathField("Work/cache folder", "work_dir", s.work_dir, "Audio chunks and cached chunk transcripts are stored here.")}
          </section>

          <section class="settings-section">
            <h4>Transcription</h4>
            <label class="field-stack">
              <span>Run mode</span>
              <select data-setting="run_mode">
                <option value="download_transcribe" ${s.run_mode === "download_transcribe" ? "selected" : ""}>Download + transcribe</option>
                <option value="download_only" ${s.run_mode === "download_only" ? "selected" : ""}>Download only</option>
                <option value="transcribe_existing" ${s.run_mode === "transcribe_existing" ? "selected" : ""}>Transcribe existing media</option>
              </select>
            </label>
            <label class="field-stack">
              <span>Transcript format</span>
              <select data-setting="transcript_format">
                <option value="txt_markdown" ${s.transcript_format === "txt_markdown" ? "selected" : ""}>TXT + Markdown</option>
                <option value="txt" ${s.transcript_format === "txt" ? "selected" : ""}>TXT only</option>
                <option value="markdown" ${s.transcript_format === "markdown" ? "selected" : ""}>Markdown only</option>
              </select>
            </label>
            <label class="field-stack">
              <span>Model <strong class="recommended-label">Recommended</strong></span>
              <input data-setting="model" value="${escapeHtml(s.model)}" />
              <small>gemini-3.1-flash-lite is recommended because it is easy to get in AI Studio and usually friendly for free-tier users.</small>
            </label>
            <div class="two-fields">
              <label class="field-stack">
                <span>Chunk minutes</span>
                <input data-setting="chunk_minutes" type="number" min="1" max="30" step="1" value="${escapeHtml(String(s.chunk_minutes))}" />
              </label>
              <label class="field-stack">
                <span>Delay seconds</span>
                <input data-setting="request_delay_seconds" type="number" min="0" max="120" step="0.5" value="${escapeHtml(String(s.request_delay_seconds))}" />
              </label>
            </div>
            <label class="field-stack">
              <span>Prompt preset</span>
              <select data-setting="prompt_preset">
                <option value="default" ${s.prompt_preset === "default" ? "selected" : ""}>Default lecture</option>
                <option value="arabic_lecture" ${s.prompt_preset === "arabic_lecture" ? "selected" : ""}>Arabic lecture</option>
                <option value="english_lecture" ${s.prompt_preset === "english_lecture" ? "selected" : ""}>English lecture</option>
                <option value="technical_math" ${s.prompt_preset === "technical_math" ? "selected" : ""}>Technical/math lecture</option>
              </select>
            </label>
            <label class="check-row">
              <input data-setting="skip_download" type="checkbox" ${s.skip_download ? "checked" : ""} />
              <span>Skip download and use existing media only</span>
            </label>
            <label class="check-row">
              <input data-setting="force" type="checkbox" ${s.force ? "checked" : ""} />
              <span>Force re-transcribe completed outputs</span>
            </label>
          </section>

          <section class="settings-section">
            <h4>Private downloads</h4>
            <label class="field-stack">
              <span>Browser cookies</span>
              <input data-setting="cookies_from_browser" placeholder="chrome, edge, or firefox" value="${escapeHtml(s.cookies_from_browser)}" />
              <small>Use this for private Drive or YouTube links you can access in your browser.</small>
            </label>
            <label class="field-stack">
              <span>Cookie file</span>
              <div class="inline-field">
                <input data-setting="cookies_file" value="${escapeHtml(s.cookies_file)}" />
                <button type="button" class="compact-button secondary" data-action="choose-cookie-file">${icon("folder")} Browse</button>
              </div>
            </label>
          </section>

          <section class="settings-section span-2">
            <div class="section-head">
              <h4>History</h4>
              <button type="button" class="text-button" data-action="clear-history">Clear history</button>
            </div>
            ${historyHtml()}
          </section>

          <section class="settings-section span-2">
            <div class="section-head">
              <h4>Activity logs</h4>
              <div class="section-actions">
                <button type="button" class="text-button" data-action="toggle-logs">${state.logsOpen ? "Hide logs" : "Show logs"}</button>
                <button type="button" class="text-button" data-action="export-bug-report">Export bug report</button>
                <button type="button" class="text-button" data-action="clear-logs">Clear</button>
              </div>
            </div>
            ${state.logsOpen ? `<div class="log-panel">${logPanelHtml()}</div>` : `<p class="muted">Logs are hidden by default. Open them only when debugging.</p>`}
          </section>
        </div>

        <div class="settings-footer">
          <span class="settings-message" role="status" aria-live="polite">${escapeHtml(state.settingsMessage)}</span>
          <div class="dialog-actions">
            <button type="button" class="compact-button secondary" data-action="close-settings">Close</button>
            <button type="submit" class="compact-button primary">${icon("check")} Save settings</button>
          </div>
        </div>
      </form>
    </dialog>
  `;
}

function pathField(label: string, key: keyof Pick<AppSettings, "output_dir" | "download_dir" | "work_dir">, value: string, helper: string): string {
  return `
    <label class="field-stack">
      <span>${escapeHtml(label)}</span>
      <div class="inline-field">
        <input data-setting="${key}" value="${escapeHtml(value)}" />
        <button type="button" class="compact-button secondary" data-action="choose-folder" data-setting-path="${key}">${icon("folder")} Browse</button>
      </div>
      <small>${escapeHtml(helper)}</small>
    </label>
  `;
}

function queueRowsHtml(): string {
  if (state.previewing && state.queue.length === 0) return `<div class="empty-state">Building queue preview...</div>`;
  if (state.queue.length === 0) {
    return `
      <div class="empty-state">
        <strong>No queue yet</strong>
        <span>Add sources or click Preview. If Drive links.txt exists, Preview loads it automatically.</span>
      </div>
    `;
  }

  const rows = filteredQueueItems();
  if (!rows.length) {
    return `
      <div class="empty-state">
        <strong>No matching items</strong>
        <span>Clear the search box or switch queue filters.</span>
      </div>
    `;
  }

  return rows
    .map((item) => {
      const source = item.url || item.source;
      const statusClass = statusClassName(item.status);
      const canOpen = Boolean(item.transcript_path && item.status.toLowerCase() === "done");
      const canReveal = Boolean(item.downloaded_media_path || item.media_path);
      return `
        <div class="queue-row ${item.selected ? "selected" : ""}">
          <div class="select-cell"><input type="checkbox" aria-label="Select ${escapeHtml(item.title)}" data-action="toggle-queue" data-id="${escapeHtml(item.id)}" ${item.selected ? "checked" : ""} ${state.running ? "disabled" : ""} /></div>
          <div class="item-title">
            <strong>${String(item.number).padStart(2, "0")}</strong>
            <span title="${escapeHtml(item.title)}">${escapeHtml(item.title)}</span>
            <em>${escapeHtml(sourceTypeLabel(item.source_type))}</em>
          </div>
          <div class="truncate" title="${escapeHtml(source)}">${escapeHtml(source || "Local media")}</div>
          <div class="truncate" title="${escapeHtml(item.media_path)}">${escapeHtml(shortName(item.media_path) || "After download")}</div>
          <div class="status-cell">
            <span class="status-pill ${statusClass}">${escapeHtml(item.status)}</span>
            ${canOpen ? `<button class="row-action" data-action="open-transcript" data-path="${escapeHtml(item.transcript_path)}" title="Open TXT transcript">${icon("open")}</button>` : ""}
            ${canOpen && item.markdown_path ? `<button class="row-action" data-action="open-transcript" data-path="${escapeHtml(item.markdown_path)}" title="Open Markdown transcript">MD</button>` : ""}
            ${canReveal ? `<button class="row-action" data-action="reveal-media" data-path="${escapeHtml(item.downloaded_media_path || item.media_path)}" title="Reveal media">${icon("folder")}</button>` : ""}
          </div>
        </div>
      `;
    })
    .join("");
}

function sourceListHtml(mode: "main" | "settings" = "settings"): string {
  if (!state.sources.length) {
    if (mode === "main" && state.autoSource) {
      return `
        <div class="source-group">
          <div class="source-group-title">Automatic link file <span>1</span></div>
          <div class="source-row">
            <strong>auto</strong>
            <span title="${escapeHtml(state.autoSource.path)}">${escapeHtml(state.autoSource.label)} (${state.autoSource.link_count} link${state.autoSource.link_count === 1 ? "" : "s"})</span>
            <button type="button" class="remove-button" data-action="clear-sources" aria-label="Clear automatic source preview">${icon("close")}</button>
          </div>
        </div>
      `;
    }
    const message = mode === "main" ? "No sources added. Add links, media, or preview a local link file." : "No sources added.";
    return `<p class="muted">${message}</p>`;
  }
  const groups: Array<[SourceKind, string]> = [
    ["pasted", "Pasted links"],
    ["links", "Text link files"],
    ["media", "Local media"],
  ];
  return groups
    .map(([kind, label]) => {
      const rows = state.sources.map((source, index) => ({ source, index })).filter(({ source }) => source.kind === kind);
      if (!rows.length) return "";
      return `
        <div class="source-group">
          <div class="source-group-title">${escapeHtml(label)} <span>${rows.length}</span></div>
          ${rows
            .map(
              ({ source, index }) => `
                <div class="source-row">
                  <strong>${escapeHtml(sourceKindLabel(source.kind))}</strong>
                  <span title="${escapeHtml(source.value)}">${escapeHtml(source.label)}</span>
                  <button type="button" class="remove-button" data-action="remove-source" data-index="${index}" aria-label="Remove ${escapeHtml(source.label)}">${icon("close")}</button>
                </div>
              `,
            )
            .join("")}
        </div>
      `;
    })
    .join("");
}

function toolStatusHtml(): string {
  const env = state.environment;
  if (!env) return `<div class="muted">Checking tools...</div>`;

  const api: ToolStatus = {
    name: "API key",
    ok: env.api_key_ok,
    detail: env.api_key_ok ? "Saved in secure credential store" : "Open Setup and save a Gemini key",
  };

  return [api, env.ffmpeg, env.yt_dlp, env.native_engine]
    .map(
      (tool) => `
        <div class="tool-row ${tool.ok ? "ok" : "missing"}">
          <strong>${escapeHtml(tool.name)}</strong>
          <span><b>${tool.ok ? "Ready" : "Needs setup"}</b> - ${escapeHtml(tool.detail)}</span>
        </div>
      `,
    )
    .join("");
}

function logPanelHtml(): string {
  if (!state.logs.length) return `<p class="muted">Logs appear here while a run is active.</p>`;
  return `<pre class="logs">${state.logs.map(escapeHtml).join("\n")}</pre>`;
}

function historyHtml(): string {
  let history: Array<{ date: string; title: string; saved: number; failed: number; output: string; duration: string }> = [];
  try {
    history = JSON.parse(localStorage.getItem("lecturescribe.history") || "[]");
  } catch {
    history = [];
  }
  if (!history.length) return `<p class="muted">Recent completed batches will appear here.</p>`;
  return `
    <div class="history-list">
      ${history
        .map(
          (item, index) => `
            <div class="history-row">
              <div>
                <strong>${escapeHtml(item.title)}</strong>
                <span>${escapeHtml(new Date(item.date).toLocaleString())} - ${item.saved} saved, ${item.failed} failed, ${escapeHtml(item.duration)}</span>
              </div>
              <button type="button" class="row-action" data-action="open-history-output" data-history-index="${index}" title="${escapeHtml(item.output)}">${icon("folder")}</button>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}

function wireEvents() {
  document.querySelector('[data-action="paste"]')?.addEventListener("click", () => {
    (document.querySelector("#paste-dialog") as HTMLDialogElement).showModal();
  });
  document.querySelectorAll('[data-action="close-paste"]').forEach((button) => {
    button.addEventListener("click", () => (document.querySelector("#paste-dialog") as HTMLDialogElement).close());
  });
  document.querySelector("#paste-form")?.addEventListener("submit", addPastedLinks);
  document.querySelector('[data-action="add-link-file"]')?.addEventListener("click", addLinkFiles);
  document.querySelector('[data-action="add-media"]')?.addEventListener("click", addMediaFiles);
  document.querySelectorAll('[data-action="preview"]').forEach((button) => button.addEventListener("click", () => void previewQueue()));
  document.querySelector('[data-action="start"]')?.addEventListener("click", () => void startTranscription());
  document.querySelector('[data-action="cancel"]')?.addEventListener("click", () => void cancelTranscription());
  document.querySelector('[data-action="retry-failed"]')?.addEventListener("click", () => void retryFailedItems());
  document.querySelector('[data-action="open-setup"]')?.addEventListener("click", openSetup);
  document.querySelectorAll('[data-action="close-setup"]').forEach((button) => button.addEventListener("click", closeSetup));
  document.querySelector('[data-action="open-settings"]')?.addEventListener("click", openSettings);
  document.querySelectorAll('[data-action="close-settings"]').forEach((button) => button.addEventListener("click", closeSettings));
  document.querySelectorAll('[data-action="open-output"]').forEach((button) => button.addEventListener("click", () => void openOutputFolder()));
  document.querySelectorAll('[data-action="cycle-theme"]').forEach((button) => button.addEventListener("click", () => void cycleTheme()));
  document.querySelectorAll('[data-action="run-setup-test"]').forEach((button) => button.addEventListener("click", () => void runSetupTest()));
  document.querySelectorAll('[data-action="run-doctor"]').forEach((button) => button.addEventListener("click", () => void runDoctor()));
  document.querySelectorAll('[data-action="save-api-key"]').forEach((button) => button.addEventListener("click", () => void saveApiKeyFromDialog()));
  document.querySelectorAll('[data-action="save-settings"]').forEach((button) => button.addEventListener("click", () => void saveAppSettings(true)));
  document.querySelectorAll('[data-action="install-downloader"]').forEach((button) => button.addEventListener("click", () => void installDownloader()));
  document.querySelectorAll('[data-action="update-downloader"]').forEach((button) => button.addEventListener("click", () => void installDownloader(true)));
  document.querySelectorAll('[data-action="choose-downloader"]').forEach((button) => button.addEventListener("click", () => void chooseDownloader()));
  document.querySelectorAll('[data-action="install-ffmpeg"]').forEach((button) => button.addEventListener("click", () => void installFfmpeg()));
  document.querySelectorAll('[data-action="choose-ffmpeg"]').forEach((button) => button.addEventListener("click", () => void chooseFfmpeg()));
  document.querySelectorAll('[data-action="refresh-environment"]').forEach((button) => button.addEventListener("click", () => void loadEnvironment()));
  document.querySelector("#queue-search")?.addEventListener("input", (event) => {
    state.queueSearch = (event.currentTarget as HTMLInputElement).value;
    render();
  });
  document.querySelectorAll('[data-action="set-filter"]').forEach((button) => {
    button.addEventListener("click", () => {
      state.queueFilter = ((button as HTMLElement).dataset.filter as QueueFilter) || "all";
      render();
    });
  });
  document.querySelectorAll('[data-action="set-run-mode"]').forEach((button) => {
    button.addEventListener("click", () => {
      state.settings.run_mode = (button as HTMLElement).dataset.runMode || "download_transcribe";
      state.settings.skip_download = state.settings.run_mode === "transcribe_existing";
      render();
    });
  });
  document.querySelector('[data-action="clear-logs"]')?.addEventListener("click", () => {
    state.logs = [];
    render();
  });
  document.querySelector('[data-action="clear-history"]')?.addEventListener("click", () => {
    localStorage.removeItem("lecturescribe.history");
    toast("info", "History cleared.");
    render();
  });
  document.querySelector('[data-action="export-bug-report"]')?.addEventListener("click", () => void exportBugReport());
  document.querySelector('[data-action="toggle-logs"]')?.addEventListener("click", () => {
    state.logsOpen = !state.logsOpen;
    render();
  });
  document.querySelector('[data-action="clear-sources"]')?.addEventListener("click", () => {
    state.sources = [];
    state.queue = [];
    state.autoSource = null;
    state.activeRunIds = [];
    state.runTotal = 0;
    resetProgress();
    state.sourceNotice = "Sources cleared.";
    state.previewNotice = "Add sources or click Preview to load Drive links.txt if it exists.";
    render();
  });
  document.querySelector('[data-action="select-all"]')?.addEventListener("click", () => {
    const visible = new Set(filteredQueueItems().map((item) => item.id));
    state.queue.forEach((item) => {
      if (visible.has(item.id)) item.selected = true;
    });
    render();
  });
  document.querySelector('[data-action="select-none"]')?.addEventListener("click", () => {
    const visible = new Set(filteredQueueItems().map((item) => item.id));
    state.queue.forEach((item) => {
      if (visible.has(item.id)) item.selected = false;
    });
    render();
  });
  document.querySelector('[data-action="select-all-checkbox"]')?.addEventListener("change", (event) => {
    const checked = (event.currentTarget as HTMLInputElement).checked;
    const visible = new Set(filteredQueueItems().map((item) => item.id));
    state.queue.forEach((item) => {
      if (visible.has(item.id)) item.selected = checked;
    });
    render();
  });
  document.querySelectorAll('[data-action="toggle-queue"]').forEach((input) => {
    input.addEventListener("change", () => {
      const id = (input as HTMLElement).dataset.id;
      const item = state.queue.find((candidate) => candidate.id === id);
      if (item) item.selected = (input as HTMLInputElement).checked;
      render();
    });
  });
  document.querySelectorAll('[data-action="open-transcript"]').forEach((button) => {
    button.addEventListener("click", () => void openTranscript((button as HTMLElement).dataset.path ?? ""));
  });
  document.querySelectorAll('[data-action="reveal-media"]').forEach((button) => {
    button.addEventListener("click", () => void revealMedia((button as HTMLElement).dataset.path ?? ""));
  });
  document.querySelectorAll('[data-action="copy-output-path"]').forEach((button) => {
    button.addEventListener("click", () => void copyOutputPath());
  });
  document.querySelectorAll('[data-action="open-history-output"]').forEach((button) => {
    button.addEventListener("click", () => {
      const index = Number((button as HTMLElement).dataset.historyIndex);
      void openHistoryOutput(index);
    });
  });
  document.querySelectorAll('[data-action="dismiss-toast"]').forEach((button) => {
    button.addEventListener("click", () => {
      const id = Number((button as HTMLElement).dataset.toastId);
      state.toasts = state.toasts.filter((toast) => toast.id !== id);
      render();
    });
  });
  document.querySelectorAll('[data-action="choose-folder"]').forEach((button) => {
    button.addEventListener("click", () => void chooseFolder((button as HTMLElement).dataset.settingPath));
  });
  document.querySelector('[data-action="choose-cookie-file"]')?.addEventListener("click", () => void chooseCookieFile());
  document.querySelector("#settings-form")?.addEventListener("submit", saveAppSettingsFromForm);
  document.querySelectorAll<HTMLInputElement | HTMLSelectElement>("[data-setting]").forEach((input) => {
    input.addEventListener("input", () => updateSettingFromInput(input));
    input.addEventListener("change", () => updateSettingFromInput(input));
  });
  document.querySelectorAll('[data-action="remove-source"]').forEach((button) => {
    button.addEventListener("click", async () => {
      const index = Number((button as HTMLElement).dataset.index);
      if (Number.isNaN(index)) return;
      const [removed] = state.sources.splice(index, 1);
      state.sourceNotice = removed ? `Removed ${removed.label}.` : "Source removed.";
      await previewAfterSourceChange();
    });
  });
  setupDropZone();

  const settingsDialog = document.querySelector<HTMLDialogElement>("#settings-dialog");
  if (settingsDialog && state.settingsOpen && !settingsDialog.open) {
    settingsDialog.addEventListener("close", () => {
      if (state.settingsOpen) {
        state.settingsOpen = false;
        state.settingsMessage = "";
        render();
      }
    });
    settingsDialog.showModal();
  }

  const setupDialog = document.querySelector<HTMLDialogElement>("#setup-dialog");
  if (setupDialog && state.wizardOpen && !setupDialog.open) {
    setupDialog.addEventListener("close", () => {
      if (state.wizardOpen) {
        state.wizardOpen = false;
        render();
      }
    });
    setupDialog.showModal();
  }
}

async function loadEnvironment() {
  try {
    state.environment = await invoke<EnvironmentStatus>("check_environment");
    if (isTauriRuntime() && setupNeedsAttention() && !state.settingsOpen && !state.running) {
      state.wizardOpen = true;
    }
  } catch (error) {
    const message = friendlyError("Desktop bridge unavailable", error);
    state.settingsMessage = message;
    state.previewNotice = message;
  }
  render();
}

async function loadAppSettings() {
  try {
    state.settings = await invoke<AppSettings>("load_settings");
    applyTheme();
  } catch (error) {
    const message = friendlyError("Settings unavailable", error);
    state.settingsMessage = message;
    state.sourceNotice = message;
  }
  render();
}

async function addLinkFiles() {
  try {
    state.sourceNotice = "Opening .txt file picker...";
    render();
    const selected = await open({ multiple: true, directory: false, filters: [{ name: "Text files", extensions: ["txt"] }] });
    const paths = normalizeSelection(selected);
    if (!paths.length) {
      state.sourceNotice = "No .txt file selected.";
      render();
      return;
    }

    let added = 0;
    let duplicates = 0;
    let totalLinks = 0;
    for (const path of paths) {
      const count = await countLinksForFile(path);
      totalLinks += count;
      const label = `${fileLabel(path)} (${count} link${count === 1 ? "" : "s"})`;
      if (addSource({ kind: "links", value: path, label, count })) added += 1;
      else duplicates += 1;
    }

    state.sourceNotice = added
      ? `Added ${added} .txt file${added === 1 ? "" : "s"} with ${totalLinks} link${totalLinks === 1 ? "" : "s"}.`
      : `${duplicates} .txt file${duplicates === 1 ? " was" : "s were"} already added.`;
    await previewAfterSourceChange();
  } catch (error) {
    state.sourceNotice = friendlyError("Could not add .txt file", error);
    render();
  }
}

async function addMediaFiles() {
  try {
    state.sourceNotice = "Opening media picker...";
    render();
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Media", extensions: ["mp3", "m4a", "mp4", "webm", "wav", "aac", "flac", "ogg", "opus", "mov", "mkv"] }],
    });
    const paths = normalizeSelection(selected);
    if (!paths.length) {
      state.sourceNotice = "No media file selected.";
      render();
      return;
    }
    await addDroppedOrPickedPaths(paths, "picker");
  } catch (error) {
    state.sourceNotice = friendlyError("Could not add media", error);
    render();
  }
}

async function addPastedLinks(event: Event) {
  event.preventDefault();
  const textarea = document.querySelector<HTMLTextAreaElement>("#paste-text");
  const text = textarea?.value.trim() ?? "";
  const entries = parsePastedSources(text);
  if (!text) {
    state.sourceNotice = "Paste at least one YouTube link, Google Drive link, or local media path.";
    render();
    return;
  }
  if (!entries.length) {
    state.sourceNotice = "No usable sources found. Use full http/https links or one local media path per line.";
    render();
    return;
  }

  let added = 0;
  let duplicates = 0;
  for (const entry of entries) {
    if (addSource(entry)) added += 1;
    else duplicates += 1;
  }
  (document.querySelector("#paste-dialog") as HTMLDialogElement).close();
  state.sourceNotice = added
    ? `Added ${added} source group${added === 1 ? "" : "s"}${duplicates ? `, skipped ${duplicates} duplicate${duplicates === 1 ? "" : "s"}` : ""}.`
    : "Those pasted sources were already added.";
  await previewAfterSourceChange();
}

async function previewQueue() {
  await runPreview("Manual");
}

async function startTranscription() {
  if (state.running) return;

  if (!state.queue.length) {
    await runPreview("Auto");
  }

  const selected = selectedQueueItems();
  if (!selected.length) {
    state.previewNotice = "Select at least one queue item before starting.";
    render();
    return;
  }

  await loadEnvironmentWithoutRender();
  if (state.settings.run_mode !== "download_only") {
    const apiReady = await invoke<boolean>("api_key_ready");
    if (!apiReady) {
      showApiDialog();
      return;
    }
  }

  const setupError = firstBlockingSetupError(selected);
  if (setupError) {
    state.current = setupError;
    state.phase = "Needs setup";
    state.settingsOpen = true;
    state.settingsMessage = setupError;
    render();
    return;
  }

  state.running = true;
  state.cancelling = false;
  state.phase = "Starting";
  state.current = "Launching native engine...";
  state.logs = [];
  state.lastSummary = null;
  state.runStartedAt = Date.now();
  state.completedItems = 0;
  state.currentItem = 0;
  state.speed = "0 KB/s";
  state.percent = "0%";
  state.activeRunIds = selected.map((item) => item.id);
  state.runTotal = selected.length;
  selected.forEach((item) => {
    item.status = item.status === "Done" ? "Ready" : item.status;
  });
  state.totalItems = selected.length;
  state.itemProgress = `0 / ${selected.length} selected`;
  render();

  try {
    await invoke("start_transcription", { inputs: selected.map(queueInput), settings: state.settings });
  } catch (error) {
    state.running = false;
    state.cancelling = false;
    const message = String(error);
    if (message.toLowerCase().includes("api key")) showApiDialog(message);
    state.phase = "Needs attention";
    state.current = message;
    render();
  }
}

async function cancelTranscription() {
  if (!state.running) return;
  state.cancelling = true;
  state.phase = "Cancelling";
  state.current = "Stopping at the next safe point...";
  render();
  try {
    await invoke("cancel_transcription");
  } catch (error) {
    state.current = friendlyError("Cancel failed", error);
    state.cancelling = false;
    render();
  }
}

async function retryFailedItems() {
  const failed = failedQueueItems();
  if (!failed.length) return;
  state.queue.forEach((item) => {
    item.selected = failed.some((failedItem) => failedItem.id === item.id);
  });
  render();
  await startTranscription();
}

async function runSetupTest() {
  if (state.running || state.setupTesting) return;
  state.setupTesting = true;
  state.setupNotice = "Running setup test. This uses one Gemini request.";
  state.settingsMessage = state.setupNotice;
  render();
  try {
    const result = await invoke<SetupTestResult>("run_setup_test");
    state.setupNotice = result.message + (result.transcript_preview ? ` Gemini said: ${result.transcript_preview}` : "");
    state.settingsMessage = state.setupNotice;
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.setupNotice = friendlyError("Setup test failed", error);
    state.settingsMessage = state.setupNotice;
  } finally {
    state.setupTesting = false;
    render();
  }
}

async function loadPreview(): Promise<QueueItem[]> {
  return invoke<QueueItem[]>("preview_inputs", { inputs: currentInputs() });
}

async function loadDefaultSourceSummary(): Promise<DefaultSourceSummary | null> {
  try {
    return await invoke<DefaultSourceSummary | null>("default_source_summary");
  } catch {
    return null;
  }
}

async function previewAfterSourceChange() {
  await runPreview("Auto");
}

async function runPreview(mode: "Auto" | "Manual") {
  if (state.running) return;
  state.previewing = true;
  state.phase = "Previewing";
  state.current = mode === "Auto" ? "Updating queue preview..." : "Building queue preview...";
  state.previewNotice = mode === "Auto" ? "Updating preview from current sources..." : "Building preview from current sources...";
  render();

  try {
    const next = await loadPreview();
    state.queue = mergePreviewSelection(next);
    state.autoSource = currentInputs().length === 0 && state.queue.length ? await loadDefaultSourceSummary() : null;
    state.phase = "Idle";
    state.current = state.queue.length ? "Queue preview ready." : "No transcriptable items found.";
    state.totalItems = state.queue.length;
    state.completedItems = 0;
    state.runTotal = 0;
    state.itemProgress = state.queue.length ? `${selectedQueueItems().length} / ${state.queue.length} selected` : "0 selected";
    state.chunks = "0 / 0 chunks";
    state.percent = "0%";
    state.previewNotice = state.queue.length
      ? "Queue ready. Start processes selected rows only."
      : "Preview found 0 items. Add links, a .txt link file, or media files.";
  } catch (error) {
    const message = friendlyError("Preview failed", error);
    state.phase = "Needs attention";
    state.current = message;
    state.previewNotice = message;
  } finally {
    state.previewing = false;
    render();
  }
}

async function openOutputFolder() {
  try {
    await invoke("open_output_folder", { path: state.settings.output_dir || state.environment?.default_output_dir || "" });
  } catch (error) {
    state.current = String(error);
    render();
  }
}

async function openTranscript(path: string) {
  try {
    await invoke("open_transcript", { path });
  } catch (error) {
    state.current = String(error);
    render();
  }
}

async function revealMedia(path: string) {
  if (!path) return;
  try {
    await invoke("reveal_media", { path });
  } catch (error) {
    state.current = String(error);
    render();
  }
}

async function copyOutputPath() {
  const output = state.settings.output_dir || state.environment?.default_output_dir || "";
  if (!output) return;
  try {
    await navigator.clipboard.writeText(output);
    toast("success", "Output path copied.");
  } catch {
    toast("warning", "Could not copy output path.");
  }
  render();
}

async function openHistoryOutput(index: number) {
  try {
    const history = JSON.parse(localStorage.getItem("lecturescribe.history") || "[]");
    const item = history[index];
    if (item?.output) await invoke("open_output_folder", { path: item.output });
  } catch (error) {
    state.current = String(error);
    render();
  }
}

async function exportBugReport() {
  const safeSettings = {
    ...state.settings,
    cookies_file: state.settings.cookies_file ? "[set]" : "",
    cookies_from_browser: state.settings.cookies_from_browser ? "[set]" : "",
    downloader_path: state.settings.downloader_path ? "[set]" : "",
    ffmpeg_path: state.settings.ffmpeg_path ? "[set]" : "",
  };
  const report = [
    "LectureScribe diagnostic report",
    `Generated: ${new Date().toISOString()}`,
    "",
    "Setup:",
    redactSensitive(JSON.stringify(state.environment, null, 2)),
    "",
    "Settings (sanitized):",
    redactSensitive(JSON.stringify(safeSettings, null, 2)),
    "",
    "Recent logs:",
    redactSensitive(state.logs.slice(-80).join("\n") || "(none)"),
  ].join("\n");
  try {
    await navigator.clipboard.writeText(report);
    toast("success", "Sanitized bug report copied.");
  } catch {
    toast("warning", "Could not copy bug report.");
  }
  render();
}

function openSettings() {
  state.settingsOpen = true;
  state.settingsMessage = "";
  render();
}

function openSetup() {
  state.wizardOpen = true;
  state.settingsMessage = "";
  render();
}

function closeSetup() {
  state.wizardOpen = false;
  render();
}

function closeSettings() {
  state.settingsOpen = false;
  state.settingsMessage = "";
  render();
}

async function chooseFolder(key: string | undefined) {
  if (!key || !["output_dir", "download_dir", "work_dir"].includes(key)) return;
  const selected = await open({ multiple: false, directory: true });
  const [path] = normalizeSelection(selected);
  if (!path) return;

  if (key === "output_dir") state.settings.output_dir = path;
  if (key === "download_dir") state.settings.download_dir = path;
  if (key === "work_dir") state.settings.work_dir = path;
  render();
}

async function chooseCookieFile() {
  const selected = await open({ multiple: false, directory: false });
  const [path] = normalizeSelection(selected);
  if (!path) return;

  state.settings.cookies_file = path;
  render();
}

async function chooseDownloader() {
  const selected = await open({ multiple: false, directory: false, filters: [{ name: "Downloader", extensions: ["exe"] }] });
  const [path] = normalizeSelection(selected);
  if (!path) return;
  try {
    state.settings = await invoke<AppSettings>("choose_downloader", { path });
    state.settingsMessage = "Downloader path saved.";
    toast("success", "Downloader path saved.");
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = friendlyError("Could not choose downloader", error);
    toast("error", state.settingsMessage);
  }
  render();
}

async function chooseFfmpeg() {
  const selected = await open({ multiple: false, directory: false, filters: [{ name: "FFmpeg", extensions: ["exe"] }] });
  const [path] = normalizeSelection(selected);
  if (!path) return;
  try {
    state.settings = await invoke<AppSettings>("choose_ffmpeg", { path });
    state.settingsMessage = "FFmpeg path saved.";
    toast("success", "FFmpeg path saved.");
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = friendlyError("Could not choose FFmpeg", error);
    toast("error", state.settingsMessage);
  }
  render();
}

async function installDownloader(update = false) {
  state.settingsMessage = update ? "Updating downloader..." : "Installing downloader...";
  render();
  try {
    await invoke<ToolStatus>(update ? "update_downloader" : "install_downloader");
    state.settingsMessage = update ? "Downloader updated." : "Downloader installed.";
    toast("success", state.settingsMessage);
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = friendlyError("Downloader setup failed", error);
    toast("error", state.settingsMessage);
  }
  render();
}

async function installFfmpeg() {
  state.settingsMessage = "Starting FFmpeg install with winget...";
  render();
  try {
    await invoke<ToolStatus>("install_ffmpeg");
    state.settingsMessage = "FFmpeg installed.";
    toast("success", "FFmpeg installed.");
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = friendlyError("FFmpeg install failed", error);
    toast("warning", state.settingsMessage);
  }
  render();
}

function updateSettingFromInput(input: HTMLInputElement | HTMLSelectElement) {
  const key = input.dataset.setting;
  if (!key) return;

  if (key === "output_dir") state.settings.output_dir = input.value;
  if (key === "download_dir") state.settings.download_dir = input.value;
  if (key === "work_dir") state.settings.work_dir = input.value;
  if (key === "model") state.settings.model = input.value;
  if (key === "run_mode") {
    state.settings.run_mode = input.value;
    state.settings.skip_download = input.value === "transcribe_existing";
  }
  if (key === "theme") {
    state.settings.theme = input.value;
    applyTheme();
  }
  if (key === "prompt_preset") state.settings.prompt_preset = input.value;
  if (key === "transcript_format") state.settings.transcript_format = input.value;
  if (key === "cookies_from_browser") state.settings.cookies_from_browser = input.value;
  if (key === "cookies_file") state.settings.cookies_file = input.value;
  if (key === "chunk_minutes") state.settings.chunk_minutes = Math.max(1, Math.min(30, Number(input.value) || 2));
  if (key === "request_delay_seconds") state.settings.request_delay_seconds = Math.max(0, Math.min(120, Number(input.value) || 0));
  if (input instanceof HTMLInputElement && key === "skip_download") state.settings.skip_download = input.checked;
  if (input instanceof HTMLInputElement && key === "force") state.settings.force = input.checked;
}

async function saveAppSettingsFromForm(event: Event) {
  event.preventDefault();
  await saveAppSettings(true);
}

async function saveAppSettings(showMessage: boolean) {
  syncSettingsFromForm();
  try {
    state.settings = await invoke<AppSettings>("save_settings", { settings: state.settings });
    applyTheme();
    if (showMessage) state.settingsMessage = "Settings saved.";
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = String(error);
  }
  render();
}

async function cycleTheme() {
  state.settings.theme = nextThemePreference();
  applyTheme();
  render();

  try {
    state.settings = await invoke<AppSettings>("save_settings", { settings: state.settings });
    applyTheme();
  } catch (error) {
    state.settingsMessage = friendlyError("Could not save theme", error);
    toast("warning", state.settingsMessage);
  }
  render();
}

async function saveApiKeyFromDialog() {
  const input =
    document.querySelector<HTMLInputElement>("#setup-dialog[open] #setup-api-key-input") ??
    document.querySelector<HTMLInputElement>("#settings-dialog[open] #api-key-input") ??
    document.querySelector<HTMLInputElement>("#setup-api-key-input") ??
    document.querySelector<HTMLInputElement>("#api-key-input");
  const apiKey = input?.value.trim() ?? "";
  if (!apiKey) {
    state.settingsMessage = "Paste a Gemini API key first.";
    render();
    return;
  }

  try {
    await invoke("save_api_key", { apiKey });
    state.settingsMessage = "API key saved. Setup status refreshed.";
    if (input) input.value = "";
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = String(error);
  }
  render();
}

function syncSettingsFromForm() {
  document.querySelectorAll<HTMLInputElement | HTMLSelectElement>("[data-setting]").forEach(updateSettingFromInput);
}

async function loadEnvironmentWithoutRender() {
  try {
    state.environment = await invoke<EnvironmentStatus>("check_environment");
  } catch (error) {
    state.settingsMessage = String(error);
  }
}

function setupNeedsAttention(): boolean {
  const env = state.environment;
  if (!env) return false;
  return !env.api_key_ok || !env.ffmpeg.ok || !env.yt_dlp.ok;
}

async function runDoctor() {
  if (state.running) return;
  await loadEnvironmentWithoutRender();
  const summary = doctorSummaryText();
  state.setupNotice = summary;
  state.settingsMessage = summary;
  if (setupNeedsAttention()) {
    state.wizardOpen = true;
    toast("warning", "Doctor found setup items to fix.");
  } else {
    toast("success", "Doctor check passed.");
  }
  render();
}

async function setupEngineEvents() {
  try {
    await listen<string>("engine-line", (event) => handleEngineLine(event.payload));
    await listen<EngineProgress>("engine-progress", (event) => handleEngineProgress(event.payload));
    await listen<EngineDone>("engine-done", (event) => handleEngineDone(event.payload));
  } catch (error) {
    state.previewNotice = friendlyError("Progress events unavailable", error);
    render();
  }
}

async function setupNativeDragDrop() {
  if (!isTauriRuntime()) return;
  try {
    await listen<{ paths?: string[] }>("tauri://drag-drop", async (event) => {
      const paths = event.payload.paths ?? [];
      if (paths.length) await addDroppedOrPickedPaths(paths, "drop");
    });
  } catch {
    // DOM drop handling still covers most desktop file drops.
  }
}

function setupDropZone() {
  const zone = document.querySelector<HTMLElement>("[data-dropzone]");
  if (!zone) return;
  zone.addEventListener("dragover", (event) => {
    event.preventDefault();
    zone.classList.add("is-dragging");
  });
  zone.addEventListener("dragleave", () => zone.classList.remove("is-dragging"));
  zone.addEventListener("drop", async (event) => {
    event.preventDefault();
    zone.classList.remove("is-dragging");
    const files = Array.from(event.dataTransfer?.files ?? []);
    const paths = files.map((file) => (file as File & { path?: string }).path || file.name).filter(Boolean);
    if (paths.length) await addDroppedOrPickedPaths(paths, "drop");
  });
}

function handleEngineProgress(progress: EngineProgress) {
  state.phase = progress.phase || state.phase;
  state.current = friendlyProgressMessage(progress.message || state.current, progress.total_items);
  state.totalItems = progress.total_items || state.runTotal || state.totalItems;
  state.runTotal = progress.total_items || state.runTotal;
  state.completedItems = progress.completed_items;
  state.itemProgress = `${progress.completed_items} / ${progress.total_items || state.runTotal || state.totalItems || 0} selected`;
  state.chunks = `${progress.chunk_current} / ${progress.chunk_total} chunks`;
  state.speed = progress.download_speed || "0 KB/s";
  state.percent = `${Math.round(progress.percent)}%`;
  if (progress.current_item && progress.status) updateQueueStatus(progress.current_item, progress.status);
  render();
}

function handleEngineLine(line: string) {
  state.logs.push(line);
  if (state.logs.length > 500) state.logs.shift();

  const found = line.match(/\[\+\] Found (\d+) media item/);
  if (found) {
    state.runTotal = Number(found[1]);
    state.totalItems = state.runTotal;
    state.itemProgress = `${state.completedItems} / ${state.runTotal} selected`;
    state.phase = "Running";
  }

  const downloading = line.match(/\[\+\] Downloading (\d+) URL/);
  if (downloading) {
    state.phase = "Downloading";
    state.runTotal = Number(downloading[1]) || state.runTotal;
    state.current = `Preparing download for ${downloading[1]} selected link${downloading[1] === "1" ? "" : "s"}`;
  }

  const downloadProgress = line.match(/\[download\]\s+([0-9.]+)%.*?at\s+([^\s]+\/s)/);
  if (downloadProgress) {
    state.phase = "Downloading";
    state.percent = `${Math.round(Number(downloadProgress[1]))}%`;
    state.speed = downloadProgress[2];
  }

  const transcribing = line.match(/\[\+\] Transcribing\s+(\d+):\s+(.+?)\s+\((\d+) chunks\)/);
  if (transcribing) {
    const runNumber = Number(transcribing[1]);
    state.currentItem = runNumber;
    state.phase = "Transcribing";
    state.current = `Transcribing selected item ${runNumber} of ${state.runTotal || state.activeRunIds.length || transcribing[1]}: ${transcribing[2]}`;
    state.chunks = `0 / ${transcribing[3]} chunks`;
    updateQueueStatus(runNumber, "Running");
  }

  const chunk = line.match(/chunk\s+(\d+)\/(\d+)\s+@/);
  if (chunk) state.chunks = `${chunk[1]} / ${chunk[2]} chunks`;

  if (line.startsWith("[OK] Saved transcript:")) {
    const path = line.replace("[OK] Saved transcript:", "").trim();
    const item = queueItemForRunNumber(state.currentItem);
    if (item) {
      item.status = "Done";
      item.transcript_path = path;
    }
    state.completedItems += 1;
    updateOverallProgress();
  }

  if (line.startsWith("[X]")) {
    state.phase = "Needs attention";
    state.current = line.replace(/^\[X\]\s*/, "");
    updateQueueStatus(state.currentItem, "Failed");
  }

  if (line.startsWith("[~] Transcription cancelled")) {
    state.phase = "Cancelled";
    state.current = "Transcription cancelled.";
  }

  if (line.startsWith("[OK] Transcription run complete")) {
    state.phase = "Complete";
    state.current = "Transcription run complete";
    state.percent = "100%";
  }

  render();
}

function handleEngineDone(done: EngineDone) {
  state.running = false;
  state.cancelling = false;
  const duration = formatDuration(Date.now() - (state.runStartedAt || Date.now()));
  const saved = state.queue.filter((item) => item.status.toLowerCase() === "done").length;
  const failed = failedQueueItems().length;
  if (!done.success) {
    if (state.phase !== "Cancelled") state.phase = "Needs attention";
    if (!state.current || state.current === "Launching native engine...") {
      state.current = `Engine exited with code ${done.code ?? "unknown"}`;
    }
    state.lastSummary = {
      title: state.phase === "Cancelled" ? "Run cancelled" : "Run needs attention",
      saved,
      failed,
      output: state.settings.output_dir,
      duration,
    };
    toast(state.phase === "Cancelled" ? "warning" : "error", state.current);
  } else {
    state.phase = "Complete";
    state.current = "Transcription run complete";
    state.percent = "100%";
    state.lastSummary = {
      title: state.settings.run_mode === "download_only" ? "Download complete" : "Transcription complete",
      saved: state.settings.run_mode === "download_only" ? state.activeRunIds.length : Math.max(saved, state.completedItems),
      failed,
      output: state.settings.output_dir,
      duration,
    };
    toast("success", `${state.lastSummary.title}: ${state.lastSummary.saved} item${state.lastSummary.saved === 1 ? "" : "s"}.`);
    saveHistory(state.lastSummary);
  }
  void loadEnvironmentWithoutRender();
  render();
}

function updateQueueStatus(runNumber: number, status: string) {
  const item = queueItemForRunNumber(runNumber);
  if (item) item.status = status;
}

function queueItemForRunNumber(runNumber: number): QueueItem | undefined {
  if (!runNumber) return undefined;
  const activeId = state.activeRunIds[runNumber - 1];
  if (activeId) return state.queue.find((candidate) => candidate.id === activeId);
  return state.queue.find((candidate) => candidate.number === runNumber);
}

function updateOverallProgress() {
  const total = state.totalItems || state.activeRunIds.length || state.queue.length || state.completedItems || 1;
  state.itemProgress = `${state.completedItems} / ${total} selected`;
  state.percent = `${Math.round((state.completedItems / total) * 100)}%`;
}

function friendlyProgressMessage(message: string, totalItems: number): string {
  const download = message.match(/^Downloading(?: selected item)?\s+(\d+)\/(\d+)$/);
  if (download) return `Downloading selected item ${download[1]} of ${download[2]}`;
  if (message === "Downloading" && totalItems) return `Downloading selected items (${totalItems})`;
  return message;
}

function mergePreviewSelection(next: QueueItem[]): QueueItem[] {
  const previous = new Map(state.queue.map((item) => [item.id, item.selected]));
  return next.map((item) => ({ ...item, selected: previous.get(item.id) ?? item.selected ?? true }));
}

function currentInputs(): string[] {
  return state.sources.map((source) => source.value);
}

function queueInput(item: QueueItem): string {
  return item.url || item.media_path || item.source;
}

function selectedQueueItems(): QueueItem[] {
  return state.queue.filter((item) => item.selected);
}

function filteredQueueItems(): QueueItem[] {
  const query = state.queueSearch.trim().toLowerCase();
  return state.queue.filter((item) => {
    const status = item.status.toLowerCase();
    const matchesFilter =
      state.queueFilter === "all" ||
      (state.queueFilter === "selected" && item.selected) ||
      (state.queueFilter === "ready" && ["ready", "will download", "queued"].includes(status)) ||
      (state.queueFilter === "downloading" && ["downloading", "running", "transcribing"].includes(status)) ||
      (state.queueFilter === "done" && status === "done") ||
      (state.queueFilter === "failed" && failedStatus(status));
    if (!matchesFilter) return false;
    if (!query) return true;
    return [item.title, item.url, item.source, item.media_path, item.status].some((value) => value.toLowerCase().includes(query));
  });
}

function failedQueueItems(): QueueItem[] {
  return state.queue.filter((item) => failedStatus(item.status.toLowerCase()));
}

function failedStatus(status: string): boolean {
  return ["failed", "needs review", "needs attention"].includes(status);
}

function addSource(source: SourceEntry): boolean {
  const normalized = normalizeSourceValue(source.value);
  if (!normalized) return false;
  if (state.sources.some((existing) => normalizeSourceValue(existing.value) === normalized)) return false;
  state.sources.push({ ...source, value: source.value.trim() });
  return true;
}

async function addDroppedOrPickedPaths(paths: string[], origin: "drop" | "picker") {
  let addedMedia = 0;
  let addedText = 0;
  let linksFound = 0;
  let duplicates = 0;
  for (const path of paths) {
    if (isTextPath(path)) {
      const count = await countLinksForFile(path);
      linksFound += count;
      if (addSource({ kind: "links", value: path, label: `${fileLabel(path)} (${count} link${count === 1 ? "" : "s"})`, count })) addedText += 1;
      else duplicates += 1;
    } else if (looksLikeMediaPath(path)) {
      if (addSource({ kind: "media", value: path, label: fileLabel(path), count: 1 })) addedMedia += 1;
      else duplicates += 1;
    }
  }

  const parts = [];
  if (addedMedia) parts.push(`${addedMedia} media file${addedMedia === 1 ? "" : "s"}`);
  if (addedText) parts.push(`${addedText} .txt file${addedText === 1 ? "" : "s"} with ${linksFound} link${linksFound === 1 ? "" : "s"}`);
  if (duplicates) parts.push(`${duplicates} duplicate${duplicates === 1 ? "" : "s"} skipped`);
  state.sourceNotice = parts.length ? `Added from ${origin}: ${parts.join(", ")}.` : "No supported files were added. Use .txt, mp3, mp4, wav, mov, mkv, webm, flac, ogg, or opus.";
  await previewAfterSourceChange();
}

async function countLinksForFile(path: string): Promise<number> {
  try {
    return await invoke<number>("count_links_in_file", { path });
  } catch {
    return 0;
  }
}

function firstBlockingSetupError(selected: QueueItem[]): string {
  const env = state.environment;
  if (!env) return "";
  const mode = state.settings.run_mode || "download_transcribe";
  const needsTranscription = mode !== "download_only";
  if (needsTranscription && !env.ffmpeg.ok) return "FFmpeg is missing. Install FFmpeg or choose ffmpeg.exe in Setup before transcription.";
  const hasLink = selected.some((item) => item.source_type === "link" || Boolean(item.url));
  const needsDownloader = hasLink && mode !== "transcribe_existing";
  if (needsDownloader && !env.yt_dlp.ok) {
    return "Downloader is missing. Install or update it in Setup before using links.";
  }
  return "";
}

function showApiDialog(message = "LectureScribe needs a Gemini API key before it can transcribe.") {
  const oldDialog = document.querySelector("#api-dialog");
  oldDialog?.remove();

  const dialog = document.createElement("dialog");
  dialog.id = "api-dialog";
  dialog.innerHTML = `
    <div class="dialog-body api-dialog-body">
      <div class="dialog-title">
        <div>
          <h3>Gemini API key needed</h3>
          <p class="muted">${escapeHtml(message)}</p>
        </div>
        <button type="button" class="icon-button" data-action="close-api" aria-label="Close API dialog">${icon("close")}</button>
      </div>
      <div class="api-callout">
        <strong>Recommended model: gemini-3.1-flash-lite</strong>
        <span>Bring your own key from AI Studio. It stays on this computer in the OS secure credential store.</span>
      </div>
      <div class="dialog-actions">
        <button type="button" class="compact-button secondary" data-action="close-api">Later</button>
        <button type="button" class="compact-button primary" data-action="open-api-settings">Open Settings</button>
      </div>
    </div>
  `;
  document.body.appendChild(dialog);
  dialog.querySelectorAll('[data-action="close-api"]').forEach((button) => button.addEventListener("click", () => dialog.close()));
  dialog.querySelector('[data-action="open-api-settings"]')?.addEventListener("click", () => {
    dialog.close();
    state.settingsOpen = true;
    state.settingsMessage = "Paste and save your Gemini API key.";
    render();
  });
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
}

function resetProgress() {
  state.completedItems = 0;
  state.totalItems = 0;
  state.currentItem = 0;
  state.runTotal = 0;
  state.itemProgress = "0 selected";
  state.chunks = "0 / 0 chunks";
  state.percent = "0%";
  state.speed = "0 KB/s";
  state.phase = "Idle";
  state.current = "No active job";
}

function parsePastedSources(text: string): SourceEntry[] {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  const urls = lines.filter(isUrl);
  const paths = lines.filter((line) => !isUrl(line) && looksLikeMediaPath(line));
  const entries: SourceEntry[] = [];

  for (const url of urls) {
    entries.push({ kind: "pasted", value: url, label: shortUrlLabel(url), count: 1 });
  }
  for (const path of paths) entries.push({ kind: "media", value: path, label: fileLabel(path), count: 1 });

  return entries;
}

function duplicateHintCount(): number {
  const sourceValues = state.sources.map((source) => normalizeSourceValue(source.value));
  return sourceValues.length - new Set(sourceValues).size;
}

function toast(kind: Toast["kind"], message: string) {
  const id = Date.now() + Math.floor(Math.random() * 1000);
  state.toasts = [...state.toasts, { id, kind, message }].slice(-4);
  window.setTimeout(() => {
    state.toasts = state.toasts.filter((candidate) => candidate.id !== id);
    render();
  }, 4500);
}

function saveHistory(summary: RunSummary) {
  try {
    const raw = localStorage.getItem("lecturescribe.history");
    const history = raw ? JSON.parse(raw) : [];
    const next = [
      {
        date: new Date().toISOString(),
        title: summary.title,
        saved: summary.saved,
        failed: summary.failed,
        output: summary.output,
        duration: summary.duration,
      },
      ...history,
    ].slice(0, 12);
    localStorage.setItem("lecturescribe.history", JSON.stringify(next));
  } catch {
    // History is a convenience feature; failure should not interrupt transcription.
  }
}

function formatDuration(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return minutes ? `${minutes}m ${rest}s` : `${rest}s`;
}

function shortUrlLabel(url: string): string {
  if (url.length <= 58) return url;
  return `${url.slice(0, 34)}...${url.slice(-16)}`;
}

function statusClassName(status: string): string {
  const value = status.toLowerCase();
  if (["ready", "done", "running", "transcribing", "downloading"].includes(value)) return "ok";
  if (["will download", "previewing", "queued"].includes(value)) return "pending";
  if (["failed", "needs review", "needs attention"].includes(value)) return "bad";
  return "pending";
}

function sourceKindLabel(kind: SourceKind): string {
  if (kind === "pasted") return "links";
  if (kind === "links") return "txt";
  return "media";
}

function sourceTypeLabel(sourceType: string): string {
  if (sourceType === "media") return "media";
  return "link";
}

function normalizeSelection(selection: string | string[] | null): string[] {
  if (!selection) return [];
  return Array.isArray(selection) ? selection : [selection];
}

function normalizeSourceValue(value: string): string {
  return value.trim().replace(/\\/g, "/").toLowerCase();
}

function isUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

function isTextPath(value: string): boolean {
  return /\.txt$/i.test(value.trim());
}

function looksLikeMediaPath(value: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(value) || value.startsWith("\\\\") || value.startsWith("/") || /\.(mp3|m4a|mp4|webm|wav|aac|flac|ogg|opus|mov|mkv)$/i.test(value);
}

function friendlyError(prefix: string, error: unknown): string {
  const message = String(error);
  if (!isTauriRuntime() || /__TAURI|ipc|not allowed|forbidden|permission/i.test(message)) {
    return `${prefix}: use the LectureScribe desktop window for local files, preview, and transcription. The browser page cannot access the desktop bridge.`;
  }
  return `${prefix}: ${message}`;
}

function isTauriRuntime(): boolean {
  const win = window as Window & { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown };
  return Boolean(win.__TAURI_INTERNALS__ || win.__TAURI__);
}

function fileLabel(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function shortName(path: string): string {
  return path ? fileLabel(path) : "";
}

function safePercent(value: string): string {
  return /^\d+%$/.test(value) ? value : "0%";
}

function redactSensitive(value: string): string {
  return value
    .replace(/(GEMINI_API_KEY|GOOGLE_API_KEY)\s*=\s*["']?[^"'\s]+/gi, "$1=[redacted]")
    .replace(/AIza[0-9A-Za-z_-]{20,}/g, "[redacted-api-key]")
    .replace(/AQ\.[0-9A-Za-z_-]{20,}/g, "[redacted-token]")
    .replace(/("api[_-]?key"\s*:\s*")[^"]+(")/gi, "$1[redacted]$2");
}

function applyTheme() {
  const preference = themePreference();
  const media = window.matchMedia?.("(prefers-color-scheme: dark)");
  const dark = preference === "dark" || (preference === "system" && Boolean(media?.matches));
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  document.documentElement.dataset.themePreference = preference;
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

function setupThemeListener() {
  const media = window.matchMedia?.("(prefers-color-scheme: dark)");
  media?.addEventListener("change", () => {
    if (themePreference() === "system") {
      applyTheme();
    }
  });
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => {
    const escaped: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#039;" };
    return escaped[char];
  });
}

async function initialPreview() {
  if (!isTauriRuntime()) return;
  await runPreview("Auto");
}

if (!isTauriRuntime()) {
  state.sourceNotice = "Browser preview detected. Use the LectureScribe desktop window to add local files and run preview.";
  state.previewNotice = "The browser page is only for visual preview; desktop features need the Tauri app window.";
}

applyTheme();
setupThemeListener();
render();
void loadAppSettings();
void loadEnvironment();
void setupEngineEvents();
void setupNativeDragDrop();
void initialPreview();
