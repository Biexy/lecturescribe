# Roadmap

## Near Term

- Add a preflight command that checks Python, FFmpeg, yt-dlp, API key, and write permissions.
- Add a real progress file with per-video and per-chunk state.
- Add cleaner terminal progress using one status line per video.
- Add export formats: `.txt`, `.md`, `.srt`, and `.docx`.
- Add optional transcript cleanup pass for headings, equations, and paragraph spacing.

## Product UX

- Add a simple desktop UI for non-technical users.
- Support drag-and-drop link files, folders, and media files.
- Show queue status: pending, downloading, transcribing, retrying, done, failed.
- Let users pause and resume a batch.
- Let users choose chunk size, model, output folder, and language rules.

## Reliability

- Add provider abstraction so Gemini, OpenAI, local Whisper, and other engines can be swapped.
- Add adaptive rate limiting instead of a fixed request delay.
- Add checksum-based resume so renamed files still reuse cached chunks.
- Add tests for URL parsing, Drive ID extraction, file matching, and repetition trimming.

## GitHub Release

- Pick a license.
- Add screenshots or a short demo GIF.
- Add GitHub Actions for linting and tests.
- Package a Windows release with a launcher.
