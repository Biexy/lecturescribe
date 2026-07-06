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
  transcript_path: string;
  selected: boolean;
  status: string;
  error: string | null;
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

const defaultSettings: AppSettings = {
  output_dir: "",
  download_dir: "",
  work_dir: "",
  model: "gemini-3.1-flash-lite",
  chunk_minutes: 2,
  request_delay_seconds: 5,
  cookies_from_browser: "",
  cookies_file: "",
  skip_download: false,
  force: false,
};

const state = {
  sources: [] as SourceEntry[],
  queue: [] as QueueItem[],
  activeRunIds: [] as string[],
  environment: null as EnvironmentStatus | null,
  settings: { ...defaultSettings },
  logs: [] as string[],
  running: false,
  previewing: false,
  cancelling: false,
  setupTesting: false,
  settingsOpen: false,
  logsOpen: false,
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
};

function icon(name: string): string {
  return icons[name] ?? "";
}

function render() {
  const selected = selectedQueueItems().length;
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
          <button class="ghost" data-action="open-output">${icon("folder")} Open output</button>
          <button class="ghost" data-action="run-setup-test" ${state.setupTesting || state.running ? "disabled" : ""}>${icon("shield")} Test setup</button>
          <button class="ghost" data-action="open-settings">${icon("gear")} Settings</button>
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
              <p>${selected} selected${state.previewing ? " - updating preview" : ""}</p>
            </div>
            <div class="queue-actions">
              <button class="small-button secondary" data-action="select-all" ${state.queue.length === 0 || state.running ? "disabled" : ""}>Select all</button>
              <button class="small-button secondary" data-action="select-none" ${state.queue.length === 0 || state.running ? "disabled" : ""}>Select none</button>
              <button class="icon-button" data-action="preview" aria-label="Refresh preview" ${state.previewing || state.running ? "disabled" : ""}>${icon("refresh")}</button>
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
        <div class="metric">${icon("list")} ${escapeHtml(state.itemProgress)}</div>
        <div class="divider"></div>
        <div class="metric">${icon("film")} ${escapeHtml(state.chunks)}</div>
        <div class="divider"></div>
        <div class="speed-block">${icon("speed")}<div><span class="muted">Download speed</span><strong>${escapeHtml(state.speed)}</strong></div></div>
        <div class="bar"><div class="bar-fill" style="width: ${safePercent(state.percent)}"></div></div>
        <div class="percent">${escapeHtml(state.percent)}</div>
      </footer>
    </main>

    ${pasteDialogHtml()}
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
        <button class="primary" data-action="paste" ${state.running ? "disabled" : ""}>${icon("link")} Paste links</button>
        <button class="secondary" data-action="add-media" ${state.running ? "disabled" : ""}>${icon("folder")} Add media</button>
      </div>
      <div class="source-tools">
        <button class="text-button" data-action="add-link-file" ${state.running ? "disabled" : ""}>${icon("upload")} Add .txt link file</button>
        <button class="text-button" data-action="clear-sources" ${state.sources.length === 0 || state.running ? "disabled" : ""}>Clear sources</button>
      </div>
      <p class="drop-hint">Drag files here, or use the buttons above.</p>
    </div>
    ${sourceSummaryHtml()}
    ${sourceNoticeHtml(state.sourceNotice)}
  `;
}

function previewContent(): string {
  return `
    <button class="wide secondary" data-action="preview" ${state.previewing || state.running ? "disabled" : ""}>${icon("eye")} ${state.previewing ? "Updating preview..." : "Preview now"}</button>
    <div class="hint-grid">
      <div><strong>${state.queue.length}</strong><span>items found</span></div>
      <div><strong>${selectedQueueItems().length}</strong><span>selected</span></div>
      <div><strong>${duplicateHintCount()}</strong><span>duplicates skipped</span></div>
    </div>
    ${sourceNoticeHtml(state.previewNotice)}
  `;
}

function startContent(): string {
  if (state.running) {
    return `
      <button class="wide danger" data-action="cancel" ${state.cancelling ? "disabled" : ""}>${icon("stop")} ${state.cancelling ? "Cancelling..." : "Cancel run"}</button>
      <p class="notice-line">Current work stops at the next safe point. Completed chunks stay cached.</p>
    `;
  }

  const hasFailures = failedQueueItems().length > 0;
  const selected = selectedQueueItems().length;
  return `
    <button class="wide primary" data-action="start" ${state.previewing || (state.queue.length > 0 && selected === 0) ? "disabled" : ""}>${icon("play")} Start transcription</button>
    <div class="start-actions">
      <button class="small-button secondary" data-action="retry-failed" ${hasFailures ? "" : "disabled"}>${icon("retry")} Retry failed</button>
      <button class="small-button secondary" data-action="open-output">${icon("folder")} Output folder</button>
    </div>
  `;
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

function sourceSummaryHtml(): string {
  const pasted = state.sources.filter((source) => source.kind === "pasted").reduce((sum, source) => sum + (source.count ?? 1), 0);
  const textFiles = state.sources.filter((source) => source.kind === "links").length;
  const textLinks = state.sources.filter((source) => source.kind === "links").reduce((sum, source) => sum + (source.count ?? 0), 0);
  const media = state.sources.filter((source) => source.kind === "media").length;
  const hasSources = state.sources.length > 0;
  return `
    <div class="source-summary ${hasSources ? "has-sources" : ""}">
      <div>
        <strong>${hasSources ? `${state.sources.length} source groups added` : "No manual sources yet"}</strong>
        <span>${pasted} pasted links, ${textFiles} text files${textLinks ? ` (${textLinks} links)` : ""}, ${media} media files.</span>
      </div>
      ${hasSources ? `<span class="mini-pill">${state.sources.length}</span>` : ""}
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

function settingsDialogHtml(): string {
  const s = state.settings;
  return `
    <dialog id="settings-dialog" class="settings-dialog">
      <form class="dialog-body settings-body" id="settings-form">
        <div class="settings-header">
          <div>
            <h3>Settings</h3>
            <p class="muted">Setup, folders, model, private download options, and logs.</p>
          </div>
          <button type="button" class="icon-button" data-action="close-settings" aria-label="Close settings">${icon("close")}</button>
        </div>

        <div class="settings-grid">
          <section class="settings-section">
            <div class="section-head">
              <h4>Setup status</h4>
              <button type="button" class="text-button" data-action="refresh-environment">Refresh</button>
            </div>
            <div class="tool-grid">${toolStatusHtml()}</div>
            <button type="button" class="compact-button secondary" data-action="run-setup-test" ${state.setupTesting || state.running ? "disabled" : ""}>${icon("shield")} Run setup test</button>
            <p class="notice-line">${escapeHtml(state.setupNotice || "The setup test uses one Gemini request.")}</p>
          </section>

          <section class="settings-section">
            <h4>API key</h4>
            <label class="field-stack">
              <span>Gemini API key</span>
              <div class="inline-field">
                <input id="api-key-input" type="password" autocomplete="off" placeholder="Paste key to save locally" />
                <button type="button" class="compact-button secondary" data-action="save-api-key">${icon("key")} Save</button>
              </div>
              <small>Stored locally in .env. Existing keys are never shown here.</small>
            </label>
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
              <h4>Activity logs</h4>
              <div class="section-actions">
                <button type="button" class="text-button" data-action="toggle-logs">${state.logsOpen ? "Hide logs" : "Show logs"}</button>
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

  return state.queue
    .map((item) => {
      const source = item.url || item.source;
      const statusClass = statusClassName(item.status);
      const canOpen = Boolean(item.transcript_path && item.status.toLowerCase() === "done");
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
            ${canOpen ? `<button class="row-action" data-action="open-transcript" data-path="${escapeHtml(item.transcript_path)}">${icon("open")} Open</button>` : ""}
          </div>
        </div>
      `;
    })
    .join("");
}

