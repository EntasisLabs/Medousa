# Copy medousa_daemon (slim) and optional medousa_local into Tauri sidecar binaries/
# Windows-native equivalent of prepare-engine-sidecar.sh

[CmdletBinding()]
param(
    [switch]$WithoutIroh,
    [switch]$WithIroh,
    [switch]$WithLocalBrain,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

function Show-Usage {
    @"
Usage: scripts/prepare-engine-sidecar.ps1 [options]

Options:
  -WithoutIroh       Omit iroh-transport (LAN-only pairing builds)
  -WithIroh          Include iroh-transport (default for Medousa.app)
  -WithLocalBrain    Also bundle medousa_local (Offline brain sidecar)
  -Help              Show this help

Environment:
  MEDOUSA_EMBEDDED_INFERENCE   auto|metal|cuda|cpu (for -WithLocalBrain only)
  MEDOUSA_WITH_IROH            0|false|no to omit iroh-transport
  CARGO_BUILD_TARGET           Rust target triple (optional)
  MEDOUSA_SIDECAR_DAEMON       Path to a prebuilt medousa_daemon; skips the cargo
                               build and copies it into the sidecar (CI reuse).
  MEDOUSA_SIDECAR_LOCAL        Path to a prebuilt medousa_local; skips the cargo
                               build (only used with -WithLocalBrain).
"@
}

if ($Help) {
    Show-Usage
    exit 0
}

foreach ($arg in $args) {
    switch ($arg) {
        "--without-iroh" { $WithoutIroh = $true }
        "--with-iroh" { $WithIroh = $true }
        "--with-local-brain" { $WithLocalBrain = $true }
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
    }
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$HomeDir = Split-Path -Parent $ScriptDir
$MedousaRoot = (Resolve-Path (Join-Path $HomeDir "..\..")).Path
$BinariesDir = Join-Path $HomeDir "src-tauri\binaries"

function Get-HostTarget {
    if ($env:CARGO_BUILD_TARGET) { return $env:CARGO_BUILD_TARGET.Trim() }
    foreach ($line in (& rustc -vV 2>&1)) {
        if ($line -match '^host:\s*(.+)$') {
            return $matches[1].Trim()
        }
    }
    throw "failed to read host target from rustc -vV"
}

function Invoke-MedousaCargo {
    param([string[]]$CargoArgs)

    $cargoMsvc = Join-Path $MedousaRoot "scripts\dev\cargo-msvc.ps1"
    if (Test-Path -LiteralPath $cargoMsvc) {
        & $cargoMsvc @CargoArgs
    } else {
        Push-Location $MedousaRoot
        try {
            & cargo @CargoArgs
        } finally {
            Pop-Location
        }
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed (exit $LASTEXITCODE)"
    }
}

function Test-WindowsMsvcTarget([string]$Target) {
    return $Target -like "*-pc-windows-msvc"
}

function Get-BinaryFileName([string]$Name, [string]$Target) {
    if (Test-WindowsMsvcTarget $Target) { return "$Name.exe" }
    return $Name
}

function Get-SidecarFileName([string]$Name, [string]$Target) {
    return "$(Get-BinaryFileName "$Name-$Target" $Target)"
}

function Resolve-InferenceFeature([string]$Target) {
    $mode = if ($env:MEDOUSA_EMBEDDED_INFERENCE) { $env:MEDOUSA_EMBEDDED_INFERENCE.Trim() } else { "auto" }
    switch ($mode) {
        "metal" { return "embedded-inference-metal" }
        "cuda" { return "embedded-inference-cuda" }
        "cpu" { return "embedded-inference" }
        "auto" {
            if ($Target -like "*-apple-*") { return "embedded-inference-metal" }
            return "embedded-inference"
        }
        default { throw "unknown MEDOUSA_EMBEDDED_INFERENCE=$mode (expected auto|metal|cuda|cpu)" }
    }
}

function Get-TargetDir {
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    if ($env:MEDOUSA_CARGO_TARGET_DIR) { return $env:MEDOUSA_CARGO_TARGET_DIR }
    return (Join-Path (Split-Path -Parent $MedousaRoot) ".cache/cargo-target")
}

function Find-ReleaseBinary([string]$BinName, [string]$Target) {
    $file = Get-BinaryFileName $BinName $Target
    $targetDir = Get-TargetDir
    $candidates = @(
        (Join-Path $targetDir "$Target/release/$file"),
        (Join-Path $targetDir "release/$file"),
        (Join-Path $MedousaRoot "target/$Target/release/$file"),
        (Join-Path $MedousaRoot "target/release/$file")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    throw "release binary not found: $file (searched under $targetDir and $($MedousaRoot)/target)"
}

$Target = Get-HostTarget
$withIroh = $true
if ($WithoutIroh) { $withIroh = $false }
if ($WithIroh) { $withIroh = $true }
switch ($env:MEDOUSA_WITH_IROH) {
    { $_ -in @("0", "false", "FALSE", "no", "NO", "off", "OFF") } { $withIroh = $false }
    { $_ -in @("1", "true", "TRUE", "yes", "YES", "on", "ON") } { $withIroh = $true }
}

New-Item -ItemType Directory -Force -Path $BinariesDir | Out-Null

$daemonFeatures = @()
if ($withIroh) { $daemonFeatures += "iroh-transport" }

# Reuse a prebuilt daemon when provided (CI passes the artifact from the engine
# build so medousa_daemon is not compiled a second time). Falls back to building
# locally when the env var is unset (dev / standalone builds).
if ($env:MEDOUSA_SIDECAR_DAEMON) {
    if (-not (Test-Path -LiteralPath $env:MEDOUSA_SIDECAR_DAEMON)) {
        throw "MEDOUSA_SIDECAR_DAEMON set but file not found: $($env:MEDOUSA_SIDECAR_DAEMON)"
    }
    Write-Host "prepare-engine-sidecar: reusing prebuilt medousa_daemon -> $($env:MEDOUSA_SIDECAR_DAEMON)"
    $daemonSrc = $env:MEDOUSA_SIDECAR_DAEMON
} else {
    Write-Host "prepare-engine-sidecar: building slim medousa_daemon for $Target..."
    $cargoArgs = @("build", "--release", "-p", "medousa", "--bin", "medousa_daemon")
    if ($daemonFeatures.Count -gt 0) {
        $cargoArgs += @("--features", ($daemonFeatures -join ","))
    }
    Invoke-MedousaCargo -CargoArgs $cargoArgs
    $daemonSrc = Find-ReleaseBinary "medousa_daemon" $Target
}
$daemonSidecar = Get-SidecarFileName "medousa_daemon" $Target
Copy-Item -LiteralPath $daemonSrc -Destination (Join-Path $BinariesDir $daemonSidecar) -Force
Write-Host "prepare-engine-sidecar: $(Join-Path $BinariesDir $daemonSidecar)"

if ($WithLocalBrain) {
    if ($env:MEDOUSA_SIDECAR_LOCAL) {
        if (-not (Test-Path -LiteralPath $env:MEDOUSA_SIDECAR_LOCAL)) {
            throw "MEDOUSA_SIDECAR_LOCAL set but file not found: $($env:MEDOUSA_SIDECAR_LOCAL)"
        }
        Write-Host "prepare-engine-sidecar: reusing prebuilt medousa_local -> $($env:MEDOUSA_SIDECAR_LOCAL)"
        $localSrc = $env:MEDOUSA_SIDECAR_LOCAL
    } else {
        $inferenceFeature = Resolve-InferenceFeature $Target
        Write-Host "prepare-engine-sidecar: building medousa_local ($inferenceFeature)…"
        Invoke-MedousaCargo -CargoArgs @(
            "build", "--release", "-p", "medousa", "--bin", "medousa_local", "--features", $inferenceFeature
        )
        $localSrc = Find-ReleaseBinary "medousa_local" $Target
    }
    $localSidecar = Get-SidecarFileName "medousa_local" $Target
    Copy-Item -LiteralPath $localSrc -Destination (Join-Path $BinariesDir $localSidecar) -Force
    Write-Host "prepare-engine-sidecar: $(Join-Path $BinariesDir $localSidecar)"
}
