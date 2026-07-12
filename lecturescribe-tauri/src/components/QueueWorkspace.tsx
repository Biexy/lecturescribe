import { useDeferredValue, useEffect, useId, useRef, useState } from "react";
import { useVirtualRows } from "../hooks/useVirtualRows";
import { formatDuration, selectedTranscriptArtifact } from "../lib/backend";
import {
  isJobActive,
  itemSnapshots,
  visibleItems,
  type QueueFilter,
  type UiState,
  type WorkspaceView,
} from "../state/app-state";
import type {
  ArtifactKind,
  HistoryEntry,
  ItemSnapshot,
  ItemState,
  PreviewItem,
  ProviderKind,
} from "../types/contracts";
import { Icon } from "./Icon";
import { Button, IconButton, ProgressBar, StatusPill } from "./ui";

const filters: Array<{ value: QueueFilter; label: string }> = [
  { value: "all", label: "All" },
  { value: "selected", label: "Selected" },
  { value: "ready", label: "Ready" },
  { value: "active", label: "Active" },
  { value: "done", label: "Done" },
  { value: "failed", label: "Failed" },
];

export function QueueWorkspace({
  state,
  onWorkspace,
  onSearch,
  onFilter,
  onToggle,
  onSelect,
  onRefresh,
  onDetail,
  onOpenArtifact,
  onOpenHistory,
}: {
  state: UiState;
  onWorkspace: (view: WorkspaceView) => void;
  onSearch: (value: string) => void;
  onFilter: (filter: QueueFilter) => void;
  onToggle: (id: string, selected: boolean) => void;
  onSelect: (ids: string[], selected: boolean) => void;
  onRefresh: () => void;
  onDetail: (id: string) => void;
  onOpenArtifact: (itemId: string, kind: ArtifactKind, reveal?: boolean) => void;
  onOpenHistory: (entry: HistoryEntry) => void;
}) {
  return (
    <main className="workspace-panel">
      <header className="workspace-header">
        <div className="workspace-tabs" role="tablist" aria-label="Workspace">
          <button
            aria-selected={state.workspaceView === "queue"}
            className={state.workspaceView === "queue" ? "is-active" : ""}
            onClick={() => onWorkspace("queue")}
            role="tab"
            type="button"
          >
            Queue <span>{state.preview?.items.length ?? 0}</span>
          </button>
          <button
            aria-selected={state.workspaceView === "history"}
            className={state.workspaceView === "history" ? "is-active" : ""}
            onClick={() => onWorkspace("history")}
            role="tab"
            type="button"
          >
            History <span>{state.history.length}</span>
          </button>
        </div>
        {state.workspaceView === "queue" && (
          <QueueTools
            state={state}
            onFilter={onFilter}
            onRefresh={onRefresh}
            onSearch={onSearch}
            onSelect={onSelect}
          />
        )}
      </header>
      {state.workspaceView === "queue" ? (
        <QueueTable
          state={state}
          onDetail={onDetail}
          onOpenArtifact={onOpenArtifact}
          onToggle={onToggle}
        />
      ) : (
        <HistoryTable entries={state.history} onOpen={onOpenHistory} />
      )}
    </main>
  );
}

function QueueTools({ state, onSearch, onFilter, onSelect, onRefresh }: {
  state: UiState;
  onSearch: (value: string) => void;
  onFilter: (filter: QueueFilter) => void;
  onSelect: (ids: string[], selected: boolean) => void;
  onRefresh: () => void;
}) {
  const visible = visibleItems(state);
  const selectable = visible.filter((item) => !item.duplicate_of && !item.error).map((item) => item.id);
  const allSelected = selectable.length > 0 && selectable.every((id) => state.selected[id]);
  return (
    <div className="queue-tools">
      <label className="search-box">
        <Icon name="search" size={16} />
        <span className="sr-only">Search queue</span>
        <input
          onInput={(event) => onSearch(event.currentTarget.value)}
          placeholder="Search title or source"
          type="search"
          value={state.search}
        />
      </label>
      <div className="queue-filters" aria-label="Queue filter">
        {filters.map((filter) => (
          <button
            aria-pressed={state.filter === filter.value}
            className={state.filter === filter.value ? "is-active" : ""}
            key={filter.value}
            onClick={() => onFilter(filter.value)}
            type="button"
          >
            {filter.label}
          </button>
        ))}
      </div>
      <div className="selection-tools">
        <Button
          disabled={isJobActive(state.job) || selectable.length === 0}
          onClick={() => onSelect(selectable, !allSelected)}
          size="sm"
          title={isJobActive(state.job) ? "Selection is locked while a run is active." : undefined}
          variant="ghost"
        >
          {allSelected ? "Select none" : "Select visible"}
        </Button>
        <IconButton
          disabled={state.previewLoading || state.sources.length === 0}
          icon="refresh"
          label="Refresh queue preview"
          onClick={onRefresh}
        />
      </div>
    </div>
  );
}

