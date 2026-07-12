import { useEffect, useState, type ReactNode } from "react";
import type { AppSettings, EnvironmentSnapshot, LanguagePreferences, ModelOption, ModelValidation } from "../../types/contracts";
import { Icon, type IconName } from "../Icon";
import { Button, Field, Modal, SegmentedControl, Toggle } from "../ui";
import { LanguagePickerDialog } from "../settings/LanguagePickerDialog";
import { normalizeLanguagePreferences } from "../settings/language-helpers";
import { ModelChoice, validateModel } from "../settings/ModelChoice";
import { BatchFolderSample, OutputFolderRow, OutputPackagePicker } from "../settings/OutputControls";
import { SettingsRow } from "../settings/SettingsRow";

type SettingsSection = "general" | "transcription" | "output" | "downloads" | "storage" | "advanced" | "privacy";
const sections: Array<{ id: SettingsSection; label: string; icon: IconName }> = [
  { id: "general", label: "General", icon: "settings" },
  { id: "transcription", label: "Transcription", icon: "file-audio" },
  { id: "output", label: "Output", icon: "file" },
  { id: "downloads", label: "Downloads", icon: "download" },
  { id: "storage", label: "Storage", icon: "layers" },
  { id: "advanced", label: "Advanced", icon: "wrench" },
  { id: "privacy", label: "Privacy & About", icon: "shield" },
];

