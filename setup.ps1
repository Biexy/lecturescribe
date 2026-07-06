param()

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot

$EnvFile = Join-Path $Root ".env"
$Example = Join-Path $Root ".env.example"
if (-not (Test-Path $EnvFile)) {
    if (Test-Path $Example) {
        Copy-Item -LiteralPath $Example -Destination $EnvFile
    } else {
        "GEMINI_API_KEY=your-key-here" | Set-Content -LiteralPath $EnvFile -Encoding UTF8
    }
    Write-Host "Created .env. Add your Gemini API key before transcribing."
}

if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
    Write-Warning "FFmpeg was not found on PATH. Install FFmpeg before transcribing local or downloaded media."
} else {
    Write-Host "FFmpeg: ready"
}

$BundledYtDlp = Join-Path $Root "yt-dlp.exe"
if ((Test-Path $BundledYtDlp) -or (Get-Command yt-dlp -ErrorAction SilentlyContinue)) {
    Write-Host "yt-dlp: ready"
} else {
    Write-Warning "yt-dlp was not found. Put yt-dlp.exe in this folder or install yt-dlp on PATH for link downloads."
}

Write-Host "LectureScribe setup check complete. Python is not required for the desktop app."
