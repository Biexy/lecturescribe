import { useEffect, useState, type ReactNode } from "react";
import { capabilityLabel } from "../../lib/setup";
import type {
  AppError,
  AppSettings,
  EnvironmentSnapshot,
  SetupCapability,
  SetupTestResult,
  ToolStatus,
} from "../../types/contracts";
import { Icon, type IconName } from "../Icon";
import { Button, Field, Modal, StatusPill } from "../ui";

const capabilityOptions: Array<{
  id: SetupCapability;
  icon: IconName;
  description: string;
}> = [
  { id: "download_links", icon: "download", description: "Save media from supported links. Gemini is not used." },
  { id: "transcribe_local", icon: "file-audio", description: "Transcribe files already on this computer." },
  { id: "transcribe_links", icon: "link", description: "Download link media, then create transcripts." },
];

export function SetupCenter({
  open,
  focus,
  settings,
  environment,
  busy,
  testResult,
  testError,
  onClose,
  onSaveKey,
  onDeleteKey,
  onChooseFfmpeg,
  onInstallDownloader,
  onChooseOutput,
  onCheck,
  onTest,
  onExportDiagnostics,
  onOpenLink,
}: {
  open: boolean;
  focus: SetupCapability | null;
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  busy: string | null;
  testResult: SetupTestResult | null;
  testError: AppError | null;
  onClose: () => void;
  onSaveKey: (key: string) => void;
  onDeleteKey: () => void;
  onChooseFfmpeg: () => void;
  onInstallDownloader: () => void;
  onChooseOutput: () => void;
  onCheck: () => void;
  onTest: () => void;
  onExportDiagnostics: () => void;
  onOpenLink: (target: "ai_studio" | "ffmpeg" | "yt_dlp") => void;
}) {
  const [capability, setCapability] = useState<SetupCapability>(focus ?? "transcribe_local");
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);

  useEffect(() => {
    if (open && focus) setCapability(focus);
  }, [open, focus]);

  const status = environment?.capabilities[capability];
  const transcription = capability !== "download_links";
  const links = capability !== "transcribe_local";
  const ffmpegReady = environment?.ffmpeg.readiness === "ready" && environment?.ffprobe.readiness === "ready";
  const downloaderReady = environment?.downloader.readiness === "ready";
  const outputDescription = capability === "download_links"
    ? "Downloaded media and batch files are saved here."
    : capability === "transcribe_local"
      ? "Transcripts and batch files are saved here."
      : "Transcripts, batch files, and any media you choose to keep are saved here.";
  const canTest = Boolean(
    environment?.api_key_configured
    && ffmpegReady
    && environment.output_writable
    && environment.database_ok,
  );

  return (
    <Modal
      description="Choose what you want to do. LectureScribe checks only the tools that action needs."
      footer={
        <>
          <span className="modal-footer-note">Setup never blocks unrelated workflows.</span>
          <Button disabled={Boolean(busy)} icon="refresh" onClick={onCheck}>Check again</Button>
          {transcription && (
            <Button
              disabled={!canTest || Boolean(busy)}
              icon="shield"
              onClick={onTest}
              title={!canTest ? "Add a Gemini key, configure FFmpeg, and choose a writable output first." : "Uses one small Gemini request."}
            >
              {busy === "test" ? "Testing..." : "Test transcription"}
            </Button>
          )}
          <Button onClick={onClose} variant="primary">Done</Button>
        </>
      }
      onClose={onClose}
      open={open}
      size="lg"
      title="Setup center"
    >
      <div className="setup-center">
        <div className="capability-picker" aria-label="Workflow to check" role="radiogroup">
          {capabilityOptions.map((option, optionIndex) => {
            const optionStatus = environment?.capabilities[option.id];
            const selected = option.id === capability;
            return (
              <button
                aria-checked={selected}
                className={selected ? "is-selected" : ""}
                key={option.id}
                onKeyDown={(event) => {
                  const keys = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"];
                  if (!keys.includes(event.key)) return;
                  event.preventDefault();
                  const last = capabilityOptions.length - 1;
                  const nextIndex = event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? last
                      : (optionIndex + (event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1) + capabilityOptions.length) % capabilityOptions.length;
                  setCapability(capabilityOptions[nextIndex].id);
                  const buttons = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]');
                  buttons?.[nextIndex]?.focus();
                }}
                onClick={() => setCapability(option.id)}
                role="radio"
                tabIndex={selected ? 0 : -1}
                type="button"
              >
                <Icon name={option.icon} size={18} />
                <span><strong>{capabilityLabel(option.id)}</strong><small>{option.description}</small></span>
                <StatusPill tone={!optionStatus ? "neutral" : optionStatus.ready ? "success" : "warning"}>
                  {!optionStatus ? "Checking" : optionStatus.ready ? "Ready" : `${optionStatus.blockers.length} to fix`}
                </StatusPill>
              </button>
            );
          })}
        </div>

        <section aria-live="polite" className="setup-requirements" aria-label={`${capabilityLabel(capability)} requirements`}>
          <header>
            <div>
              <span className="eyebrow">Requirements</span>
              <h3>{status?.ready ? `${capabilityLabel(capability)} is ready` : `Prepare ${capabilityLabel(capability).toLowerCase()}`}</h3>
            </div>
            <StatusPill tone={!status ? "neutral" : status.ready ? "success" : "warning"}>
              {!status ? "Checking" : status.ready ? "Ready" : `${status.blockers.length} required`}
            </StatusPill>
          </header>

          <div className="requirement-list">
            <Requirement
              description={outputDescription}
              icon="folder"
              ready={Boolean(environment?.output_writable)}
              status={environment?.output_writable ? "Writable" : "Choose folder"}
              title="Output folder"
            >
              <div className="path-display compact"><Icon name="folder" size={15} /><span title={settings?.output_dir}>{settings?.output_dir || "No output folder selected"}</span></div>
              <Button disabled={Boolean(busy)} icon="folder" onClick={onChooseOutput} size="sm">Choose folder</Button>
            </Requirement>

            {transcription && (
            <Requirement
              description="Used only when transcription runs. Stored in Windows Credential Manager."
              icon="key"
              ready={Boolean(environment?.api_key_verified)}
              status={environment?.api_key_verified ? "Verified" : environment?.api_key_configured ? "Saved - test required" : "Required"}
              title="Gemini API key"
              >
                <button className="inline-link" onClick={() => onOpenLink("ai_studio")} type="button"><Icon name="external" size={14} /> Get a key from Google AI Studio</button>
                <Field hint="Paste a new key to add or replace the saved credential." label="API key">
                  <div className="secret-input">
                    <input
                      autoComplete="off"
                      onInput={(event) => setApiKey(event.currentTarget.value)}
                      placeholder={environment?.api_key_configured ? "Key already saved" : "Paste API key"}
                      type={showKey ? "text" : "password"}
                      value={apiKey}
                    />
                    <button aria-label={showKey ? "Hide API key" : "Show API key"} onClick={() => setShowKey(!showKey)} title={showKey ? "Hide API key" : "Show API key"} type="button"><Icon name="eye" size={16} /></button>
                  </div>
                </Field>
                <div className="setup-actions">
                  <Button disabled={apiKey.trim().length < 20 || Boolean(busy)} icon="key" onClick={() => { onSaveKey(apiKey); setApiKey(""); }} size="sm" variant="primary">{busy === "key" ? "Saving..." : "Save key"}</Button>
                  {environment?.api_key_configured && <Button disabled={Boolean(busy)} onClick={onDeleteKey} size="sm" variant="ghost">Remove key</Button>}
                </div>
              </Requirement>
            )}

            {transcription && (
              <Requirement
                description="Inspects media, prepares audio, and creates reliable segments."
                icon="wrench"
                ready={ffmpegReady}
                status={ffmpegReady ? "Detected" : "Required"}
                title="FFmpeg & FFprobe"
              >
                <ToolSummary label="FFmpeg" tool={environment?.ffmpeg} />
                <ToolSummary label="FFprobe" tool={environment?.ffprobe} />
                <div className="setup-actions">
                  <Button disabled={Boolean(busy)} icon="folder" onClick={onChooseFfmpeg} size="sm">Choose tools</Button>
                  <Button icon="external" onClick={() => onOpenLink("ffmpeg")} size="sm" variant="ghost">Download page</Button>
                </div>
              </Requirement>
            )}

            {links && (
              <Requirement
                description="Downloads supported YouTube, Google Drive, and other link media."
                icon="download"
                ready={downloaderReady}
                status={downloaderReady ? "Ready" : "Required"}
                title="Downloader (yt-dlp)"
              >
                <ToolSummary label="Downloader" tool={environment?.downloader} />
                <div className="setup-actions">
                  <Button disabled={Boolean(busy)} icon="download" onClick={onInstallDownloader} size="sm" variant="primary">{busy === "downloader" ? "Installing..." : downloaderReady ? "Update downloader" : "Install downloader"}</Button>
                  <Button icon="external" onClick={() => onOpenLink("yt_dlp")} size="sm" variant="ghost">Project page</Button>
                </div>
              </Requirement>
            )}
          </div>
        </section>

        {environment && !environment.database_ok && (
          <div className="setup-critical" role="alert">
            <Icon name="alert" size={17} />
            <div><strong>Run database needs attention</strong><p>Job history and recovery cannot be trusted until diagnostics pass.</p></div>
            <Button icon="bug" onClick={onExportDiagnostics} size="sm">Export diagnostics</Button>
          </div>
        )}

        {testResult && (
          <div className={`setup-test-result ${testResult.ok ? "is-success" : "is-warning"}`}>
            <Icon name={testResult.ok ? "check" : "alert"} size={17} />
            <div><strong>{testResult.message}</strong><small>{testResult.transcript_preview}</small></div>
          </div>
        )}

        {testError && (
          <div className="setup-test-result is-warning" role="alert">
            <Icon name="alert" size={17} />
            <div><strong>{testError.user_message}</strong><small>{testError.preserved_work || "Your saved files were not changed."}</small></div>
          </div>
        )}

        <div className="privacy-note"><Icon name="shield" size={16} /><span>Files and history stay local. Prepared audio is sent to Gemini only when transcription runs.</span></div>
      </div>
    </Modal>
  );
}

function Requirement({
  icon,
  title,
  description,
  ready,
  status,
  children,
}: {
  icon: IconName;
  title: string;
  description: string;
  ready: boolean;
  status: string;
  children: ReactNode;
}) {
  return (
    <article className={`requirement ${ready ? "is-ready" : "needs-attention"}`}>
      <header>
        <span className="requirement-icon"><Icon name={icon} size={17} /></span>
        <div><strong>{title}</strong><small>{description}</small></div>
        <StatusPill tone={ready ? "success" : "warning"}>{status}</StatusPill>
      </header>
      <div className="requirement-body">{children}</div>
    </article>
  );
}

function ToolSummary({ label, tool }: { label: string; tool?: ToolStatus }) {
  const ready = tool?.readiness === "ready";
  return (
    <div className="tool-summary">
      <Icon name={ready ? "check" : "alert"} size={15} />
      <span><strong>{label}</strong><small title={tool?.path ?? tool?.detail}>{tool?.version ?? tool?.detail ?? "Not detected"}</small></span>
    </div>
  );
}
