# LectureScribe

LectureScribe is a small desktop app for downloading and transcribing lecture batches from YouTube links, Google Drive file links, `.txt` link files, and local audio/video files.

The desktop app uses:

- Tauri + TypeScript for the GUI
- a native Rust transcription engine
- FFmpeg for audio extraction/chunking
- yt-dlp for YouTube and Google Drive downloads
- Gemini for transcription

Python is not required for the desktop app. `bulk_transcriber.py` remains in the repo only as a legacy/reference script.

## Requirements

- Gemini API key from AI Studio
- FFmpeg on PATH
- `yt-dlp.exe` in this folder, or `yt-dlp` on PATH, only when downloading links

## Setup

Run the setup check:

```powershell
.\setup.ps1
```

Then launch the app:

```powershell
.\run.ps1
```

In the app, open **Settings**, paste your Gemini API key, and save it. The recommended default model is `gemini-3.1-flash-lite` because it is easy to get in AI Studio and is usually the most free-tier-friendly option.

Use **Test setup** after saving the key if you want to verify FFmpeg, Gemini, and the native audio request path. The test uses one Gemini request.

## Basic Use

1. Add sources with **Paste links**, **Add .txt link file**, drag and drop, or **Add media**.
2. Review the queue. Preview updates automatically and every queue row is selectable.
3. Uncheck anything you do not want to transcribe.
4. Click **Start transcription**.

When a `.txt` link file is added, LectureScribe shows how many links it found. Links and local media stay separated in the source inbox, and long URLs are truncated in the table without breaking the layout.

During a run you can cancel at the next safe point. Completed transcript chunks stay cached, and failed items can be selected and retried without redoing successful cached chunks unless **Force re-transcribe** is enabled.

## Outputs

By default, transcripts are written to:

```text
Transcripts\organized
```

The generated `00_index.md` tracks completed and pending transcript files. Audio chunks and cached chunk transcripts are stored under the work/cache folder so interrupted runs can resume without duplicating transcript text.

## Developer Commands

From `lecturescribe-tauri`:

```powershell
npm run dev
npm run build:frontend
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml
```

Create a desktop release:

```powershell
npm run build
```

## GitHub Safety

Do not commit `.env`, transcripts, logs, downloaded media, or cache folders. The included `.gitignore` excludes the normal generated files.
