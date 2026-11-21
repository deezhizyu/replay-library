$ErrorActionPreference = "Stop"

$releaseTag = "autobuild-2026-11-21-12-37"
$archiveName = "ffmpeg-n9.0.2-61-gd59078794d-win64-lgpl-shared-9.0.zip"
$archiveSha256 = "499fa5e468f9ea6a4d38647f81f9ca4a9ac33655f30faf54b7a800ca9f4d6e5f"

$root = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $root "third_party/ffmpeg"
$marker = Join-Path $destination ".sha256"

if ((Test-Path $marker) -and ((Get-Content $marker -Raw).Trim() -eq $archiveSha256)) {
    Write-Host "FFmpeg $archiveName already present"
    exit 0
}

$workDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("ffmpeg-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory $workDirectory | Out-Null
try {
    $archivePath = Join-Path $workDirectory $archiveName
    $uri = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$releaseTag/$archiveName"
    Invoke-WebRequest -Uri $uri -OutFile $archivePath

    $actualSha256 = (Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLower()
    if ($actualSha256 -ne $archiveSha256) {
        throw "SHA256 mismatch for ${archiveName}: expected $archiveSha256, got $actualSha256"
    }

    Expand-Archive $archivePath -DestinationPath $workDirectory
    $extracted = Get-ChildItem $workDirectory -Directory | Select-Object -First 1
    if (Test-Path $destination) { Remove-Item $destination -Recurse -Force }
    New-Item -ItemType Directory (Split-Path -Parent $destination) -Force | Out-Null
    Move-Item $extracted.FullName $destination
    Set-Content -Path $marker -Value $archiveSha256 -NoNewline
}
finally {
    Remove-Item $workDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
