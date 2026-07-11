# LectureScribe

LectureScribe is a local-first desktop app for turning videos and audio into organized transcripts.

It supports YouTube links, Google Drive file links, `.txt` link lists, and local audio/video files. The app previews the queue before starting, skips duplicates, tracks progress, and writes clean transcript files plus an index.

![LectureScribe main window](docs/assets/lecturescribe-main.png)

## Why LectureScribe?

- Batch transcribe videos or audio from links, text files, or local media.
- Preview and select exactly what will run before spending Gemini requests.
- Resume/retry with cached downloads and chunks.
- No LectureScribe account, subscription, Python setup, or command line is required for normal use.
- Local-first: your files stay on your computer, and audio chunks are sent to Gemini only during transcription.

## Highlights

- Native Windows desktop app built with Tauri, TypeScript, and Rust.
- No Python runtime, virtualenv, pip install, or `.env` setup for normal users.
- Bring your own Gemini API key from [Google AI Studio](https://aistudio.google.com/app/apikey).
- Default model: `gemini-3.1-flash-lite`.
- Bundled Downloader for release builds, powered by `yt-dlp`.
- Guided FFmpeg setup in the first-run wizard.
- Supports batch preview, selection, retry failed, cancel, cache reuse, run history, and sanitized bug report export.

## Current Status

Version 0.2 is currently verified as a source build. This checkout does not contain a signed installer or portable ZIP. Release links should be added only after those artifacts are published and smoke-tested.

## Run From Source

1. Install Node.js, npm, and the Rust toolchain.
2. Run `npm install` in `lecturescribe-tauri`.
3. Run `npm run dev`.
4. Follow the first-run setup wizard:
   - add your Gemini API key from [Google AI Studio](https://aistudio.google.com/app/apikey),
   - confirm FFmpeg,
   - confirm Downloader,
   - choose an output folder,
   - run the setup test.
5. Add links, a `.txt` link file, or local media.
6. Review the queue and start.

The setup test uses one tiny Gemini request to verify the key, FFmpeg audio path, and Gemini request path.
## Get Your Gemini API Key

LectureScribe uses your own Gemini API key. The easiest place to get one is:

**[Get a Gemini API key in Google AI Studio](https://aistudio.google.com/app/apikey)**

Basic steps:

1. Sign in with your Google account.
2. Click **Create API key**.
3. Create the key in a new project if you do not already have one.
4. Copy the key into LectureScribe's Setup screen.
5. Run **Test setup** in the app.

Google's official guide is here: [Using Gemini API keys](https://ai.google.dev/gemini-api/docs/api-key).

`gemini-3.1-flash-lite` is the app default. Model availability, pricing, and rate limits can change, so check Google's current model and pricing documentation before a release.

The app saves the key in the OS secure credential store. It is not written to `.env`, settings JSON, logs, history, or diagnostic exports.

## Supported Sources

- YouTube links.
- Google Drive file links.
- `.txt` files containing links.
- Local media files:
  - `mp3`, `m4a`, `wav`, `mp4`, `mov`, `mkv`, `webm`, `flac`, `ogg`, `opus`, `aac`.

Pasted links are added as a source group. Individual queue items can be selected or excluded before a run. `.txt` files show the detected link count immediately. Duplicate items are skipped and explained.

## Run Modes

- **Download + transcribe**: download link sources, then transcribe.
- **Download only**: download YouTube/Drive media without using Gemini.
- **Transcribe existing media**: transcribe local files or already downloaded media.

The Downloader is required only for link download modes. FFmpeg is required for transcription and chunking.

## Output

By default, transcripts are written to:

```text
%LOCALAPPDATA%\LectureScribe\Transcripts
```

With the default output settings, each completed transcription writes:

- a clean `.txt` transcript,
- a readable Markdown `.md` transcript,
- cached chunk transcripts for resume/retry,
- `00_index.md` and `batch-manifest.json` for the batch.

Successful cached chunks are reused unless **Force re-transcribe** is enabled.

## Privacy

LectureScribe is local-first:

- Your sources, downloads, transcripts, cache, and history stay on your computer.
- Audio chunks are sent to Gemini only when transcription runs.
- Diagnostic exports are sanitized and do not include API keys.

## Developer Setup

Requirements:

- Node.js and npm.
- Rust toolchain.
- FFmpeg for transcription tests.

From `lecturescribe-tauri`:

```powershell
npm install
npm run dev
npm run check
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml
```

Create a release build:

```powershell
npm run build
```

`npm run build` downloads the pinned Windows `yt-dlp.exe`, verifies its SHA-256 checksum, and places it in Tauri resources before packaging. The binary is ignored by Git.

## Legacy Python

The old Python implementation is archived in `archive/python-legacy/` for reference only. It is not part of the normal app runtime, setup, or release.

## Troubleshooting

- **Missing Gemini key**: open Setup and save a key from Google AI Studio.
- **Missing FFmpeg**: open the FFmpeg download page from Setup, install it, or choose an existing `ffmpeg.exe`.
- **Private Drive/YouTube links fail**: add browser cookies in Advanced settings.
- **Downloader missing**: release builds include it; Setup can also install or update it.
- **Portable ZIP does not download links**: make sure `resources\yt-dlp.exe` is still beside `LectureScribe.exe`.
- **Repeated transcript text**: retry with Force disabled first so cached successful chunks are reused.

## Project Files

- License: `LICENSE`
- Third-party notices: `THIRD_PARTY_NOTICES.md`
- Release validation: `RELEASE_CHECKLIST.md`