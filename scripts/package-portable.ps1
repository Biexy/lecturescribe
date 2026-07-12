param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$TauriRoot = Join-Path $Root "lecturescribe-tauri\src-tauri"
$ReleaseDir = Join-Path $TauriRoot "target\release"
$PortableDir = Join-Path $ReleaseDir "bundle\portable"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Config = Get-Content -Raw -LiteralPath (Join-Path $TauriRoot "tauri.conf.json") | ConvertFrom-Json
    $Version = $Config.version
}

if ($Version -ne "0.2.0") {
    throw "Portable packaging is release-pinned to v0.2.0; received version '$Version'."
}

$Msi = Join-Path $ReleaseDir "bundle\msi\LectureScribe_${Version}_x64_en-US.msi"
$Installer = Join-Path $ReleaseDir "bundle\nsis\LectureScribe_${Version}_x64-setup.exe"

$Required = @(
    (Join-Path $ReleaseDir "lecturescribe.exe"),
    (Join-Path $ReleaseDir "resources\yt-dlp.exe"),
    $Msi,
    $Installer,
    (Join-Path $Root "README-PORTABLE.txt"),
    (Join-Path $Root "LICENSE"),
    (Join-Path $Root "THIRD_PARTY_NOTICES.md")
)
foreach ($Path in $Required) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Portable package input is missing: $Path"
    }
}

New-Item -ItemType Directory -Force -Path $PortableDir | Out-Null
$Stage = Join-Path $PortableDir "stage-$PID"
$PortableFull = [IO.Path]::GetFullPath($PortableDir).TrimEnd('\') + '\'
$StageFull = [IO.Path]::GetFullPath($Stage)
if (-not $StageFull.StartsWith($PortableFull, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to create a staging directory outside the portable bundle folder."
}

$Zip = Join-Path $PortableDir "LectureScribe_${Version}_x64-portable.zip"
$Checksum = "$Zip.sha256"
$ReleaseChecksums = Join-Path $PortableDir "LectureScribe_${Version}_SHA256SUMS.txt"
try {
    New-Item -ItemType Directory -Force -Path (Join-Path $Stage "resources") | Out-Null
    Copy-Item -LiteralPath (Join-Path $ReleaseDir "lecturescribe.exe") -Destination (Join-Path $Stage "LectureScribe.exe")
    Copy-Item -LiteralPath (Join-Path $ReleaseDir "resources\yt-dlp.exe") -Destination (Join-Path $Stage "resources\yt-dlp.exe")
    Copy-Item -LiteralPath (Join-Path $Root "README-PORTABLE.txt") -Destination $Stage
    Copy-Item -LiteralPath (Join-Path $Root "LICENSE") -Destination $Stage
    Copy-Item -LiteralPath (Join-Path $Root "THIRD_PARTY_NOTICES.md") -Destination $Stage

    if (Test-Path -LiteralPath $Zip) {
        Remove-Item -Force -LiteralPath $Zip
    }
    if (Test-Path -LiteralPath $Checksum) {
        Remove-Item -Force -LiteralPath $Checksum
    }
    Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Zip -CompressionLevel Optimal
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $Archive = [IO.Compression.ZipFile]::OpenRead($Zip)
    try {
        $ActualEntries = @($Archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') } | Sort-Object)
    }
    finally {
        $Archive.Dispose()
    }
    $ExpectedEntries = @("LICENSE", "LectureScribe.exe", "README-PORTABLE.txt", "THIRD_PARTY_NOTICES.md", "resources/yt-dlp.exe") | Sort-Object
    if (($ActualEntries -join "`n") -ne ($ExpectedEntries -join "`n")) {
        throw "Portable ZIP contents do not match the required release package."
    }
    $Hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Zip).Hash.ToLowerInvariant()
    Set-Content -LiteralPath $Checksum -Value "$Hash  $([IO.Path]::GetFileName($Zip))" -Encoding ascii
    if ($Hash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $Zip).Hash.ToLowerInvariant()) {
        throw "Portable ZIP checksum changed during validation."
    }
    $ReleaseHashLines = @($Installer, $Msi, $Zip) | ForEach-Object {
        $ArtifactHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_).Hash.ToLowerInvariant()
        "$ArtifactHash  $([IO.Path]::GetFileName($_))"
    }
    Set-Content -LiteralPath $ReleaseChecksums -Value $ReleaseHashLines -Encoding ascii
    Write-Host "Built portable package: $Zip"
    Write-Host "SHA-256: $Hash"
    Write-Host "Release checksums: $ReleaseChecksums"
}
finally {
    if (Test-Path -LiteralPath $Stage) {
        Remove-Item -Recurse -Force -LiteralPath $Stage
    }
}
