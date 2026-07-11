import { formatBytes, formatDuration, selectedTranscriptArtifact } from "../../lib/backend";
import type {
  AppSettings,
  ArtifactKind,
  EnvironmentSnapshot,
  ItemSnapshot,
  JobSnapshot,
  PreviewItem,
} from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, Drawer, Modal, ProgressBar, StatusPill } from "../ui";

export function AboutModal({
  open,
  environment,
  onClose,
  onGitHub,
}: {
  open: boolean;
  environment: EnvironmentSnapshot | null;
  onClose: () => void;
  onGitHub: () => void;
}) {
  return (
    <Modal
      footer={<><span className="modal-footer-note">MIT licensed - community maintained</span><Button onClick={onClose}>Close</Button><Button icon="external" onClick={onGitHub} variant="primary">Open GitHub</Button></>}
      onClose={onClose}
      open={open}
      size="md"
      title="About LectureScribe"
    >
      <div className="about-product">
        <div className="about-mark"><Icon name="file" size={27} /></div>
        <div><h3>LectureScribe {environment?.app_version ?? "0.2.0"}</h3><p>Batch downloading and Gemini transcription for general audio and video.</p></div>
      </div>
      <div className="about-principles">
        <div><Icon name="shield" size={18} /><span><strong>Local-first</strong><small>No account, telemetry, subscription, or cloud history.</small></span></div>
        <div><Icon name="key" size={18} /><span><strong>Bring your own Gemini key</strong><small>Stored only in Windows Credential Manager.</small></span></div>
        <div><Icon name="layers" size={18} /><span><strong>Recoverable batches</strong><small>Verified downloads, segments, and outputs are reused.</small></span></div>
      </div>
      <div className="version-table">
        <VersionRow label="Native engine" value="Rust 0.2 task ledger" />
        <VersionRow label="FFmpeg" value={environment?.ffmpeg.version ?? environment?.ffmpeg.detail ?? "Not detected"} />
        <VersionRow label="FFprobe" value={environment?.ffprobe.version ?? environment?.ffprobe.detail ?? "Not detected"} />
        <VersionRow label="Downloader" value={environment?.downloader.version ?? environment?.downloader.detail ?? "Not detected"} />
        <VersionRow label="Model" value="Configurable - gemini-3.1-flash-lite recommended" />
      </div>
      <p className="about-privacy">Audio segments are sent to Gemini only when transcription runs. Download-only mode does not use Gemini.</p>
    </Modal>
  );
}

export function SummaryModal({
  job,
  open,
  onClose,
  onOpenOutput,
  onRetry,
}: {
  job: JobSnapshot | null;
  open: boolean;
  onClose: () => void;
  onOpenOutput: () => void;
  onRetry: () => void;
}) {
  const summary = job?.summary;
  if (!job || !summary) return null;
  const success = summary.outcome === "complete" && summary.counts.failed === 0;
  return (
    <Modal
      description="Completed work is already saved. This summary remains available in History."
      footer={<><Button onClick={onClose}>Close</Button>{summary.counts.failed > 0 && <Button icon="refresh" onClick={onRetry}>Retry failed</Button>}<Button icon="folder" onClick={onOpenOutput} variant="primary">Open output</Button></>}
      onClose={onClose}
      open={open}
      title={success ? "Run complete" : summary.outcome === "cancelled" ? "Run cancelled safely" : "Run finished with issues"}
    >
      <div className={`summary-banner ${success ? "is-success" : "is-warning"}`}>
        <Icon name={success ? "check" : "alert"} size={24} />
        <div><h3>{success ? `${summary.saved_transcripts || summary.downloaded_media} outputs saved` : `${summary.counts.failed} items need attention`}</h3><p>{success ? "LectureScribe finished every planned item." : "Successful items were kept and can be reused when failed items are retried."}</p></div>
      </div>
      <div className="summary-grid">
        <SummaryValue label="Complete" value={summary.counts.complete} />
        <SummaryValue label="Reused" value={summary.counts.reused} />
        <SummaryValue label="Failed" value={summary.counts.failed} />
        <SummaryValue label="Cancelled" value={summary.counts.cancelled} />
        <SummaryValue label="Transcripts" value={summary.saved_transcripts} />
        <SummaryValue label="Downloads" value={summary.downloaded_media} />
        <SummaryValue label="Gemini requests" value={summary.gemini_requests} />
        <SummaryValue label="Elapsed" value={formatElapsed(summary.elapsed_seconds)} />
      </div>
      <div className="path-display"><Icon name="folder" size={16} /><span title={summary.output_dir}>{summary.output_dir}</span></div>
    </Modal>
  );
}