function QueueTable({ state, onToggle, onDetail, onOpenArtifact }: {
  state: UiState;
  onToggle: (id: string, selected: boolean) => void;
  onDetail: (id: string) => void;
  onOpenArtifact: (itemId: string, kind: ArtifactKind, reveal?: boolean) => void;
}) {
  const deferredSearch = useDeferredValue(state.search);
  const scopedState = deferredSearch === state.search ? state : { ...state, search: deferredSearch };
  const items = visibleItems(scopedState);
  const snapshots = itemSnapshots(state.job);
  const virtual = useVirtualRows(items.length, 54, 8);

  if (state.previewLoading && !state.preview) {
    return <QueueEmpty icon="refresh" title="Building your queue" message="Inspecting links, local media, duration, and existing outputs." loading />;
  }
  if (state.previewError && !state.preview) {
    return <QueueEmpty icon="alert" title="Preview could not finish" message={state.previewError.user_message} />;
  }
  if (!state.preview || state.preview.items.length === 0) {
    return <QueueEmpty icon="file-audio" title="Your queue is empty" message="Add a link file, paste URLs, or choose local audio and video." />;
  }
  if (items.length === 0) {
    return <QueueEmpty icon="search" title="No items match" message="Change the search or queue filter to see other items." />;
  }

  return (
    <div className="queue-table">
      <div className="queue-grid queue-grid-head" role="row">
        <span aria-hidden="true" />
        <span>Title and source</span>
        <span>Planned action</span>
        <span>Status</span>
        <span aria-hidden="true" />
      </div>
      <div
        className="queue-scroll modern-scrollbar"
        onScroll={virtual.onScroll}
        ref={virtual.scrollRef}
        role="rowgroup"
      >
        <div className="virtual-space" style={{ height: `${virtual.totalHeight}px` }}>
          {virtual.rows.map((row) => {
            const item = items[row.index];
            return (
              <QueueRow
                active={isJobActive(state.job)}
                item={item}
                key={item.id}
                onDetail={onDetail}
                onOpenArtifact={onOpenArtifact}
                onToggle={onToggle}
                mode={state.mode}
                selected={Boolean(state.selected[item.id])}
                settings={state.settings}
                snapshot={snapshots.get(item.id)}
                top={row.start}
              />
            );
          })}
        </div>
      </div>
      <footer className="queue-footer">
        <span>{items.length} visible</span>
        <span>{Object.values(state.selected).filter(Boolean).length} selected</span>
        {state.preview.warnings.length > 0 && <span className="queue-warning"><Icon name="alert" size={13} /> {state.preview.warnings[0]}</span>}
      </footer>
    </div>
  );
}

