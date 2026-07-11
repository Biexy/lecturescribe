import type { RunPlan } from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, Modal, StatusPill } from "../ui";

export function PreflightModal({
  plan,
  open,
  starting,
  onClose,
  onStart,
  onFixSetup,
}: {
  plan: RunPlan | null;
  open: boolean;
  starting: boolean;
  onClose: () => void;
  onStart: () => void;
  onFixSetup: () => void;
}) {
  if (!plan) return null;
  const downloads = plan.items.filter((item) => ["download_and_transcribe", "download_only"].includes(item.action)).length;
  const reused = plan.items.filter((item) => ["reuse_media_and_transcribe", "reuse_transcript"].includes(item.action)).length;
  const transcribed = plan.items.filter((item) => ["download_and_transcribe", "reuse_media_and_transcribe", "transcribe_local"].includes(item.action)).length;
  const blocked = plan.blocking_errors.length > 0 || plan.blocked_count > 0;
  return (
    <Modal
      description="This plan is immutable. LectureScribe will run exactly the selected actions shown below."
      footer={
        <>
          <span className="modal-footer-note">{plan.runnable_count} runnable - {plan.estimated_requests} estimated Gemini request{plan.estimated_requests === 1 ? "" : "s"}</span>
          <Button onClick={onClose}>Back</Button>
          {blocked && <Button icon="wrench" onClick={onFixSetup}>Fix setup</Button>}
          <Button disabled={blocked || starting} icon={plan.mode === "transcribe" ? "play" : "download"} onClick={onStart} variant="primary">
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

      {plan.blocking_errors.length > 0 && (
        <div className="preflight-problems" role="alert">
          <h3><Icon name="alert" size={17} /> Setup required</h3>
          {plan.blocking_errors.map((error) => <p key={error.code}>{error.user_message}</p>)}
        </div>
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
