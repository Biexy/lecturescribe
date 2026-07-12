import type { PreviewItem } from "../src/types/contracts";

export function previewItem(overrides: Partial<PreviewItem> = {}): PreviewItem {
  return {
    id: "item-1",
    source_id: "source-1",
    source_kind: "pasted_link",
    provider: "you_tube",
    source_group: "Test source",
    title: "Test lecture",
    source: "https://example.test/lecture",
    canonical_source: "https://example.test/lecture",
    url: "https://example.test/lecture",
    media_path: null,
    existing_media_path: null,
    existing_transcript_path: null,
    thumbnail_url: null,
    duration_seconds: null,
    expected_media_name: null,
    selected: true,
    status: "ready",
    duplicate_of: null,
    error: null,
    ...overrides,
  };
}
