param()

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$ResourceDir = Join-Path $Root "lecturescribe-tauri\src-tauri\resources"
$Target = Join-Path $ResourceDir "yt-dlp.exe"
$Temporary = "$Target.download"
$Version = "2026.06.09"
$ExpectedSha256 = "3a48cb955d55c8821b60ccbdbbc6f61bc958f2f3d3b7ad5eaf3d83a543293a27"
$Url = "https://github.com/yt-dlp/yt-dlp/releases/download/$Version/yt-dlp.exe"

New-Item -ItemType Directory -Force -Path $ResourceDir | Out-Null
if (Test-Path $Target) {
    $ExistingSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Target).Hash.ToLowerInvariant()
    if ($ExistingSha256 -eq $ExpectedSha256) {
        Write-Host "Verified bundled yt-dlp $Version."
        exit 0
    }
}

Write-Host "Downloading pinned yt-dlp $Version..."
try {
    Invoke-WebRequest -Uri $Url -OutFile $Temporary
    $ActualSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Temporary).Hash.ToLowerInvariant()
    if ($ActualSha256 -ne $ExpectedSha256) {
        throw "yt-dlp checksum mismatch. Expected $ExpectedSha256, received $ActualSha256."
    }
    Move-Item -Force -LiteralPath $Temporary -Destination $Target
    Write-Host "Verified and saved $Target"
}
finally {
    if (Test-Path $Temporary) {
        Remove-Item -Force -LiteralPath $Temporary
    }
}