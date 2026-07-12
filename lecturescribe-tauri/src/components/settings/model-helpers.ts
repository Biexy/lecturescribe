import type { ModelOption, ModelValidation } from "../../types/contracts";

export const CURATED_MODEL_OPTIONS: ModelOption[] = [
  { id: "gemini-3.1-flash-lite", display_name: "Gemini 3.1 Flash-Lite", description: "Fast, economical transcription for everyday batches.", recommended: true, quality_label: "Recommended" },
  { id: "gemini-3.5-flash", display_name: "Gemini 3.5 Flash", description: "Higher quality when difficult audio needs more model capacity.", recommended: false, quality_label: "Higher quality" },
];

export function isRecommendedModel(modelId: string): boolean { return modelId === CURATED_MODEL_OPTIONS[0].id; }
export function modelBadgeForSelection(modelId: string): string | null { if (modelId === CURATED_MODEL_OPTIONS[0].id) return "Recommended"; if (modelId === CURATED_MODEL_OPTIONS[1].id) return "Higher quality"; return modelId.trim() ? "Custom" : null; }
export function validateModel(modelId: string): ModelValidation { const id = modelId.trim().replace(/^models\//, ""); if (!id) return { model_id: id, availability: "unknown", status: "invalid", message: "Enter a Gemini model ID.", checked_at: null }; const curated = CURATED_MODEL_OPTIONS.some((option) => option.id === id); return { model_id: id, availability: curated ? "available" : "unknown", status: curated ? "valid" : "unverified", message: curated ? "" : "Availability will be checked when the app connects.", checked_at: null }; }
