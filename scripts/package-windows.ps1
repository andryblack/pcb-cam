# Build a Windows release zip for PCB-CAM.
#
# Usage:  pwsh -File scripts/package-windows.ps1
# Output: dist/pcb-cam-<version>-windows-x86_64.zip
#
# The archive contains the release executable, LICENSE and THIRD_PARTY_LICENSES.md.

[CmdletBinding()]
param(
    [string]$Output = ""
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

if ($env:OS -ne 'Windows_NT') {
    throw 'this packaging script is for Windows only'
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'cargo is not installed'
}

$version = ((cargo pkgid -p pcb-cam) -replace '.*[@#]', '').Trim()
if (-not $version) {
    throw 'could not determine the PCB-CAM version'
}

$stageName = "pcb-cam-$version-windows-x86_64"
$dist = Join-Path $Root 'dist'
$stage = Join-Path $dist $stageName
if (-not $Output) {
    $Output = Join-Path $dist "$stageName.zip"
}
$Output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Output)

Write-Host "Building PCB-CAM $version for windows-x86_64..."
cargo build --release --locked -p pcb-cam
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE"
}

if (Test-Path $stage) {
    Remove-Item -Recurse -Force $stage
}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -Path 'target/release/pcb-cam.exe', 'LICENSE', 'THIRD_PARTY_LICENSES.md' -Destination $stage

foreach ($file in @('pcb-cam.exe', 'LICENSE', 'THIRD_PARTY_LICENSES.md')) {
    $path = Join-Path $stage $file
    if (-not (Test-Path $path) -or (Get-Item $path).Length -eq 0) {
        throw "missing or empty $file in the release staging directory"
    }
}

$outputDir = Split-Path -Parent $Output
if ($outputDir -and -not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
}
if (Test-Path $Output) {
    Remove-Item -Force $Output
}

Compress-Archive -Path $stage -DestinationPath $Output -CompressionLevel Optimal

Write-Host ''
Write-Host 'Created:'
Write-Host "  $Output"