export function SettingsModal({ open, settings, environment, saving, modelOptions, modelValidation, modelBusy, onClose, onSave, onChooseOutput, onOpenDiagnostics, onExportDiagnostics, onOpenOutput, onValidateModel }: {
  open: boolean;
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  saving: boolean;
  modelOptions?: ModelOption[];
  modelValidation?: ModelValidation | null;
  modelBusy?: boolean;
  onClose: () => void;
  onSave: (settings: AppSettings) => void;
  onChooseOutput: () => void;
  onOpenDiagnostics: () => void;
  onExportDiagnostics: () => void;
  onOpenOutput?: () => void;
  onValidateModel?: (model: string) => void;
}) {
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [section, setSection] = useState<SettingsSection>("general");
  const [languageOpen, setLanguageOpen] = useState(false);

  useEffect(() => {
    if (open && settings) { setDraft(settings); setSection("general"); setLanguageOpen(false); }
  }, [open, settings]);

  if (!draft) return null;
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setDraft((current) => current ? { ...current, [key]: value } : current);
  const languagePreferences = normalizeLanguagePreferences(draft.language);
  const setLanguagePreferences = (value: LanguagePreferences) => {
    update("language", value);
  };
  const setModel = (model: string) => update("model", validateModel(model).model_id);

  return <>
    <Modal description="Preferences only. Credentials and media tools are managed in Setup." footer={<><span className="modal-footer-note">Version {environment?.app_version ?? "0.2.0"}</span><Button onClick={onClose}>Cancel</Button><Button disabled={saving} icon="check" onClick={() => onSave(draft)} variant="primary">{saving ? "Saving..." : "Save changes"}</Button></>} onClose={onClose} open={open} size="lg" title="Settings">
      <div className="settings-shell">
        <nav aria-label="Settings sections" className="settings-nav">{sections.map((item) => <button aria-current={section === item.id ? "page" : undefined} className={section === item.id ? "is-active" : ""} key={item.id} onClick={() => setSection(item.id)} type="button"><Icon name={item.icon} size={16} /><span>{item.label}</span></button>)}</nav>
        <div className="settings-content">
          {section === "general" && <SettingsPage description="Appearance and update preferences for this app." title="General"><div className="settings-rows"><SettingsRow description="Use the interface in a light or dark palette." label="Theme"><SegmentedControl label="Theme" onChange={(theme) => update("theme", theme)} options={[{ value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} value={draft.theme} /></SettingsRow><SettingsRow description="Stable is recommended for most users." label="Update channel"><select className="settings-compact-select" onChange={(event) => update("update_channel", event.currentTarget.value)} value={draft.update_channel}><option value="stable">Stable</option><option value="beta">Beta</option></select></SettingsRow></div></SettingsPage>}

          {section === "transcription" && <SettingsPage description="Defaults used when you choose Transcribe selected." title="Transcription"><Field hint="Auto-detect and preserve original is the default. Hints do not translate or exclude other languages." label="Language"><button className="language-summary" onClick={() => setLanguageOpen(true)} type="button"><span><strong>{languagePreferences.mode === "auto" ? "Auto-detect and preserve original" : languageLabel(languagePreferences.hints[0])}</strong><small>{languagePreferences.hints.length ? `${languagePreferences.hints.length} hint${languagePreferences.hints.length === 1 ? "" : "s"} added` : "Recognition hint only"}</small></span><Icon name="chevron-right" size={16} /></button></Field><Field hint="Curated models keep common choices visible; use Advanced for another model ID." label="Model"><ModelChoice onChange={setModel} options={modelOptions} validation={modelValidation ?? undefined} value={draft.model} /></Field><Field hint="Profiles improve terminology and formatting; they never change download behavior." label="Content profile"><select onChange={(event) => update("prompt_preset", event.currentTarget.value)} value={normalizedProfile(draft.prompt_preset)}><option value="default">General audio and video</option><option value="math_science">Math and science</option><option value="technical_code">Technical and code</option><option value="interview">Interview and conversation</option><option value="multilingual">Multilingual / code-switching</option></select></Field><Field hint="Optional words, names, or instructions that must be preserved. Do not paste sensitive material." label="Glossary or additional guidance"><textarea onInput={(event) => update("additional_prompt", event.currentTarget.value)} value={draft.additional_prompt} /></Field><div className="settings-note"><Icon name="info" size={15} /><span>Mixed-language audio remains mixed-language audio. A language hint does not translate speech or remove other languages.</span></div></SettingsPage>}

          {section === "output" && <SettingsPage description="Choose where batch results are saved and which files are created." title="Output"><Field hint="Transcripts, batch manifests, and optional media are saved here." label="Output folder"><OutputFolderRow onChange={onChooseOutput} onOpen={onOpenOutput} path={draft.output_dir} /></Field><Field label="Output package"><OutputPackagePicker formats={draft.output_formats} onChange={(packageValue, formats) => { update("output_package", packageValue); update("output_formats", formats); }} value={draft.output_package} /></Field><BatchFolderSample /></SettingsPage>}

          {section === "downloads" && <SettingsPage description="Options for supported links and retained media." title="Downloads"><Toggle checked={draft.keep_downloaded_media} description="Otherwise link media remains in the recovery cache and transcript output stays clean." label="Keep downloaded media in output" onChange={(value) => update("keep_downloaded_media", value)} /><Field hint="Optional. Used for private sources that you are authorized to access." label="Cookies file"><input onInput={(event) => update("cookies_file", event.currentTarget.value)} value={draft.cookies_file} /></Field><Field hint="Examples: chrome, edge, firefox. Browser cookies are never included in diagnostics." label="Cookies from browser"><input onInput={(event) => update("cookies_from_browser", event.currentTarget.value)} value={draft.cookies_from_browser} /></Field></SettingsPage>}

          {section === "storage" && <SettingsPage description="Bound local cache growth without deleting final user output." title="Storage & history"><div className="settings-grid two"><Field label="Cache limit (GiB)"><input max="200" min="1" onInput={(event) => update("cache_limit_gib", Number(event.currentTarget.value))} type="number" value={draft.cache_limit_gib} /></Field><Field label="Remove cache after (days)"><input max="365" min="1" onInput={(event) => update("cache_max_age_days", Number(event.currentTarget.value))} type="number" value={draft.cache_max_age_days} /></Field></div><div className="settings-note"><Icon name="shield" size={15} /><span>Cache cleanup never removes completed files from your output folder.</span></div></SettingsPage>}

          {section === "advanced" && <SettingsPage description="Processing controls and custom integrations for experienced users." title="Advanced"><div className="advanced-subsection"><h4>Custom model</h4><p>Use a compatible Gemini model ID when it is not one of the curated choices. LectureScribe never switches models automatically.</p><Field error={draft.model.trim() ? undefined : "Enter a Gemini model ID."} label="Model ID"><div className="model-id-row"><input onInput={(event) => setModel(event.currentTarget.value)} placeholder="gemini-..." value={draft.model} /><Button disabled={modelBusy || !draft.model.trim()} icon="check" onClick={() => onValidateModel?.(draft.model)}>{modelBusy ? "Checking..." : "Check model"}</Button></div></Field>{modelValidation?.model_id === draft.model && modelValidation.message && <div className="settings-note"><Icon name={modelValidation.status === "invalid" ? "alert" : "info"} size={15} /><span>{modelValidation.message}</span></div>}</div><div className="advanced-subsection"><h4>Processing</h4><div className="settings-grid three"><Field label="Segment minutes"><input max="30" min="5" onInput={(event) => update("segment_minutes", Number(event.currentTarget.value))} type="number" value={draft.segment_minutes} /></Field><Field label="Overlap seconds"><input max="10" min="0" onInput={(event) => update("overlap_seconds", Number(event.currentTarget.value))} type="number" value={draft.overlap_seconds} /></Field><Field label="Request delay (ms)"><input max="120000" min="0" onInput={(event) => update("request_delay_ms", Number(event.currentTarget.value))} type="number" value={draft.request_delay_ms} /></Field></div></div><Toggle checked={draft.force} description="Ignores successful transcript cache and makes new Gemini requests." label="Force retranscription" onChange={(value) => update("force", value)} /></SettingsPage>}

          {section === "privacy" && <SettingsPage description="Review local data behavior and create safe diagnostics." title="Privacy & About"><div className="privacy-note"><Icon name="shield" size={16} /><span>Sources, downloads, transcripts, cache, and history stay on this computer. Prepared audio segments are sent to Gemini only for transcription.</span></div><div className="settings-actions"><Button icon="wrench" onClick={onOpenDiagnostics}>Open setup center</Button><Button icon="bug" onClick={onExportDiagnostics} variant="ghost">Export sanitized bug report</Button></div><div className="version-row"><span>LectureScribe version</span><strong>{environment?.app_version ?? "0.2.0"}</strong></div></SettingsPage>}
        </div>
      </div>
    </Modal>
    <LanguagePickerDialog onChange={setLanguagePreferences} onClose={() => setLanguageOpen(false)} open={languageOpen} value={languagePreferences} />
  </>;
}

function SettingsPage({ title, description, children }: { title: string; description: string; children: ReactNode }) { return <section className="settings-page"><header><h3>{title}</h3><p>{description}</p></header><div className="settings-page-content">{children}</div></section>; }

export function normalizedProfile(value: string): string {
  if (value === "technical" || value === "technical_math") return "math_science";
  if (["default", "math_science", "technical_code", "interview", "multilingual"].includes(value)) return value;
  return "default";
}

function languageLabel(code: string): string { return code || "Choose a language hint"; }
