LectureScribe Portable for Windows
==================================

This package does not need an installer.

How to run
----------
1. Extract the entire ZIP folder.
2. Open LectureScribe.exe.
3. In Setup, enter your Gemini API key from Google AI Studio:
   https://aistudio.google.com/app/apikey
4. Use Setup or Doctor to check FFmpeg, Downloader, and your output folder.
5. Add links or local media, preview the queue, then start.

Important notes
---------------
- Keep LectureScribe.exe and the resources folder together.
- The Downloader is included as resources\yt-dlp.exe.
- FFmpeg is not included. LectureScribe can detect an existing FFmpeg install,
  or you can choose/install FFmpeg from Setup.
- Enter the key only in Setup. It is saved in Windows Credential Manager, not
  in this portable folder. Never put it in a file, command line, or issue log.
- App settings, downloads, transcripts, and cache use the normal LectureScribe
  local data/output folders unless you choose a different output folder.

Privacy
-------
LectureScribe is local-first. Your files and transcripts stay on your computer.
Audio chunks are sent to Gemini only when transcription runs.
Download-only mode does not contact Gemini. The package contains no Python
runtime. Upload the ZIP and `.sha256` checksum to the GitHub Release; do not
commit either artifact to the repository.
