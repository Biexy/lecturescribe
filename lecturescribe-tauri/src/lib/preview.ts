const PLAYLIST_CONFIRMATION = /^playlist_confirmation_required:([^:]+):(\d+):(.*)$/;

export interface PlaylistConfirmation {
  sourceId: string;
  itemCount: number;
  title: string;
}

export function playlistConfirmations(warnings: string[]): PlaylistConfirmation[] {
  return warnings.flatMap((warning) => {
    const match = PLAYLIST_CONFIRMATION.exec(warning);
    if (!match) return [];
    return [{
      sourceId: match[1],
      itemCount: Number(match[2]),
      title: match[3].trim() || "Untitled playlist",
    }];
  });
}

export function playlistConfirmationMessage(confirmations: PlaylistConfirmation[]): string {
  const details = confirmations
    .map(({ title, itemCount }) => `“${title}” (${itemCount} items)`)
    .join("\n");
  return `${details}\n\nAdd up to 200 items from each playlist to the queue?`;
}

export function humanizePreviewWarnings(warnings: string[]): string[] {
  return warnings.map((warning) => {
    const [confirmation] = playlistConfirmations([warning]);
    return confirmation
      ? `Playlist “${confirmation.title}” (${confirmation.itemCount} items) was not expanded.`
      : warning;
  });
}