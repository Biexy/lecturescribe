# Third-Party Notices

LectureScribe is MIT licensed, but it works with several third-party tools and services.

## yt-dlp

Release builds may include the official Windows `yt-dlp.exe` Downloader from:

https://github.com/yt-dlp/yt-dlp

yt-dlp is distributed under the Unlicense, but the official standalone release binaries include bundled components under GPLv3+ according to the yt-dlp release-file licensing notes. Keep this notice with packaged releases.

## FFmpeg

LectureScribe can install or use an existing FFmpeg build for audio extraction and chunking.

https://ffmpeg.org/

FFmpeg is not bundled by default in LectureScribe releases. Users can install or choose it from the app setup wizard.

## Google Gemini

LectureScribe sends audio chunks to the Gemini API only when transcription runs. Users provide their own API key from Google AI Studio.

https://ai.google.dev/

Google API usage is governed by Google's terms, pricing, and rate limits.

## Tauri, Rust, and npm dependencies

The desktop app is built with Tauri, Rust crates, and npm packages. See `lecturescribe-tauri/Cargo.lock` and `lecturescribe-tauri/package-lock.json` for dependency versions.

## React, React Reconciler, and Scheduler

LectureScribe includes React, React Reconciler, and Scheduler, each distributed
under the MIT License.

Copyright (c) Meta Platforms, Inc. and affiliates.

The complete MIT license text for each package is included in its vendored
`LICENSE` file under `lecturescribe-tauri/vendor/`.

https://github.com/facebook/react
