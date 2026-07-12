import type { ModelOption, ModelValidation } from "../../types/contracts";
import { StatusPill } from "../ui";
import { CURATED_MODEL_OPTIONS, modelBadgeForSelection, validateModel } from "./model-helpers";
export { CURATED_MODEL_OPTIONS, isRecommendedModel, modelBadgeForSelection, validateModel } from "./model-helpers";

export function ModelChoice({ value, options = CURATED_MODEL_OPTIONS, validation, onChange }: { value: string; options?: ModelOption[]; validation?: ModelValidation; onChange: (modelId: string) => void }) {
  return <div aria-label="Transcription model" className="model-choice" role="radiogroup">{options.map((option) => { const selected = value === option.id; return <button aria-checked={selected} className={`model-choice-option ${selected ? "is-selected" : ""}`} key={option.id} onClick={() => onChange(option.id)} role="radio" type="button"><span className="model-choice-mark" aria-hidden="true" /><span className="model-choice-copy"><strong>{option.display_name}</strong><small>{option.description}</small></span>{selected && <StatusPill tone={option.recommended ? "info" : "neutral"}>{option.quality_label}</StatusPill>}</button>; })}{validation && validation.status === "invalid" && <p className="field-error" role="alert">{validation.message}</p>}</div>;
}
