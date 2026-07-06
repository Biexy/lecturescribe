param()

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$ResourceDir = Join-Path $Root "lecturescribe-tauri\src-tauri\resources"
$Target = Join-Path $ResourceDir "yt-dlp.exe"
$Url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"

New-Item -ItemType Directory -Force -Path $ResourceDir | Out-Null
Write-Host "Downloading official yt-dlp.exe..."
Invoke-WebRequest -Uri $Url -OutFile $Target
Write-Host "Saved $Target"
