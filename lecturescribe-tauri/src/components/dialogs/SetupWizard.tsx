import { useState } from "react";
import type { AppSettings, EnvironmentSnapshot, SetupTestResult, ToolStatus } from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, Field, Modal, StatusPill } from "../ui";

const steps = ["Privacy", "Gemini", "FFmpeg", "Downloader", "Output", "Test"];

export function SetupWizard({
  open,
  step,
  settings,
  environment,
  busy,
  testResult,
  onStep,
  onClose,
  onSaveKey,
  onDeleteKey,
  onChooseFfmpeg,
  onInstallDownloader,
  onChooseOutput,
  onCheck,
  onTest,
  onOpenLink,
}: {
  open: boolean;
  step: number;
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  busy: string | null;
  testResult: SetupTestResult | null;
  onStep: (step: number) => void;
  onClose: () => void;
  onSaveKey: (key: string) => void;
  onDeleteKey: () => void;
  onChooseFfmpeg: () => void;
  onInstallDownloader: () => void;
  onChooseOutput: () => void;
  onCheck: () => void;
  onTest: () => void;
  onOpenLink: (target: "ai_studio" | "ffmpeg" | "yt_dlp") => void;
}) {
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const lastStep = steps.length - 1;
  const canClose = environment?.setup_complete || step === 0;
  return (
    <Modal
      description="A resumable check of the few things LectureScribe needs. Local-media transcription does not require the Downloader."
      footer={
        <>
          <span className="modal-footer-note">Step {step + 1} of {steps.length}</span>
          {step > 0 && <Button disabled={Boolean(busy)} onClick={() => onStep(step - 1)}>Back</Button>}
          {canClose && <Button disabled={Boolean(busy)} onClick={onClose}>{environment?.setup_complete ? "Done" : "Close"}</Button>}
          {step < lastStep && <Button disabled={Boolean(busy)} onClick={() => onStep(step + 1)} variant="primary">Continue</Button>}
          {step === lastStep && <Button disabled={Boolean(busy)} icon="shield" onClick={onTest} variant="primary">{busy === "test" ? "Testing..." : "Test setup"}</Button>}
        </>
      }
      onClose={onClose}
      open={open}
      size="lg"
      title="Setup & diagnostics"
    >
      <ol className="setup-steps" aria-label="Setup progress">
        {steps.map((label, index) => (
          <li className={index === step ? "is-current" : index < step ? "is-complete" : ""} key={label}>
            <button aria-current={index === step ? "step" : undefined} onClick={() => onStep(index)} type="button">
              <span>{index < step ? <Icon name="check" size={13} /> : index + 1}</span>
              {label}
            </button>
          </li>
        ))}
      </ol>

      <div className="setup-content">
        {step === 0 && (
          <div className="setup-intro">
            <div className="setup-hero-icon"><Icon name="shield" size={24} /></div>
            <h3>Local-first by design</h3>
            <p>Sources, downloads, history, cache, and transcripts stay on this computer. When you transcribe, only prepared audio segments are sent to the Gemini API using your key.</p>
            <div className="privacy-grid">
              <div><Icon name="check" size={16} /><span><strong>No account</strong><small>No LectureScribe login or subscription.</small></span></div>
              <div><Icon name="key" size={16} /><span><strong>Your API key</strong><small>Stored in Windows Credential Manager.</small></span></div>
              <div><Icon name="folder" size={16} /><span><strong>Your output</strong><small>You choose where files are saved.</small></span></div>
              <div><Icon name="history" size={16} /><span><strong>Local history</strong><small>No telemetry or cloud job history.</small></span></div>
            </div>
          </div>
        )}

        {step === 1 && (
          <div className="setup-pane">
            <PaneHeading icon="key" title="Connect Gemini" status={environment?.api_key_configured ? "Ready" : "Required"} ready={Boolean(environment?.api_key_configured)} />
            <p>Get a key from Google AI Studio, paste it once, and LectureScribe stores it in Windows Credential Manager. It is never written to settings or diagnostics.</p>
            <button className="inline-link" onClick={() => onOpenLink("ai_studio")} type="button"><Icon name="external" size={14} /> Get an API key from Google AI Studio</button>
            <Field hint="gemini-3.1-flash-lite is recommended because it is easy to select in AI Studio and is commonly free-tier friendly. Limits can change." label="Gemini API key">
              <div className="secret-input">
                <input autoComplete="off" onInput={(event: Event) => setApiKey((event.currentTarget as HTMLInputElement).value)} placeholder={environment?.api_key_configured ? "Key saved securely" : "Paste API key"} type={showKey ? "text" : "password"} value={apiKey} />
                <button aria-label={showKey ? "Hide API key" : "Show API key"} onClick={() => setShowKey(!showKey)} title={showKey ? "Hide API key" : "Show API key"} type="button"><Icon name="eye" size={16} /></button>
              </div>
            </Field>
            <div className="setup-actions">
              <Button disabled={apiKey.trim().length < 20 || Boolean(busy)} icon="key" onClick={() => { onSaveKey(apiKey); setApiKey(""); }} variant="primary">{busy === "key" ? "Saving..." : "Save key securely"}</Button>
              {environment?.api_key_configured && <Button disabled={Boolean(busy)} onClick={onDeleteKey} variant="ghost">Remove saved key</Button>}
            </div>
          </div>
        )}

        {step === 2 && (
          <div className="setup-pane">
            <PaneHeading icon="wrench" title="FFmpeg & FFprobe" status={toolLabel(environment?.ffmpeg)} ready={Boolean(environment?.ffmpeg.path && environment?.ffprobe.path)} />
            <p>Required for local audio/video inspection, audio normalization, and silence-aware segments. LectureScribe automatically checks configured tools, its tools folder, the app folder, and PATH.</p>
            <ToolDetail label="FFmpeg" tool={environment?.ffmpeg} />
            <ToolDetail label="FFprobe" tool={environment?.ffprobe} />
            <div className="setup-actions">
              <Button disabled={Boolean(busy)} icon="folder" onClick={onChooseFfmpeg}>Choose FFmpeg</Button>
              <Button icon="external" onClick={() => onOpenLink("ffmpeg")} variant="ghost">Download page</Button>
              <Button disabled={Boolean(busy)} icon="refresh" onClick={onCheck} variant="ghost">Check again</Button>
            </div>
          </div>
        )}

        {step === 3 && (
          <div className="setup-pane">
            <PaneHeading icon="download" title="Downloader (yt-dlp)" status={toolLabel(environment?.downloader)} ready={Boolean(environment?.downloader.path)} />
            <p>Required only for YouTube, Google Drive, and other supported links. Local files can be transcribed without it. The app installs an official pinned build and verifies its checksum.</p>
            <ToolDetail label="Downloader" tool={environment?.downloader} />
            <div className="setup-actions">
              <Button disabled={Boolean(busy)} icon="download" onClick={onInstallDownloader} variant="primary">{busy === "downloader" ? "Installing..." : environment?.downloader.path ? "Update Downloader" : "Install Downloader"}</Button>
              <Button icon="external" onClick={() => onOpenLink("yt_dlp")} variant="ghost">Project page</Button>
              <Button disabled={Boolean(busy)} icon="refresh" onClick={onCheck} variant="ghost">Check again</Button>
            </div>
          </div>
        )}

        {step === 4 && (
          <div className="setup-pane">
            <PaneHeading icon="folder" title="Output folder" status={environment?.output_writable ? "Ready" : "Required"} ready={Boolean(environment?.output_writable)} />
            <p>Transcripts, optional downloaded media, the batch index, and manifest are saved here. Work cache stays in the local app-data folder.</p>
            <div className="path-display"><Icon name="folder" size={16} /><span title={settings?.output_dir}>{settings?.output_dir || "No output folder selected"}</span></div>
            <div className="setup-actions">
              <Button disabled={Boolean(busy)} icon="folder" onClick={onChooseOutput} variant="primary">Choose output folder</Button>
              <Button disabled={Boolean(busy)} icon="refresh" onClick={onCheck} variant="ghost">Check again</Button>
            </div>
          </div>
        )}

        {step === 5 && (
          <div className="setup-pane">
            <PaneHeading icon="shield" title="Test the pipeline" status={testResult?.ok ? "Passed" : "Optional"} ready={Boolean(testResult?.ok)} />
            <p>This creates one second of silent audio with FFmpeg, verifies it with FFprobe, and sends one small request to your selected Gemini model.</p>
            <div className="doctor-list">
              <DoctorRow label="Gemini key" ready={Boolean(environment?.api_key_configured)} detail={environment?.api_key_verified ? "Verified by a setup test" : environment?.api_key_configured ? "Saved, not tested yet" : "Missing"} />
              <DoctorRow label="FFmpeg suite" ready={Boolean(environment?.ffmpeg.path && environment?.ffprobe.path)} detail={environment?.ffmpeg.detail ?? "Not checked"} />
              <DoctorRow label="Downloader" ready={Boolean(environment?.downloader.path)} detail={environment?.downloader.detail ?? "Not checked - optional for local media"} />
              <DoctorRow label="Output folder" ready={Boolean(environment?.output_writable)} detail={environment?.output_writable ? "Writable" : "Needs attention"} />
              <DoctorRow label="Run database" ready={Boolean(environment?.database_ok)} detail={environment?.database_ok ? "Integrity check passed" : "Integrity check failed"} />
            </div>
            {testResult && <div className="setup-test-result"><Icon name="check" size={17} /><div><strong>{testResult.message}</strong><small>{testResult.transcript_preview}</small></div></div>}
          </div>
        )}
      </div>
    </Modal>
  );
}

