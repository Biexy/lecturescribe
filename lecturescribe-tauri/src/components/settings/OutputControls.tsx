import { useState } from "react";
import type { OutputPackage, TranscriptFormat } from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, IconButton, StatusPill } from "../ui";
import { ALL_OUTPUT_FORMATS, formatsForOutputPackage, outputPackageForFormats } from "./output-helpers";
export { ALL_OUTPUT_FORMATS, formatsForOutputPackage, outputPackageForFormats } from "./output-helpers";

export const OUTPUT_PACKAGE_OPTIONS: Array<{ value: OutputPackage; label: string; description: string }> = [
  { value: "readable", label: "Readable", description: "TXT + Markdown" },
  { value: "subtitles", label: "Subtitles", description: "SRT + WebVTT" },
  { value: "complete", label: "Complete", description: "All four formats" },
  { value: "custom", label: "Custom", description: "Choose formats" },
];

export function OutputPackagePicker({ value, formats, onChange }: { value: OutputPackage; formats: TranscriptFormat[]; onChange: (value: OutputPackage, formats: TranscriptFormat[]) => void }) {
  const toggleFormat = (format: TranscriptFormat) => { const next = formats.includes(format) ? formats.filter((item) => item !== format) : [...formats, format]; onChange("custom", next.length ? next : ["text"]); };
  return <div className="output-package-picker"><div aria-label="Output package" className="output-package-options" role="radiogroup">{OUTPUT_PACKAGE_OPTIONS.map((option) => <button aria-checked={value === option.value} className={`output-package-option ${value === option.value ? "is-selected" : ""}`} key={option.value} onClick={() => onChange(option.value, option.value === "custom" ? formats : formatsForOutputPackage(option.value))} role="radio" type="button"><strong>{option.label}</strong><small>{option.description}</small>{value === option.value && <StatusPill tone="info">Selected</StatusPill>}</button>)}</div>{value === "custom" && <fieldset className="format-grid"><legend>Custom formats</legend>{ALL_OUTPUT_FORMATS.map((format) => <label key={format}><input checked={formats.includes(format)} onChange={() => toggleFormat(format)} type="checkbox" /><span><strong>{formatLabel(format)}</strong><small>{formatDescription(format)}</small></span></label>)}</fieldset>}</div>;
}

function formatLabel(format: TranscriptFormat): string { return format === "text" ? "TXT" : format.toUpperCase(); }
function formatDescription(format: TranscriptFormat): string { return ({ text: "Plain text", markdown: "Readable document", srt: "Subtitle file", vtt: "Web captions" } as Record<TranscriptFormat, string>)[format]; }

export function OutputFolderRow({ path, onChange, onOpen, onCopy }: { path: string; onChange: () => void; onOpen?: () => void; onCopy?: () => void }) {
  const [copied, setCopied] = useState(false);
  const copy = () => { if (onCopy) onCopy(); else if (navigator.clipboard) void navigator.clipboard.writeText(path); setCopied(true); window.setTimeout(() => setCopied(false), 1200); };
  return <div className="output-folder-row"><Icon name="folder" size={17} /><span title={path}>{path || "No output folder selected"}</span><Button icon="folder" onClick={onChange} size="sm">Change</Button>{onOpen && <IconButton icon="external" label="Open output folder" onClick={onOpen} size="sm" />}{(onCopy || path) && <IconButton icon="copy" label="Copy output folder path" onClick={copy} size="sm" />}{copied && <StatusPill tone="success">Copied</StatusPill>}</div>;
}

export function BatchFolderSample() {
  return <div className="batch-folder-sample"><div className="batch-folder-heading"><Icon name="layers" size={15} /><strong>Sample batch folder</strong></div><pre>{"Research batch/\n  00 - Batch summary.html\n  Transcripts/\n    Interview [item-001].md\n    Interview [item-001].srt\n  Media/  (when requested)\n  Metadata/\n    batch-manifest.json"}</pre></div>;
}
