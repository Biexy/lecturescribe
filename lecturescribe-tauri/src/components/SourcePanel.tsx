import type { UiState } from "../state/app-state";
import { isJobActive, selectedItemIds } from "../state/app-state";
import type { RunMode, SourceFileSummary, SourceKind } from "../types/contracts";
import { Icon } from "./Icon";
import { Button, IconButton, SegmentedControl, StatusPill } from "./ui";

const sourceLabels: Record<SourceKind, string> = {
  pasted_link: "Pasted links",
  text_file: "Link file",
  local_media: "Local media",
  directory: "Media folder",
  automatic_file: "Automatic link file",
};

export function SourcePanel({
  state,
  onPaste,
  onAddMedia,
  onAddText,
  onAddFolder,
  onRemove,
  onClear,
  onRefresh,
  onMode,
  onReview,
}: {
  state: UiState;
  onPaste: () => void;
  onAddMedia: () => void;
  onAddText: () => void;
  onAddFolder: () => void;
  onRemove: (id: string) => void;
  onClear: () => void;
  onRefresh: () => void;
  onMode: (mode: RunMode) => void;
  onReview: () => void;
}) {
  const active = isJobActive(state.job);
  const selectedCount = selectedItemIds(state).length;
  const found = state.preview?.items.filter((item) => !item.duplicate_of).length ?? 0;
  const duplicates = state.preview?.duplicate_count ?? 0;
  const shownSources = state.sources.slice(0, 4);
  const hiddenSources = Math.max(0, state.sources.length - shownSources.length);
  const disabledReason = active
    ? "Finish or cancel the active run before starting another."
    : state.previewLoading
      ? "Wait for the automatic preview to finish."
      : selectedCount === 0
        ? "Select at least one ready queue item."
        : undefined;

  return (
    <aside className="source-panel" aria-label="Sources and run controls">
      <section className="source-section add-section">
        <div className="section-heading">
          <div>
            <span className="eyebrow">Sources</span>
            <h2>Add audio, video, or links</h2>
          </div>
          {state.sources.length > 0 && (
            <button className="text-button" disabled={active} onClick={onClear} type="button">
              Clear
            </button>
          )}
        </div>
        <div className="source-actions">
          <Button disabled={active} icon="link" onClick={onPaste} variant="primary">
            Paste links
          </Button>
          <Button disabled={active} icon="file-audio" onClick={onAddMedia}>
            Add media
          </Button>
        </div>
        <div className="source-secondary-actions">
          <button disabled={active} onClick={onAddText} type="button">
            <Icon name="file-up" size={15} /> Add .txt link file
          </button>
          <button disabled={active} onClick={onAddFolder} type="button">
            <Icon name="folder" size={15} /> Add folder
          </button>
        </div>
        <p className="source-hint">Drop files here, or use YouTube and Google Drive links.</p>

        {state.sources.length === 0 ? (
          <div className="source-empty">
            <Icon name="layers" size={18} />
            <div>
              <strong>No sources yet</strong>
              <span>Add links or local media to build the queue.</span>
            </div>
          </div>
        ) : (
          <div className="source-groups" aria-label={`${state.sources.length} source groups`}>
            {shownSources.map((summary) => (
              <SourceRow
                active={active}
                key={summary.source.id}
                onRemove={onRemove}
                summary={summary}
              />
            ))}
            {hiddenSources > 0 && <div className="more-sources">+{hiddenSources} more source groups</div>}
          </div>
        )}
      </section>

      <section className="source-section preview-section">
        <div className="section-heading compact">
          <div>
            <span className="eyebrow">Queue</span>
            <h2>Preview and choose</h2>
          </div>
          <IconButton
            disabled={state.sources.length === 0 || state.previewLoading}
            icon="refresh"
            label="Refresh queue preview"
            onClick={onRefresh}
            size="sm"
          />
        </div>
        <div className="queue-metrics">
          <Metric value={found} label="found" />
          <Metric value={selectedCount} label="selected" />
          <Metric value={duplicates} label="duplicates" />
        </div>
        <div className="preview-message" role="status">
          {state.previewLoading ? (
            <><span className="spinner" /> Inspecting sources and media...</>
          ) : state.previewError ? (
            <><Icon name="alert" size={15} /> {state.previewError.user_message}</>
          ) : state.preview ? (
            <><Icon name="check" size={15} /> Queue ready. Start uses selected rows only.</>
          ) : (
            <><Icon name="info" size={15} /> Preview updates automatically after you add a source.</>
          )}
        </div>
      </section>

      <section className="source-section run-section">
        <div className="section-heading compact">
          <div>
            <span className="eyebrow">Action</span>
            <h2>Choose what to do</h2>
          </div>
        </div>
        <SegmentedControl
          label="Run mode"
          onChange={onMode}
          options={[
            { value: "transcribe", label: "Transcribe", hint: "Downloads missing link media, then transcribes selected items." },
            { value: "download", label: "Download", hint: "Downloads selected links without sending audio to Gemini." },
          ]}
          value={state.mode}
        />
        <div className="run-estimate">
          <span>{selectedCount} selected</span>
          <span>{state.mode === "transcribe" ? "Gemini used" : "No Gemini requests"}</span>
          <span>{state.settings?.output_formats.map(formatLabel).join(" + ") ?? "TXT + Markdown"}</span>
        </div>
        <Button
          className="review-button"
          disabled={Boolean(disabledReason)}
          icon={state.mode === "transcribe" ? "play" : "download"}
          onClick={onReview}
          title={disabledReason}
          variant="primary"
        >
          {state.planLoading ? "Building plan..." : `Review ${state.mode}`}
        </Button>
        {active && <StatusPill tone="info">A run is active. Progress is shown below.</StatusPill>}
      </section>
    </aside>
  );
}

function SourceRow({ summary, active, onRemove }: { summary: SourceFileSummary; active: boolean; onRemove: (id: string) => void }) {
  const { source, link_count: count } = summary;
  const icon = source.kind === "local_media" || source.kind === "directory" ? "file-audio" : "link";
  return (
    <div className="source-row">
      <Icon name={icon} size={16} />
      <div>
        <strong>{source.label || sourceLabels[source.kind]}</strong>
        <span>{count > 0 ? `${count} link${count === 1 ? "" : "s"}` : sourceLabels[source.kind]}{source.automatic ? " - automatic" : ""}</span>
      </div>
      <IconButton disabled={active} icon="x" label={`Remove ${source.label}`} onClick={() => onRemove(source.id)} size="sm" />
    </div>
  );
}

function Metric({ value, label }: { value: number; label: string }) {
  return <div><strong>{value}</strong><span>{label}</span></div>;
}

function formatLabel(value: string): string {
  return value === "text" ? "TXT" : value.toUpperCase();
}