function PaneHeading({ icon, title, status, ready }: { icon: "key" | "wrench" | "download" | "folder" | "shield"; title: string; status: string; ready: boolean }) {
  return <div className="pane-heading"><span><Icon name={icon} size={19} /><h3>{title}</h3></span><StatusPill tone={ready ? "success" : "warning"}>{status}</StatusPill></div>;
}

function ToolDetail({ label, tool }: { label: string; tool?: ToolStatus }) {
  return <div className="tool-detail"><Icon name={tool?.path ? "check" : "alert"} size={15} /><span><strong>{label}</strong><small>{tool?.path ?? tool?.detail ?? "Not found"}</small></span>{tool?.version && <code>{tool.version}</code>}</div>;
}

function DoctorRow({ label, ready, detail }: { label: string; ready: boolean; detail: string }) {
  return <div><Icon name={ready ? "check" : "alert"} size={15} /><span><strong>{label}</strong><small>{detail}</small></span><StatusPill tone={ready ? "success" : "warning"}>{ready ? "Ready" : "Fix"}</StatusPill></div>;
}

function toolLabel(tool?: ToolStatus): string {
  if (!tool) return "Checking";
  return tool.readiness === "ready" ? "Ready" : tool.readiness === "outdated" ? "Update available" : "Required";
}