export function ItemDrawer({
  item,
  snapshot,
  settings,
  open,
  onClose,
  onOpenArtifact,
}: {
  item: PreviewItem | null;
  snapshot?: ItemSnapshot;
  settings: AppSettings | null;
  open: boolean;
  onClose: () => void;
  onOpenArtifact: (itemId: string, kind: ArtifactKind, reveal?: boolean) => void;
}) {
  if (!item) return null;
  const transcriptKind = settings ? selectedTranscriptArtifact(settings.output_formats) : "text_transcript";
  const transcriptReady = snapshot?.artifacts.some((artifact) => artifact.kind === transcriptKind);
  const downloaded = snapshot?.artifacts.find((artifact) => artifact.kind === "downloaded_media");
  return (
    <Drawer onClose={onClose} open={open} title={item.title}>
      {item.thumbnail_url && <img alt="Video thumbnail" className="detail-thumbnail" referrerPolicy="no-referrer" src={item.thumbnail_url} />}
      <div className="detail-status">
        <StatusPill tone={snapshot?.outcome === "failed" || item.error ? "danger" : snapshot?.outcome === "complete" || snapshot?.outcome === "reused" ? "success" : "info"}>{snapshot?.state ?? item.status}</StatusPill>
        <span>{providerLabel(item.provider)}{item.duration_seconds ? ` - ${formatDuration(item.duration_seconds)}` : ""}</span>
      </div>
      <section className="detail-section">
        <h3>Source</h3>
        <DetailValue label="Origin" value={item.source_group} />
        <DetailValue label="URL or path" value={item.url ?? item.media_path ?? item.source} copy />
        <DetailValue label="Expected media" value={item.expected_media_name ?? "Resolved when needed"} />
      </section>
      <section className="detail-section">
        <h3>Activity timeline</h3>
        {!snapshot ? <p className="detail-empty">No run activity yet. Review a plan to see exact actions.</p> : (
          <div className="task-timeline">
            {snapshot.tasks.map((task) => {
              const percent = task.progress?.total ? task.progress.current / task.progress.total * 100 : task.state === "succeeded" || task.state === "reused" ? 100 : 0;
              return <div className={`task-row task-${task.state}`} key={task.id}><span className="task-icon"><Icon name={task.state === "succeeded" || task.state === "reused" ? "check" : task.state === "failed" ? "alert" : "clock"} size={13} /></span><div><strong>{taskLabel(task.kind)}</strong><small>{task.message || task.state}{task.attempt ? ` - attempt ${task.attempt}/${task.max_attempts}` : ""}</small>{task.progress && <ProgressBar label={`${task.kind} progress`} value={percent} />}</div></div>;
            })}
          </div>
        )}
      </section>
      {snapshot?.error && <section className="detail-error"><Icon name="alert" size={17} /><div><strong>{snapshot.error.user_message}</strong><p>{snapshot.error.preserved_work || "Other completed items are unaffected."}</p></div></section>}
      <section className="detail-section">
        <h3>Outputs</h3>
        {snapshot?.artifacts.length ? snapshot.artifacts.map((artifact) => <div className="artifact-row" key={artifact.id}><Icon name={artifact.kind === "downloaded_media" ? "video" : "file"} size={15} /><span><strong>{artifactLabel(artifact.kind)}</strong><small>{formatBytes(artifact.size_bytes)}</small></span><button onClick={() => onOpenArtifact(item.id, artifact.kind, artifact.kind === "downloaded_media")} type="button">Open</button></div>) : <p className="detail-empty">Outputs appear here as they are verified and saved.</p>}
      </section>
      <div className="drawer-actions">
        <Button disabled={!transcriptReady} icon="file" onClick={() => onOpenArtifact(item.id, transcriptKind)} title={!transcriptReady ? "No saved transcript yet." : undefined}>Open transcript</Button>
        <Button disabled={!downloaded} icon="folder" onClick={() => onOpenArtifact(item.id, "downloaded_media", true)} title={!downloaded ? "No kept downloaded media yet." : undefined}>Reveal media</Button>
      </div>
    </Drawer>
  );
}

function VersionRow({ label, value }: { label: string; value: string }) { return <div><span>{label}</span><code>{value}</code></div>; }
function SummaryValue({ label, value }: { label: string; value: string | number }) { return <div><strong>{value}</strong><span>{label}</span></div>; }
function DetailValue({ label, value, copy = false }: { label: string; value: string; copy?: boolean }) { return <div className="detail-value"><span>{label}</span><strong title={value}>{value}</strong>{copy && <button onClick={() => navigator.clipboard.writeText(value)} title="Copy value" type="button"><Icon name="copy" size={14} /></button>}</div>; }
function formatElapsed(seconds: number): string { const minutes = Math.floor(seconds / 60); return minutes > 0 ? `${minutes}m ${seconds % 60}s` : `${seconds}s`; }
function providerLabel(provider: string): string { return provider === "you_tube" ? "YouTube" : provider === "google_drive" ? "Google Drive" : provider === "local" ? "Local media" : "Web link"; }
function taskLabel(kind: string): string { return ({ inspect: "Inspect source", download: "Download media", verify: "Verify media", prepare: "Prepare audio", segment: "Create segments", transcribe: "Gemini transcription", validate: "Validate transcript", merge: "Merge segments", save: "Save outputs", reuse: "Reuse output" } as Record<string, string>)[kind] ?? kind; }
function artifactLabel(kind: string): string { return kind.replaceAll("_", " ").replace(/^./, (value) => value.toUpperCase()); }