function QueueRow({
  item,
  snapshot,
  selected,
  active,
  top,
  settings,
  mode,
  onToggle,
  onDetail,
  onOpenArtifact,
}: {
  item: PreviewItem;
  snapshot?: ItemSnapshot;
  selected: boolean;
  active: boolean;
  top: number;
  settings: UiState["settings"];
  mode: UiState["mode"];
  onToggle: (id: string, selected: boolean) => void;
  onDetail: (id: string) => void;
  onOpenArtifact: (itemId: string, kind: ArtifactKind, reveal?: boolean) => void;
}) {
  const status = snapshot?.state ?? item.status;
  const progress = snapshot?.progress;
  const percent = progress?.total ? Math.round(progress.current / progress.total * 100) : 0;
  const blocked = Boolean(item.duplicate_of || item.error || item.status === "blocked");
  const transcriptKind = settings ? selectedTranscriptArtifact(settings.output_formats) : "text_transcript";
  const transcriptReady = snapshot?.artifacts.some((artifact) => artifact.kind === transcriptKind);
  const mediaReady = snapshot?.artifacts.some((artifact) => artifact.kind === "downloaded_media");
  return (
    <div className="queue-grid queue-row" role="row" style={{ height: "54px", transform: `translateY(${top}px)` }}>
      <input
        aria-label={`Select ${item.title}`}
        checked={selected}
        disabled={blocked || active}
        onChange={(event) => onToggle(item.id, event.currentTarget.checked)}
        title={blocked ? item.error?.user_message ?? "Duplicate items are not processed twice." : active ? "Selection is locked during a run." : undefined}
        type="checkbox"
      />
      <button className="item-identity" onClick={() => onDetail(item.id)} type="button">
        <Thumbnail item={item} />
        <span>
          <strong dir="auto" title={item.title}>{item.title}</strong>
          <small><ProviderIcon provider={item.provider} /> {providerLabel(item.provider)}{item.duration_seconds ? ` - ${formatDuration(item.duration_seconds)}` : ""}</small>
        </span>
      </button>
      <div className="planned-action">
        <span>{snapshot ? actionLabel(snapshot.item.action) : previewActionLabel(item, mode)}</span>
        <small dir="auto">{item.expected_media_name ?? item.media_path ?? "Media resolved when the run starts"}</small>
      </div>
      <div className="row-status">
        <StatusPill tone={statusTone(status)}>{statusLabel(status, item)}</StatusPill>
        {progress && progress.total && !["complete", "reused", "failed", "cancelled"].includes(status) && (
          <div className="row-progress"><ProgressBar label={`${item.title} progress`} value={percent} /><span>{percent}%</span></div>
        )}
      </div>
      <RowActionsMenu
        item={item}
        mediaReady={Boolean(mediaReady)}
        onDetail={onDetail}
        onOpenArtifact={onOpenArtifact}
        transcriptKind={transcriptKind}
        transcriptReady={Boolean(transcriptReady)}
      />
    </div>
  );
}

function Thumbnail({ item }: { item: PreviewItem }) {
  if (item.thumbnail_url) {
    return <img alt="" loading="lazy" onError={(event) => { event.currentTarget.hidden = true; }} referrerPolicy="no-referrer" src={item.thumbnail_url} />;
  }
  return <span className="thumbnail-placeholder"><Icon name={item.provider === "local" ? "file-audio" : "video"} size={17} /></span>;
}

function ProviderIcon({ provider }: { provider: ProviderKind }) {
  return <Icon name={provider === "local" ? "file-audio" : provider === "google_drive" ? "folder" : "link"} size={12} />;
}

function HistoryTable({ entries, onOpen }: { entries: HistoryEntry[]; onOpen: (entry: HistoryEntry) => void }) {
  if (entries.length === 0) {
    return <QueueEmpty icon="history" title="No completed runs yet" message="Finished and interrupted batches will appear here." />;
  }
  return (
    <div className="history-list modern-scrollbar">
      {entries.map((entry) => (
        <button className="history-row" key={entry.job_id} onClick={() => onOpen(entry)} type="button">
          <Icon name="history" size={17} />
          <span><strong>{entry.title}</strong><small>{new Date(entry.started_at).toLocaleString()} - {entry.mode === "transcribe" ? "Transcription" : "Download"}</small></span>
          <span className="history-counts">{entry.counts.complete + entry.counts.reused} done{entry.counts.failed ? ` - ${entry.counts.failed} failed` : ""}</span>
          <StatusPill tone={entry.state === "complete" ? "success" : entry.state === "failed" ? "danger" : "warning"}>{entry.state}</StatusPill>
          <Icon name="chevron-right" size={16} />
        </button>
      ))}
    </div>
  );
}

function QueueEmpty({ icon, title, message, loading = false }: { icon: "refresh" | "alert" | "file-audio" | "search" | "history"; title: string; message: string; loading?: boolean }) {
  return <div className="queue-empty">{loading ? <span className="spinner large" /> : <Icon name={icon} size={24} />}<h3>{title}</h3><p>{message}</p></div>;
}

