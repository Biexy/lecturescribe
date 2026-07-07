LectureScribe Portable for Windows
==================================

This package does not need an installer.

How to run
----------
1. Extract the entire ZIP folder.
2. Open LectureScribe.exe.
3. In Setup, add your Gemini API key from Google AI Studio:
   https://aistudio.google.com/app/apikey
4. Use Setup or Doctor to check FFmpeg, Downloader, and your output folder.
5. Add links or local media, preview the queue, then start.

Important notes
---------------
- Keep LectureScribe.exe and the resources folder together.
- The Downloader is included as resources\yt-dlp.exe.
- FFmpeg is not included. LectureScribe can detect an existing FFmpeg install,
  or you can choose/install FFmpeg from Setup.
- Your Gemini API key is saved in the Windows credential store, not in this
  portable folder.
- App settings, downloads, transcripts, and cache use the normal LectureScribe
  local data/output folders unless you choose a different output folder.

Privacy
-------
LectureScribe is local-first. Your files and transcripts stay on your computer.
Audio chunks are sent to Gemini only when transcription runs.
