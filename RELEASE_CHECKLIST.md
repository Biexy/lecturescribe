# Release Checklist

## Before Packaging

- Run `npm install` in `lecturescribe-tauri`.
- Run `npm run check` and confirm the frontend tests and production build pass.
- Run `cargo check --manifest-path .\lecturescribe-tauri\src-tauri\Cargo.toml`.
- Run `cargo test --manifest-path .\lecturescribe-tauri\src-tauri\Cargo.toml`.
- Confirm `Drive links.txt` preview loads 22 items when the file is present.
- Confirm local MP3 and MP4 preview/transcription paths work.
- Confirm a `.TXT` link file is accepted and a playlist over 50 items requires confirmation.
- Confirm download-only completes on a machine without FFmpeg or FFprobe.
- Confirm Setup does not write API keys to `.env` or settings JSON.

## Package

- From `lecturescribe-tauri`, run `npm run build`.
- Confirm `scripts/fetch-yt-dlp.ps1` downloaded `lecturescribe-tauri\src-tauri\resources\yt-dlp.exe`.
- Confirm the bundled Downloader checksum matches the pinned value in both `tools.rs` and `fetch-yt-dlp.ps1`.
- Confirm the generated installer detects the bundled Downloader on first run.
- Confirm missing FFmpeg opens the setup wizard with download/choose actions.
- Build the portable ZIP from the release executable and bundled resources.
- Confirm the portable ZIP contains `LectureScribe.exe`, `resources\yt-dlp.exe`, `README-PORTABLE.txt`, `LICENSE`, and `THIRD_PARTY_NOTICES.md`.

## Smoke Test

- Launch the packaged app on a clean Windows machine.
- Add Gemini API key from Google AI Studio.
- Run Test setup.
- Add pasted links, a `.txt` link file, and local media.
- Preview, select/unselect, search, filter, and start.
- Verify `.txt`, `.md`, and `00_index.md` outputs.
- Verify retry failed and cancel do not duplicate successful transcripts.
- Verify sanitized bug report export does not include secrets.
- Unzip the portable package into a clean folder and confirm the bundled Downloader is present.