function providerLabel(provider: ProviderKind): string {
  return provider === "you_tube" ? "YouTube" : provider === "google_drive" ? "Google Drive" : provider === "local" ? "Local media" : "Web link";
}

function actionLabel(action: string): string {
  return ({ download_and_transcribe: "Download + transcribe", reuse_media_and_transcribe: "Reuse media + transcribe", transcribe_local: "Transcribe local", reuse_transcript: "Reuse transcript", download_only: "Download only", excluded: "Excluded", blocked: "Blocked" } as Record<string, string>)[action] ?? action;
}

function previewActionLabel(item: PreviewItem, mode: UiState["mode"]): string {
  if (mode === "download") return item.provider === "local" ? "Already local" : "Download media";
  if (item.existing_transcript_path) return "Reuse transcript";
  if (item.provider === "local") return "Transcribe local";
  return item.existing_media_path ? "Reuse media + transcribe" : "Download + transcribe";
}

function RowActionsMenu({
  item,
  transcriptKind,
  transcriptReady,
  mediaReady,
  onDetail,
  onOpenArtifact,
}: {
  item: PreviewItem;
  transcriptKind: ArtifactKind;
  transcriptReady: boolean;
  mediaReady: boolean;
  onDetail: (id: string) => void;
  onOpenArtifact: (itemId: string, kind: ArtifactKind, reveal?: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuId = useId();

  const focusFirstItem = () => {
    requestAnimationFrame(() => {
      rootRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus();
    });
  };

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [open]);

  const closeAndRun = (action: () => void) => {
    setOpen(false);
    action();
  };

  return (
    <div className="row-menu" ref={rootRef}>
      <button
        aria-controls={menuId}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={`Actions for ${item.title}`}
        className="row-menu-trigger"
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown") return;
          event.preventDefault();
          setOpen(true);
          focusFirstItem();
        }}
        ref={triggerRef}
        title="Item actions"
        type="button"
      >
        <Icon name="more" size={17} />
      </button>
      {open && (
        <div
          aria-label={`Actions for ${item.title}`}
          className="row-menu-popover"
          id={menuId}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              setOpen(false);
              triggerRef.current?.focus();
              return;
            }
            if (event.key === "Tab") {
              setOpen(false);
              return;
            }
            if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
            event.preventDefault();
            const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
            if (!items.length) return;
            const current = items.indexOf(document.activeElement as HTMLButtonElement);
            const next = event.key === "Home"
              ? 0
              : event.key === "End"
                ? items.length - 1
                : (Math.max(0, current) + (event.key === "ArrowUp" ? -1 : 1) + items.length) % items.length;
            items[next].focus();
          }}
          role="menu"
        >
          <button onClick={() => closeAndRun(() => onDetail(item.id))} role="menuitem" type="button"><Icon name="eye" size={15} /> View details</button>
          <button disabled={!transcriptReady} onClick={() => closeAndRun(() => onOpenArtifact(item.id, transcriptKind))} role="menuitem" type="button"><Icon name="file" size={15} /> {transcriptReady ? "Open transcript" : "Transcript not ready"}</button>
          <button disabled={!mediaReady} onClick={() => closeAndRun(() => onOpenArtifact(item.id, "downloaded_media", true))} role="menuitem" type="button"><Icon name="folder" size={15} /> {mediaReady ? "Reveal media" : "Media not kept"}</button>
        </div>
      )}
    </div>
  );
}
function statusTone(status: ItemState): "neutral" | "info" | "success" | "warning" | "danger" {
  if (["complete", "reused"].includes(status)) return "success";
  if (["failed", "blocked"].includes(status)) return "danger";
  if (["waiting", "cancelled", "excluded"].includes(status)) return "warning";
  if (["ready", "queued", "inspecting"].includes(status)) return "neutral";
  return "info";
}

function statusLabel(status: ItemState, item: PreviewItem): string {
  if (item.duplicate_of) return "Duplicate";
  return status.replaceAll("_", " ").replace(/^./, (value) => value.toUpperCase());
}
