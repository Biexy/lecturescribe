import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { normalizeError } from "../src/lib/backend.ts";
import { capabilityForSelection, describeSelectedWork } from "../src/lib/setup.ts";
import { previewItem } from "./fixtures.ts";

test("selects download capability only when selected work includes links", () => {
  assert.equal(capabilityForSelection("download", []), null);
  assert.equal(capabilityForSelection("download", [previewItem()]), "download_links");
  assert.equal(
    capabilityForSelection("download", [previewItem({ provider: "local", url: null })]),
    null,
  );
});

test("selects local transcription capability for local media", () => {
  assert.equal(
    capabilityForSelection("transcribe", [previewItem({ provider: "local", url: null })]),
    "transcribe_local",
  );
});

test("selects link transcription capability for remote and mixed selections", () => {
  assert.equal(capabilityForSelection("transcribe", [previewItem()]), "transcribe_links");
  assert.equal(
    capabilityForSelection("transcribe", [previewItem(), previewItem({ id: "local-1", provider: "local", url: null })]),
    "transcribe_links",
  );
});

test("describes selected work and keeps local files out of download work", () => {
  const local = previewItem({ id: "local-1", provider: "local", url: null });
  const remote = previewItem({ id: "remote-1" });

  assert.equal(
    describeSelectedWork("download", [local]),
    "Selected files are already local. Choose link items to download.",
  );
  assert.equal(
    describeSelectedWork("download", [remote, local]),
    "1 link will download; 1 local file will be skipped.",
  );
  assert.equal(
    describeSelectedWork("transcribe", [remote, local]),
    "1 link will download first; 1 local file will transcribe directly.",
  );
});

test("settings present auto language as preserving speech and normalize legacy profiles", async () => {
  const source = await readFile(new URL("../src/components/dialogs/SettingsModal.tsx", import.meta.url), "utf8");

  assert.match(source, /normalizeLanguagePreferences\(draft\.language\)/);
  assert.match(source, /Hints do not translate or exclude other languages/);
  assert.match(source, /value === "technical" \|\| value === "technical_math"\) return "math_science"/);
  assert.match(source, /\["default", "math_science", "technical_code", "interview", "multilingual"\]\.includes\(value\)/);
  assert.match(source, /return "default"/);
});

test("normalizes errors to the browser-preview message without global mutation", () => {
  const error = normalizeError(new Error("Tauri command failed"));

  assert.deepEqual(error, {
    code: "desktop_runtime_unavailable",
    category: "setup",
    severity: "warning",
    user_message: "Desktop features are unavailable in this browser preview. Open the LectureScribe desktop app to add sources and run jobs.",
    technical_detail: "The Tauri desktop bridge is not available in this browser context.",
    retryable: false,
    preserved_work: "No local files or settings were changed.",
    recovery_actions: [],
  });
});
