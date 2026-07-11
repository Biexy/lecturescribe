import assert from "node:assert/strict";
import test from "node:test";
import {
  humanizePreviewWarnings,
  playlistConfirmationMessage,
  playlistConfirmations,
} from "../src/lib/preview.ts";

test("parses large-playlist confirmation warnings", () => {
  assert.deepEqual(
    playlistConfirmations(["playlist_confirmation_required:source-1:73:Course: Week 1"]),
    [{ sourceId: "source-1", itemCount: 73, title: "Course: Week 1" }],
  );
});

test("keeps ordinary preview warnings unchanged", () => {
  assert.deepEqual(
    humanizePreviewWarnings(["No supported links were found in links.txt."]),
    ["No supported links were found in links.txt."],
  );
});

test("large-playlist prompts state the safety limit", () => {
  const confirmations = playlistConfirmations([
    "playlist_confirmation_required:first:51:Part one",
    "playlist_confirmation_required:second:82:Part two",
  ]);
  const message = playlistConfirmationMessage(confirmations);
  assert.match(message, /Part one/);
  assert.match(message, /Part two/);
  assert.match(message, /up to 200 items from each playlist/);
});