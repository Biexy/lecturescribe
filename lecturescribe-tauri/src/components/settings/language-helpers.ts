import type { LanguagePreferences } from "../../types/contracts";
// @ts-expect-error The raw Node test runner requires explicit TypeScript extensions.
import { LANGUAGE_OPTIONS, type LanguageOption } from "./language-data.ts";

export const MAX_LANGUAGE_HINTS = 5;

export function filterLanguages(query: string, options: LanguageOption[] = LANGUAGE_OPTIONS): LanguageOption[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return options;
  return options.filter((option) => `${option.name} ${option.nativeName} ${option.code}`.toLocaleLowerCase().includes(normalized));
}

export function addLanguageHint(hints: string[], code: string, limit = MAX_LANGUAGE_HINTS): string[] {
  if (!code || hints.includes(code) || hints.length >= limit) return hints;
  return [...hints, code];
}

export function removeLanguageHint(hints: string[], code: string): string[] { return hints.filter((hint) => hint !== code); }

export function normalizeLanguagePreferences(value: unknown): LanguagePreferences {
  if (typeof value === "string") {
    const hint = value.trim().toLocaleLowerCase();
    return hint === "en" || hint === "ar" ? { mode: "hints", hints: [hint] } : { mode: "auto", hints: [] };
  }
  const candidate = typeof value === "object" && value !== null ? value as Partial<LanguagePreferences> : {};
  const hints = Array.isArray(candidate.hints) ? [...new Set(candidate.hints.filter((hint): hint is string => typeof hint === "string" && hint.trim().length > 0))].slice(0, MAX_LANGUAGE_HINTS) : [];
  const mode = candidate.mode === "hints" && hints.length > 0 ? "hints" : "auto";
  return { mode, hints: [...new Set(hints)] };
}