function sourceListHtml(mode: "main" | "settings" = "settings"): string {
  if (!state.sources.length) {
    const message = mode === "main" ? "No manual sources added. Preview can still use Drive links.txt." : "No sources added.";
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
    detail: env.api_key_ok ? "Saved locally" : "Open Settings and save a Gemini key",
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
  document.querySelector('[data-action="open-settings"]')?.addEventListener("click", openSettings);
  document.querySelectorAll('[data-action="close-settings"]').forEach((button) => button.addEventListener("click", closeSettings));
  document.querySelectorAll('[data-action="open-output"]').forEach((button) => button.addEventListener("click", () => void openOutputFolder()));
  document.querySelectorAll('[data-action="run-setup-test"]').forEach((button) => button.addEventListener("click", () => void runSetupTest()));
  document.querySelector('[data-action="save-api-key"]')?.addEventListener("click", () => void saveApiKeyFromDialog());
  document.querySelector('[data-action="refresh-environment"]')?.addEventListener("click", () => void loadEnvironment());
  document.querySelector('[data-action="clear-logs"]')?.addEventListener("click", () => {
    state.logs = [];
    render();
  });
  document.querySelector('[data-action="toggle-logs"]')?.addEventListener("click", () => {
    state.logsOpen = !state.logsOpen;
    render();
  });
  document.querySelector('[data-action="clear-sources"]')?.addEventListener("click", () => {
    state.sources = [];
    state.queue = [];
    state.activeRunIds = [];
    resetProgress();
    state.sourceNotice = "Sources cleared.";
    state.previewNotice = "Add sources or click Preview to load Drive links.txt if it exists.";
    render();
  });
  document.querySelector('[data-action="select-all"]')?.addEventListener("click", () => {
    state.queue.forEach((item) => (item.selected = true));
    render();
  });
  document.querySelector('[data-action="select-none"]')?.addEventListener("click", () => {
    state.queue.forEach((item) => (item.selected = false));
    render();
  });
  document.querySelector('[data-action="select-all-checkbox"]')?.addEventListener("change", (event) => {
    const checked = (event.currentTarget as HTMLInputElement).checked;
    state.queue.forEach((item) => (item.selected = checked));
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
  document.querySelectorAll('[data-action="choose-folder"]').forEach((button) => {
    button.addEventListener("click", () => void chooseFolder((button as HTMLElement).dataset.settingPath));
  });
  document.querySelector('[data-action="choose-cookie-file"]')?.addEventListener("click", () => void chooseCookieFile());
  document.querySelector("#settings-form")?.addEventListener("submit", saveAppSettingsFromForm);
  document.querySelectorAll<HTMLInputElement>("[data-setting]").forEach((input) => {
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
}

async function loadEnvironment() {
  try {
    state.environment = await invoke<EnvironmentStatus>("check_environment");
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

  const apiReady = await invoke<boolean>("api_key_ready");
  if (!apiReady) {
    showApiDialog();
    return;
  }

  await loadEnvironmentWithoutRender();
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
  state.completedItems = 0;
  state.currentItem = 0;
  state.speed = "0 KB/s";
  state.percent = "0%";
  state.activeRunIds = selected.map((item) => item.id);
  selected.forEach((item) => {
    item.status = item.status === "Done" ? "Ready" : item.status;
  });
  state.totalItems = selected.length;
  state.itemProgress = `0 / ${selected.length} items`;
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
    state.phase = "Idle";
    state.current = state.queue.length ? "Queue preview ready." : "No transcriptable items found.";
    state.totalItems = state.queue.length;
    state.completedItems = 0;
    state.itemProgress = `0 / ${state.queue.length} items`;
    state.chunks = "0 / 0 chunks";
    state.percent = "0%";
    state.previewNotice = state.queue.length
      ? `Preview ready: ${state.queue.length} item${state.queue.length === 1 ? "" : "s"} found, ${selectedQueueItems().length} selected.`
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

function openSettings() {
  state.settingsOpen = true;
  state.settingsMessage = "";
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

function updateSettingFromInput(input: HTMLInputElement) {
  const key = input.dataset.setting;
  if (!key) return;

  if (key === "output_dir") state.settings.output_dir = input.value;
  if (key === "download_dir") state.settings.download_dir = input.value;
  if (key === "work_dir") state.settings.work_dir = input.value;
  if (key === "model") state.settings.model = input.value;
  if (key === "cookies_from_browser") state.settings.cookies_from_browser = input.value;
  if (key === "cookies_file") state.settings.cookies_file = input.value;
  if (key === "chunk_minutes") state.settings.chunk_minutes = Math.max(1, Math.min(30, Number(input.value) || 2));
  if (key === "request_delay_seconds") state.settings.request_delay_seconds = Math.max(0, Math.min(120, Number(input.value) || 0));
  if (key === "skip_download") state.settings.skip_download = input.checked;
  if (key === "force") state.settings.force = input.checked;
}

async function saveAppSettingsFromForm(event: Event) {
  event.preventDefault();
  await saveAppSettings(true);
}

async function saveAppSettings(showMessage: boolean) {
  syncSettingsFromForm();
  try {
    state.settings = await invoke<AppSettings>("save_settings", { settings: state.settings });
    if (showMessage) state.settingsMessage = "Settings saved.";
    await loadEnvironmentWithoutRender();
  } catch (error) {
    state.settingsMessage = String(error);
  }
  render();
}

async function saveApiKeyFromDialog() {
  const input = document.querySelector<HTMLInputElement>("#api-key-input");
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
  document.querySelectorAll<HTMLInputElement>("[data-setting]").forEach(updateSettingFromInput);
}

async function loadEnvironmentWithoutRender() {
  try {
    state.environment = await invoke<EnvironmentStatus>("check_environment");
  } catch (error) {
    state.settingsMessage = String(error);
  }
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
  state.current = progress.message || state.current;
  state.totalItems = progress.total_items || state.totalItems;
  state.completedItems = progress.completed_items;
  state.itemProgress = `${progress.completed_items} / ${progress.total_items || state.totalItems || 0} items`;
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
    state.totalItems = Number(found[1]);
    state.itemProgress = `${state.completedItems} / ${state.totalItems} items`;
    state.phase = "Running";
  }

  const downloading = line.match(/\[\+\] Downloading (\d+) URL/);
  if (downloading) {
    state.phase = "Downloading";
    state.current = `Downloading ${downloading[1]} item(s)`;
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
    state.current = transcribing[2];
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
  if (!done.success) {
    if (state.phase !== "Cancelled") state.phase = "Needs attention";
    if (!state.current || state.current === "Launching native engine...") {
      state.current = `Engine exited with code ${done.code ?? "unknown"}`;
    }
  } else {
    state.phase = "Complete";
    state.current = "Transcription run complete";
    state.percent = "100%";
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
  state.itemProgress = `${state.completedItems} / ${total} items`;
  state.percent = `${Math.round((state.completedItems / total) * 100)}%`;
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

function failedQueueItems(): QueueItem[] {
  return state.queue.filter((item) => ["failed", "needs review", "needs attention"].includes(item.status.toLowerCase()));
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
  if (!env.ffmpeg.ok) return "FFmpeg is missing. Install FFmpeg and refresh setup before starting.";
  const hasLink = selected.some((item) => item.source_type === "link" || Boolean(item.url));
  if (hasLink && !state.settings.skip_download && !env.yt_dlp.ok) {
    return "yt-dlp is missing. Add yt-dlp.exe beside the app or install yt-dlp before downloading links.";
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
        <span>Bring your own key from AI Studio. It stays on this computer in the local .env file.</span>
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
  state.itemProgress = "0 / 0 items";
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

  if (urls.length) entries.push({ kind: "pasted", value: urls.join("\n"), label: `${urls.length} pasted link${urls.length === 1 ? "" : "s"}`, count: urls.length });
  for (const path of paths) entries.push({ kind: "media", value: path, label: fileLabel(path), count: 1 });

  return entries;
}

function duplicateHintCount(): number {
  const sourceValues = state.sources.map((source) => normalizeSourceValue(source.value));
  return sourceValues.length - new Set(sourceValues).size;
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

render();
void loadAppSettings();
void loadEnvironment();
void setupEngineEvents();
void setupNativeDragDrop();
void initialPreview();
