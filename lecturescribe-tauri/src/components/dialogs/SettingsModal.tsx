import { useEffect, useState } from "react";
import type { AppSettings, EnvironmentSnapshot, TranscriptFormat } from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, Field, Modal, SegmentedControl, StatusPill, Toggle } from "../ui";

export function SettingsModal({
  open,
  settings,
  environment,
  saving,
  onClose,
  onSave,
  onChooseOutput,
  onOpenDiagnostics,
  onExportDiagnostics,
}: {
  open: boolean;
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  saving: boolean;
  onClose: () => void;
  onSave: (settings: AppSettings) => void;
  onChooseOutput: () => void;
  onOpenDiagnostics: () => void;
  onExportDiagnostics: () => void;
}) {
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  useEffect(() => {
    if (open && settings) setDraft(settings);
  }, [open, settings]);
  if (!draft) return null;
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft({ ...draft, [key]: value });
  const toggleFormat = (format: TranscriptFormat, enabled: boolean) => {
    const formats = enabled
      ? [...new Set([...draft.output_formats, format])]
      : draft.output_formats.filter((value) => value !== format);
    if (formats.length > 0) update("output_formats", formats);
  };
  return (
    <Modal
      description="Common choices are shown first. Tool paths, cookies, cache, and segmentation stay under Advanced."
      footer={
        <>
          <span className="modal-footer-note">Version {environment?.app_version ?? "0.2.0"}</span>
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={saving} icon="check" onClick={() => onSave(draft)} variant="primary">{saving ? "Saving..." : "Save settings"}</Button>
        </>
      }
      onClose={onClose}
      open={open}
      size="lg"
      title="Settings"
    >
      <div className="settings-layout">
        <section className="settings-section">
          <h3>Appearance &amp; output</h3>
          <div className="settings-grid two">
            <Field label="Theme">
              <SegmentedControl
                label="Theme"
                onChange={(theme) => update("theme", theme)}
                options={[{ value: "light", label: "Light" }, { value: "dark", label: "Dark" }]}
                value={draft.theme}
              />
            </Field>
            <Field hint="Transcripts and batch files are saved here." label="Output folder">
              <div className="path-input"><input readOnly title={draft.output_dir} value={draft.output_dir} /><Button icon="folder" onClick={onChooseOutput} size="sm">Choose</Button></div>
            </Field>
          </div>
          <fieldset className="format-options">
            <legend>Transcript formats</legend>
            {(["text", "markdown", "srt", "vtt"] as TranscriptFormat[]).map((format) => (
              <label key={format}><input checked={draft.output_formats.includes(format)} onChange={(event: Event) => toggleFormat(format, (event.currentTarget as HTMLInputElement).checked)} type="checkbox" /><span><strong>{format === "text" ? "TXT" : format.toUpperCase()}</strong><small>{formatDescription(format)}</small></span></label>
            ))}
          </fieldset>
        </section>

        <section className="settings-section">
          <h3>Transcription</h3>
          <div className="settings-grid two">
            <Field hint="You can enter another compatible Gemini model name." label="Gemini model">
              <div className="recommended-input"><input list="model-options" onInput={(event: Event) => update("model", (event.currentTarget as HTMLInputElement).value)} value={draft.model} /><StatusPill tone="info">Recommended</StatusPill></div>
              <datalist id="model-options"><option value="gemini-3.1-flash-lite" /></datalist>
            </Field>
            <Field label="Language">
              <select onChange={(event: Event) => update("language", (event.currentTarget as HTMLSelectElement).value)} value={draft.language}>
                <option value="auto">Auto detect</option><option value="en">English</option><option value="ar">Arabic</option>
              </select>
            </Field>
            <Field label="Prompt preset">
              <select onChange={(event: Event) => update("prompt_preset", (event.currentTarget as HTMLSelectElement).value)} value={draft.prompt_preset}>
                <option value="default">Default audio/video</option><option value="arabic_lecture">Arabic speech</option><option value="english_lecture">English speech</option><option value="technical">Technical / math</option>
              </select>
            </Field>
            <Field hint="Optional instructions appended to the selected preset." label="Additional prompt">
              <input onInput={(event: Event) => update("additional_prompt", (event.currentTarget as HTMLInputElement).value)} value={draft.additional_prompt} />
            </Field>
          </div>
          <p className="model-note"><Icon name="info" size={14} /> `gemini-3.1-flash-lite` is the recommended default because it is easy to get in AI Studio and commonly free-tier friendly. Google can change model availability and quotas.</p>
        </section>

        <details className="advanced-settings">
          <summary><span><Icon name="settings" size={16} /> Advanced</span><Icon name="chevron-down" size={16} /></summary>
          <div className="advanced-content">
            <div className="settings-grid three">
              <Field label="Segment minutes"><input max="30" min="5" onInput={(event: Event) => update("segment_minutes", Number((event.currentTarget as HTMLInputElement).value))} type="number" value={draft.segment_minutes} /></Field>
              <Field label="Overlap seconds"><input max="10" min="0" onInput={(event: Event) => update("overlap_seconds", Number((event.currentTarget as HTMLInputElement).value))} type="number" value={draft.overlap_seconds} /></Field>
              <Field label="Request delay (ms)"><input max="120000" min="0" onInput={(event: Event) => update("request_delay_ms", Number((event.currentTarget as HTMLInputElement).value))} type="number" value={draft.request_delay_ms} /></Field>
            </div>
            <div className="settings-grid two">
              <Field label="FFmpeg path"><input onInput={(event: Event) => update("ffmpeg_path", (event.currentTarget as HTMLInputElement).value)} value={draft.ffmpeg_path} /></Field>
              <Field label="FFprobe path"><input onInput={(event: Event) => update("ffprobe_path", (event.currentTarget as HTMLInputElement).value)} value={draft.ffprobe_path} /></Field>
              <Field label="Downloader path"><input onInput={(event: Event) => update("downloader_path", (event.currentTarget as HTMLInputElement).value)} value={draft.downloader_path} /></Field>
              <Field label="Cookies file"><input onInput={(event: Event) => update("cookies_file", (event.currentTarget as HTMLInputElement).value)} value={draft.cookies_file} /></Field>
              <Field hint="Example: chrome, edge, firefox" label="Cookies from browser"><input onInput={(event: Event) => update("cookies_from_browser", (event.currentTarget as HTMLInputElement).value)} value={draft.cookies_from_browser} /></Field>
              <Field label="Cache limit (GiB)"><input max="200" min="1" onInput={(event: Event) => update("cache_limit_gib", Number((event.currentTarget as HTMLInputElement).value))} type="number" value={draft.cache_limit_gib} /></Field>
            </div>
            <Toggle checked={draft.keep_downloaded_media} description="Link media is otherwise cached only for recovery." label="Keep downloaded media in output" onChange={(value) => update("keep_downloaded_media", value)} />
            <Toggle checked={draft.force} description="Ignore successful transcript cache and make new Gemini requests." label="Force retranscription" onChange={(value) => update("force", value)} />
          </div>
        </details>

        <section className="settings-section diagnostics-section">
          <div><h3>Diagnostics &amp; privacy</h3><p>Review setup health or export a sanitized report. API keys, cookies, URLs, filenames, content, and personal paths are removed.</p></div>
          <div><Button icon="wrench" onClick={onOpenDiagnostics}>Open Doctor</Button><Button icon="bug" onClick={onExportDiagnostics} variant="ghost">Export bug report</Button></div>
        </section>
      </div>
    </Modal>
  );
}

function formatDescription(format: TranscriptFormat): string {
  return ({ text: "Plain text", markdown: "Readable timestamps", srt: "Subtitle file", vtt: "Web captions" } as Record<TranscriptFormat, string>)[format];
}
