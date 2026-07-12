import type { PreviewItem, RunMode, SetupCapability } from "../types/contracts";

export function capabilityForSelection(mode: RunMode, items: PreviewItem[]): SetupCapability | null {
  if (items.length === 0) return null;
  const hasRemote = items.some((item) => item.provider !== "local");
  if (mode === "download") return hasRemote ? "download_links" : null;
  return hasRemote ? "transcribe_links" : "transcribe_local";
}

export function describeSelectedWork(mode: RunMode, items: PreviewItem[]): string {
  if (items.length === 0) return "Select at least one ready queue item.";
  const local = items.filter((item) => item.provider === "local").length;
  const remote = items.length - local;

  if (mode === "download") {
    if (remote === 0) return "Selected files are already local. Choose link items to download.";
    const download = `${remote} ${noun(remote, "link")} will download`;
    return local > 0
      ? `${download}; ${local} local ${noun(local, "file")} will be skipped.`
      : `${download}. Gemini is not used.`;
  }

  if (remote === 0) {
    return `${local} local ${noun(local, "file")} will be transcribed. The downloader is not needed.`;
  }
  if (local === 0) {
    return `${remote} ${noun(remote, "link")} will download, then transcribe.`;
  }
  return `${remote} ${noun(remote, "link")} will download first; ${local} local ${noun(local, "file")} will transcribe directly.`;
}

export function capabilityLabel(capability: SetupCapability): string {
  return {
    download_links: "Download links",
    transcribe_local: "Transcribe local media",
    transcribe_links: "Transcribe links",
  }[capability];
}

function noun(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`;
}
