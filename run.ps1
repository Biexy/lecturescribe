param()

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$TauriDir = Join-Path $Root "lecturescribe-tauri"
$ReleaseExe = Join-Path $TauriDir "src-tauri\target\release\lecturescribe.exe"
$DebugExe = Join-Path $TauriDir "src-tauri\target\debug\lecturescribe.exe"

if (Test-Path $ReleaseExe) {
    Start-Process -FilePath $ReleaseExe -WorkingDirectory $Root
    return
}

if (Test-Path $DebugExe) {
    Start-Process -FilePath $DebugExe -WorkingDirectory $Root
    return
}

if (Test-Path (Join-Path $TauriDir "package.json")) {
    Push-Location $TauriDir
    try {
        npm run dev
    } finally {
        Pop-Location
    }
    return
}

throw "LectureScribe app files were not found."
