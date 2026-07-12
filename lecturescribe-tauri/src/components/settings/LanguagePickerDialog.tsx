import { useEffect, useMemo, useState } from "react";
import type { LanguagePreferences } from "../../types/contracts";
import { Icon } from "../Icon";
import { Button, IconButton, Modal, StatusPill } from "../ui";
import { LANGUAGE_OPTION_MAP } from "./language-data";
import { addLanguageHint, filterLanguages, MAX_LANGUAGE_HINTS, removeLanguageHint } from "./language-helpers";

export function LanguagePickerDialog({ open, value, onClose, onChange }: { open: boolean; value: LanguagePreferences; onClose: () => void; onChange: (value: LanguagePreferences) => void }) {
  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    if (open) { setQuery(""); setDraft(value); }
  }, [open, value]);

  const matches = useMemo(() => filterLanguages(query), [query]);
  const visible = query.trim() ? matches : matches.slice(0, 12);
  const selectPrimary = (primary: string) => {
    const next = primary === "auto" ? { mode: "auto" as const, hints: [] } : { mode: "hints" as const, hints: addLanguageHint(draft.hints, primary) };
    setDraft(next); onChange(next);
  };
  const addHint = (code: string) => { const next = { mode: "hints" as const, hints: addLanguageHint(draft.hints, code) }; setDraft(next); onChange(next); };
  const removeHint = (code: string) => { const hints = removeLanguageHint(draft.hints, code); const next = { mode: hints.length ? "hints" as const : "auto" as const, hints }; setDraft(next); onChange(next); };

  return <Modal description="Choose a recognition hint without changing the language of the saved transcript." footer={<Button icon="check" onClick={onClose} variant="primary">Done</Button>} onClose={onClose} open={open} size="md" title="Transcription language">
    <div className="language-picker">
      <div className="language-search"><Icon name="search" size={16} /><input autoFocus aria-label="Search languages" onChange={(event) => setQuery(event.currentTarget.value)} placeholder="Search by language or code" value={query} /></div>
      <div className="language-picker-note"><Icon name="info" size={15} /><span>Hints improve recognition. They do not translate, exclude, or replace other spoken languages.</span></div>
      <div className="language-primary-label">Primary expectation</div>
      <button className={`language-option ${draft.mode === "auto" ? "is-selected" : ""}`} onClick={() => selectPrimary("auto")} type="button"><span><strong>Auto-detect and preserve original</strong><small>Recommended for mixed-language audio</small></span>{draft.mode === "auto" && <StatusPill tone="info">Selected</StatusPill>}</button>
      <div className="language-primary-label">Common languages</div>
      <div aria-label="Language results" className="language-results" role="listbox">
        {visible.map((option) => <div className="language-result" key={option.code} role="option" aria-selected={draft.mode === "hints" && draft.hints[0] === option.code}><button className={draft.mode === "hints" && draft.hints[0] === option.code ? "is-selected" : ""} onClick={() => selectPrimary(option.code)} type="button"><span><strong>{option.name}</strong><small>{option.nativeName} · {option.code}</small></span>{draft.mode === "hints" && draft.hints[0] === option.code && <StatusPill tone="info">Selected</StatusPill>}</button><button className="language-hint-button" disabled={draft.hints.length >= MAX_LANGUAGE_HINTS || draft.hints.includes(option.code)} onClick={() => addHint(option.code)} type="button">{draft.hints.includes(option.code) ? "Added" : "Add hint"}</button></div>)}
        {visible.length === 0 && <p className="language-empty">No languages match "{query}".</p>}
      </div>
      {!query.trim() && matches.length > visible.length && <p className="language-search-more">{matches.length - visible.length} more languages available. Search to find them.</p>}
      <div className="language-hints"><div className="language-hints-header"><strong>Hints</strong><span>{draft.hints.length}/{MAX_LANGUAGE_HINTS}</span></div>{draft.hints.length === 0 ? <p className="language-empty">No hints added.</p> : draft.hints.map((code) => { const option = LANGUAGE_OPTION_MAP.get(code); return <span className="language-hint" key={code}>{option?.name ?? code}<IconButton icon="x" label={`Remove ${option?.name ?? code} hint`} onClick={() => removeHint(code)} size="sm" /></span>; })}</div>
    </div>
  </Modal>;
}
