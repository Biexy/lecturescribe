import { useEffect, useState } from "react";
import type { ModelOption, RunOverrides, RunPlan } from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, Modal, StatusPill } from "../ui";

export function PreflightModal({
  plan,
  open,
  starting,
  rebuilding,
  modelOptions,
  onClose,
  onRebuild,
  onStart,
  onFixSetup,
}: {
  plan: RunPlan | null;
  open: boolean;
  starting: boolean;
  rebuilding: boolean;
  modelOptions: ModelOption[];
  onClose: () => void;
  onRebuild: (overrides: RunOverrides) => void;
  onStart: () => void;
  onFixSetup: () => void;
}) {
  const [batchName, setBatchName] = useState("");
  const [modelId, setModelId] = useState("");
  useEffect(() => {
    if (!plan) return;
    setBatchName(plan.batch_name);
    setModelId(plan.settings.model);
  }, [plan?.id]);
  if (!plan) return null;
  const downloads = plan.items.filter((item) => ["download_and_transcribe", "download_only"].includes(item.action)).length;
  const reused = plan.items.filter((item) => ["reuse_media_and_transcribe", "reuse_transcript"].includes(item.action)).length;
  const transcribed = plan.items.filter((item) => ["download_and_transcribe", "reuse_media_and_transcribe", "transcribe_local"].includes(item.action)).length;
  const setupBlocked = plan.blocking_errors.length > 0;
  const cannotStart = setupBlocked || plan.runnable_count === 0;
  const runNote = plan.mode === "transcribe"
    ? `${plan.runnable_count} runnable - about ${plan.estimated_requests} Gemini request${plan.estimated_requests === 1 ? "" : "s"}`
    : `${plan.runnable_count} runnable - Gemini is not used`;
  const normalizedBatchName = batchName.trim();
  const overridesChanged = normalizedBatchName !== plan.batch_name || modelId !== plan.settings.model;
  const availableModels = modelOptions.some((model) => model.id === modelId)
    ? modelOptions
    : [...modelOptions, { id: modelId, display_name: modelId, description: "Custom model for this batch.", recommended: false, quality_label: "Custom" }];
  return (
    <Modal
      description="This plan is immutable. LectureScribe will run exactly the selected actions shown below."
      footer={
        <>
          <span className="modal-footer-note">{runNote}</span>
          <Button onClick={onClose}>Back</Button>
          {setupBlocked && <Button icon="wrench" onClick={onFixSetup}>Fix {plan.blocking_errors.length} setup requirement{plan.blocking_errors.length === 1 ? "" : "s"}</Button>}
          <Button disabled={cannotStart || starting} icon={plan.mode === "transcribe" ? "play" : "download"} onClick={onStart} title={cannotStart ? "Resolve the listed requirements before starting." : undefined} variant="primary">
            {starting ? "Starting..." : plan.mode === "transcribe" ? "Start transcription" : "Start download"}
          </Button>
        </>
      }
      onClose={onClose}
      open={open}
      size="lg"
      title={plan.mode === "transcribe" ? "Review transcription plan" : "Review download plan"}
    >
      <div className="preflight-metrics">
        <PlanMetric icon="list" label="Selected" value={plan.selected_count} />
        <PlanMetric icon="download" label="Will download" value={downloads} />
        <PlanMetric icon="file-audio" label="Will transcribe" value={transcribed} />
        <PlanMetric icon="undo" label="Will reuse" value={reused} />
      </div>

      <section className="preflight-options" aria-label="Batch options">
        <label><span>Batch folder name</span><input onInput={(event) => setBatchName(event.currentTarget.value)} value={batchName} /></label>
        <label><span>Model for this batch</span><select disabled={plan.mode !== "transcribe"} onChange={(event) => setModelId(event.currentTarget.value)} value={modelId}>{availableModels.map((model) => <option key={model.id} value={model.id}>{model.display_name}{model.recommended ? " - Recommended" : ""}</option>)}</select></label>
        <Button disabled={!overridesChanged || rebuilding || !normalizedBatchName} icon="refresh" onClick={() => onRebuild({ batch_name: normalizedBatchName, model_id: modelId })}>{rebuilding ? "Updating..." : "Apply"}</Button>
        <small title={plan.batch_output_dir}>Saved to {plan.batch_output_dir}</small>
      </section>

      {plan.blocking_errors.length > 0 && (
        <div className="preflight-problems" role="alert">
          <h3><Icon name="alert" size={17} /> {plan.blocking_errors.length} setup requirement{plan.blocking_errors.length === 1 ? "" : "s"}</h3>
          <div>{plan.blocking_errors.map((error) => <p key={error.code}><Icon name="wrench" size={14} /> {error.user_message}</p>)}</div>
        </div>
      )}

      {plan.blocked_count > 0 && !setupBlocked && (
        <div className="preflight-notice"><Icon name="info" size={16} /><span>{plan.blocked_count} item{plan.blocked_count === 1 ? " is" : "s are"} unavailable and will not stop the other runnable items.</span></div>
      )}

      <div className="preflight-list">
        {plan.items.map((item) => (
          <div className="preflight-row" key={item.item.id}>
            <span className="preflight-number">{String(item.ordinal).padStart(2, "0")}</span>
            <div><strong>{item.item.title}</strong><small>{item.reason}</small></div>
            <span>{actionLabel(item.action)}</span>
            <span>{item.estimated_segments} segment{item.estimated_segments === 1 ? "" : "s"}</span>
            <StatusPill tone={item.action === "blocked" ? "danger" : item.action === "reuse_transcript" ? "success" : "info"}>{item.action === "blocked" ? "Blocked" : "Planned"}</StatusPill>
          </div>
        ))}
      </div>
      <div className="privacy-note"><Icon name="shield" size={16} /><span>Your files stay local. Audio segments are sent to Gemini only for transcription.</span></div>
    </Modal>
  );
}

function PlanMetric({ icon, value, label }: { icon: "list" | "download" | "file-audio" | "undo"; value: number; label: string }) {
  return <div><Icon name={icon} size={17} /><span><strong>{value}</strong><small>{label}</small></span></div>;
}

function actionLabel(action: string): string {
  return ({ download_and_transcribe: "Download + transcribe", reuse_media_and_transcribe: "Reuse media + transcribe", transcribe_local: "Transcribe local media", reuse_transcript: "Reuse transcript", download_only: "Download media", excluded: "Excluded", blocked: "Blocked" } as Record<string, string>)[action] ?? action;
}
