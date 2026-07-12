# Release Checklist

## v0.2.0 Release Gates

All gates below must pass on the exact commit being released. The only release version is `0.2.0`.

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
- Confirm no Python interpreter/runtime is required by the built app, installer, or portable package.
- Run a read-only secret/metadata scan over tracked files and the staged package: reject API-key-like values, `.env` files, cookies, source URLs, personal paths, transcript content, and unintended build metadata. Placeholder values are allowed only when clearly marked as examples.

## Package

- From `lecturescribe-tauri`, run `npm run build`.
- Confirm `scripts/fetch-yt-dlp.ps1` downloaded `lecturescribe-tauri\src-tauri\resources\yt-dlp.exe`.
- Confirm the bundled Downloader checksum matches the pinned value in both `tools.rs` and `fetch-yt-dlp.ps1`.
- Confirm the generated installer detects the bundled Downloader on first run.
- Confirm missing FFmpeg opens the setup wizard with download/choose actions.
- Run `npm run build:portable` to build the portable ZIP from the release executable and bundled resources.
- Confirm the portable ZIP contains `LectureScribe.exe`, `resources\yt-dlp.exe`, `README-PORTABLE.txt`, `LICENSE`, and `THIRD_PARTY_NOTICES.md`.
- Confirm the ZIP contains exactly those five entries and no `.env`, settings, logs, cache, cookies, source media, transcripts, or Python files.
- Verify the emitted `.sha256` with `Get-FileHash -Algorithm SHA256 .\LectureScribe_0.2.0_x64-portable.zip` and compare the lowercase hash exactly. Do not commit either artifact; upload both to the GitHub Release only.
- Verify `LectureScribe_0.2.0_SHA256SUMS.txt` matches the NSIS installer, MSI, and Portable ZIP.

## Smoke Test

- On a clean Windows 10/11 x64 machine with no Node.js, Rust, Python, or FFmpeg installed, install from the installer and separately extract the portable ZIP.
- Launch each package and confirm it starts without a terminal or runtime installation; confirm no Python process/file is required.
- Enter the Gemini API key only through Setup, run the model/audio test, and confirm the key is present only in Windows Credential Manager.
- Add pasted links, a `.txt` link file, and local media.
- Preview, select/unselect, search, filter, and start.
- Verify the selected transcript formats, `00 - Batch summary.html`, and `Metadata\batch-manifest.json`.
- Verify retry failed and cancel do not duplicate successful transcripts.
- Verify sanitized bug report export does not include secrets.
- Unzip the portable package into a clean folder and confirm the bundled Downloader is present.
- Confirm download-only works without FFmpeg/FFprobe, and transcription correctly blocks until FFmpeg/FFprobe are selected.
- Delete the clean-machine test data and remove the temporary credential after testing.
