import type { OutputPackage, TranscriptFormat } from "../../types/contracts";

export const ALL_OUTPUT_FORMATS: TranscriptFormat[] = ["text", "markdown", "srt", "vtt"];
export function formatsForOutputPackage(value: OutputPackage): TranscriptFormat[] { if (value === "readable") return ["text", "markdown"]; if (value === "subtitles") return ["srt", "vtt"]; if (value === "complete") return [...ALL_OUTPUT_FORMATS]; return []; }
export function outputPackageForFormats(formats: TranscriptFormat[]): OutputPackage { const normalized = [...new Set(formats)].sort().join(","); if (normalized === ["markdown", "text"].sort().join(",")) return "readable"; if (normalized === ["srt", "vtt"].sort().join(",")) return "subtitles"; if (normalized === ALL_OUTPUT_FORMATS.slice().sort().join(",")) return "complete"; return "custom"; }
