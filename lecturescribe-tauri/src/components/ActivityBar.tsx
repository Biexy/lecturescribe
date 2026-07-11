import { formatBytes, progressPercent } from "../lib/backend";
import { isJobActive, type UiState } from "../state/app-state";
import type { ItemSnapshot, TaskSnapshot } from "../types/contracts";
import { Icon } from "./Icon";
import { Button, IconButton, ProgressBar, StatusPill } from "./ui";

export function ActivityBar({
  state,
  onExpand,
  onPause,
  onResume,
  onCancel,
  onRetry,
  onOpenOutput,
}: {
  state: UiState;
  onExpand: (expanded: boolean) => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onOpenOutput: () => void;
}) {
  const { job } = state;
  const active = isJobActive(job);
  const currentItem = job?.items.find((item) => item.item.item.id === job.current_item_id) ??
    job?.items.find((item) => item.tasks.some((task) => ["running", "waiting"].includes(task.state)));
  const currentTask = currentItem?.tasks.find((task) => task.id === job?.current_task_id) ??
    currentItem?.tasks.find((task) => ["running", "waiting"].includes(task.state));
  const percent = progressPercent(job);
  const completed = job ? job.counts.complete + job.counts.reused + job.counts.skipped : 0;
  const status = activityStatus(state, currentItem, currentTask);
  const measurement = taskMeasurement(currentTask);

  return (
    <section className={`activity-bar ${state.activityExpanded ? "is-expanded" : ""}`} aria-label="Run activity">
      <div className="activity-main">
        <button className="activity-summary" onClick={() => onExpand(!state.activityExpanded)} type="button">
          <span className={`activity-dot ${active ? "is-active" : job?.state === "failed" ? "is-error" : ""}`} />
          <span className="activity-title">Activity</span>
          <StatusPill tone={active ? "info" : job?.state === "failed" ? "danger" : job?.state === "complete" ? "success" : "neutral"}>
            {job ? job.state.replaceAll("_", " ") : "Idle"}
          </StatusPill>
          <span className="activity-message" title={status}>{status}</span>
          <Icon name={state.activityExpanded ? "chevron-down" : "chevron-right"} size={16} />
        </button>

        <div className="activity-count">
          <Icon name="list" size={16} />
          <span><strong>{completed}</strong> / {job?.counts.planned ?? state.preview?.items.length ?? 0} items</span>
        </div>
        <div className="activity-measurement">
          {measurement.icon && <Icon name={measurement.icon} size={16} />}
          <span>{measurement.label}</span>
        </div>
        <div className="activity-progress">
          <ProgressBar label="Overall run progress" value={percent} />
          <span>{Math.round(percent)}%</span>
        </div>
        <div className="activity-actions">
          {job?.state === "paused" || job?.state === "interrupted" ? (
            <IconButton icon="play" label="Resume run" onClick={onResume} />
          ) : active ? (
            <IconButton icon="pause" label="Pause run" onClick={onPause} />
          ) : null}
          {active && <IconButton danger icon="square" label="Cancel run safely" onClick={onCancel} />}
          {!active && (job?.counts.failed ?? 0) > 0 && <Button icon="refresh" onClick={onRetry} size="sm" title="Retries failed items and reuses verified downloads and segments.">Retry failed</Button>}
        </div>
      </div>

      {state.activityExpanded && (
        <div className="activity-details">
          <div className="activity-detail-summary">
            <div><span>Current item</span><strong>{currentItem?.item.item.title ?? "No active item"}</strong></div>
            <div><span>Operation</span><strong>{currentTask ? taskLabel(currentTask) : "Waiting"}</strong></div>
            <div><span>Attempts</span><strong>{currentTask ? `${currentTask.attempt} / ${currentTask.max_attempts}` : "--"}</strong></div>
            <div><span>Preserved work</span><strong>{currentTask?.error?.preserved_work || "Verified outputs stay cached"}</strong></div>
          </div>
          <div className="recent-items" aria-label="Recent item activity">
            {(job?.items ?? []).filter((item) => item.state !== "queued").slice(-4).map((item) => (
              <div key={item.item.item.id}>
                <Icon name={item.outcome === "complete" || item.outcome === "reused" ? "check" : item.outcome === "failed" ? "alert" : "clock"} size={14} />
                <span>{item.item.item.title}</span>
                <small>{item.message || item.state}</small>
              </div>
            ))}
          </div>
          <div className="activity-detail-actions">
            <button onClick={onOpenOutput} type="button"><Icon name="folder" size={15} /> Open output folder</button>
            <span>Pause and cancel stop at the next safe point. Completed work remains reusable.</span>
          </div>
        </div>
      )}
    </section>
  );
}

function activityStatus(state: UiState, item?: ItemSnapshot, task?: TaskSnapshot): string {
  if (task?.state === "waiting" && task.error) return task.error.user_message;
  if (task?.message) return `${taskLabel(task)} - ${task.message}`;
  if (item) return `${item.state.replaceAll("_", " ")} - ${item.item.item.title}`;
  if (state.previewLoading) return "Inspecting added sources...";
  if (state.preview) return `${Object.values(state.selected).filter(Boolean).length} selected and ready to review.`;
  return "Add a source to begin.";
}

function taskMeasurement(task?: TaskSnapshot): { icon?: "download" | "layers" | "clock"; label: string } {
  if (!task?.progress) return { label: "No active measurement" };
  const progress = task.progress;
  if (task.kind === "download") {
    const rate = progress.rate ? `${formatBytes(progress.rate)}/s` : "Calculating speed";
    const eta = progress.eta_seconds != null ? ` - ${progress.eta_seconds}s remaining` : "";
    return { icon: "download", label: `${formatBytes(progress.current)} - ${rate}${eta}` };
  }
  if (progress.kind === "segments" && progress.total != null) {
    return { icon: "layers", label: `${Math.round(progress.current)} / ${Math.round(progress.total)} segments` };
  }
  if (task.kind === "transcribe") return { icon: "layers", label: task.message || "Transcribing with Gemini" };
  if (progress.eta_seconds) {
    return { icon: "clock", label: `${progress.eta_seconds}s remaining` };
  }
  return { label: taskLabel(task) };
}

function taskLabel(task: TaskSnapshot): string {
  const labels: Record<string, string> = {
    inspect: "Inspecting source",
    download: "Downloading media",
    verify: "Verifying media",
    prepare: "Preparing audio",
    segment: "Creating segments",
    transcribe: "Transcribing with Gemini",
    validate: "Validating response",
    merge: "Merging transcript",
    save: "Saving outputs",
    reuse: "Reusing verified output",
  };
  return labels[task.kind] ?? task.kind;
}
