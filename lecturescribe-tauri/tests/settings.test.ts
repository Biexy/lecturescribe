import assert from "node:assert/strict";
import test from "node:test";
import { addLanguageHint, filterLanguages, normalizeLanguagePreferences } from "../src/components/settings/language-helpers.ts";
import { isRecommendedModel, modelBadgeForSelection, validateModel } from "../src/components/settings/model-helpers.ts";
import { formatsForOutputPackage, outputPackageForFormats } from "../src/components/settings/output-helpers.ts";
import { settingsFixture } from "./settings-fixtures.ts";

test("language picker keeps the common list searchable and accepts BCP-47 hints", () => {
  assert.ok(filterLanguages("").length >= 100);
  assert.equal(filterLanguages("pt-BR")[0]?.code, "pt-BR");
  assert.ok(filterLanguages("japanese").some((option) => option.code === "ja"));
});

test("language hints are unique, removable, and capped at five", () => {
  const five = ["en", "ar", "fr", "de", "es"];
  assert.deepEqual(addLanguageHint(five, "ja"), five);
  assert.deepEqual(addLanguageHint(["en"], "en"), ["en"]);
  assert.deepEqual(addLanguageHint(["en", "ar"], "fr"), ["en", "ar", "fr"]);
});

test("output packages map to the exact transcript format sets", () => {
  assert.deepEqual(formatsForOutputPackage("readable"), ["text", "markdown"]);
  assert.deepEqual(formatsForOutputPackage("subtitles"), ["srt", "vtt"]);
  assert.deepEqual(formatsForOutputPackage("complete"), ["text", "markdown", "srt", "vtt"]);
  assert.equal(outputPackageForFormats(["markdown", "text"]), "readable");
  assert.equal(outputPackageForFormats(["text", "srt"]), "custom");
});

test("model badges follow the actual selected model ID", () => {
  assert.equal(isRecommendedModel("gemini-3.1-flash-lite"), true);
  assert.equal(modelBadgeForSelection("gemini-3.1-flash-lite"), "Recommended");
  assert.equal(modelBadgeForSelection("gemini-3.5-flash"), "Higher quality");
  assert.equal(modelBadgeForSelection("gemini-custom"), "Custom");
  assert.equal(validateModel("models/gemini-3.1-flash-lite").model_id, "gemini-3.1-flash-lite");
});

test("legacy language values normalize to the exact LanguagePreferences shape", () => {
  assert.deepEqual(normalizeLanguagePreferences("auto"), { mode: "auto", hints: [] });
  assert.deepEqual(normalizeLanguagePreferences("en"), { mode: "hints", hints: ["en"] });
  assert.deepEqual(normalizeLanguagePreferences({ mode: "hints", hints: ["en", "en", "ar", "fr", "de", "es"] }), { mode: "hints", hints: ["en", "ar", "fr", "de", "es"] });
});

test("settings fixture uses the corrected wire fields", () => {
  assert.deepEqual(settingsFixture.language, { mode: "auto", hints: [] });
  assert.equal(settingsFixture.output_package, "readable");
});
