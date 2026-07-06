param()

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot

Write-Host "LectureScribe desktop setup is handled inside the app."
Write-Host "Normal users should run the installer, open LectureScribe, and follow the first-run wizard."
Write-Host ""

if (Get-Command ffmpeg -ErrorAction SilentlyContinue) {
    Write-Host "FFmpeg: available on PATH"
} else {
    Write-Warning "FFmpeg was not found on PATH. The app wizard can install it or let you choose ffmpeg.exe."
}

$AppManagedYtDlp = Join-Path $env:LOCALAPPDATA "LectureScribe\tools\yt-dlp.exe"
$LocalYtDlp = Join-Path $Root "yt-dlp.exe"
if ((Test-Path $AppManagedYtDlp) -or (Test-Path $LocalYtDlp) -or (Get-Command yt-dlp -ErrorAction SilentlyContinue)) {
    Write-Host "Downloader: available"
} else {
    Write-Host "Downloader: not installed yet. Release builds bundle it; the app can also install/update it."
}

Write-Host ""
Write-Host "Developer launch:"
Write-Host ".\run.ps1"
