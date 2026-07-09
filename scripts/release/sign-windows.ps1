# Sign Medousa Windows release artifacts with Azure Artifact Signing (Trusted Signing).
# Requires: Azure Artifact Signing client tools + metadata.json + az login (or managed identity).
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Paths,
    [string]$MetadataFile = $env:MEDOUSA_AZURE_SIGNING_METADATA,
    [string]$SignTool = $env:MEDOUSA_SIGNTOOL,
    [string]$Dlib = $env:MEDOUSA_AZURE_SIGNING_DLIB,
    [switch]$Verify,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot/common.ps1"

function Show-Usage {
    @"
Usage: scripts/release/sign-windows.ps1 [options] <file.exe|file.msi> [...]

Sign Windows executables and installers with Azure Artifact Signing.

Options:
  -MetadataFile <path>   JSON with Endpoint, CodeSigningAccountName, CertificateProfileName
  -SignTool <path>       signtool.exe (default: newest Windows SDK x64 signtool)
  -Dlib <path>           Azure.CodeSigning.Dlib.dll (x64)
  -Verify                Verify signatures after signing
  -Help                  Show this help

Environment:
  MEDOUSA_AZURE_SIGNING_METADATA   path to metadata.json
  MEDOUSA_AZURE_SIGNING_DLIB       path to Azure.CodeSigning.Dlib.dll
  MEDOUSA_SIGNTOOL                 path to signtool.exe

Setup (one time):
  winget install -e --id Microsoft.Azure.ArtifactSigningClientTools
  az login
  Create metadata.json from your Azure Artifact Signing account (see docs/cookbook/release-to-r2.md)

Examples:
  .\scripts\release\sign-windows.ps1 dist\final\Medousa_0.1.0_x64-setup.exe
  .\scripts\release\sign-windows.ps1 dist\final\MedousaInstaller_0.1.0_x64-setup.exe
  .\scripts\release\sign-windows.ps1 -Verify apps\medousa-home\src-tauri\target\release\bundle\nsis\*.exe
"@
}

if ($Help) {
    Show-Usage
    exit 0
}

function Resolve-SignToolPath {
    if ($SignTool -and (Test-Path -LiteralPath $SignTool)) {
        return (Resolve-Path -LiteralPath $SignTool).Path
    }
    $kitsRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
    if (-not (Test-Path -LiteralPath $kitsRoot)) {
        throw "signtool not found — install Windows SDK or set MEDOUSA_SIGNTOOL"
    }
    $candidate = Get-ChildItem -Path $kitsRoot -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName "x64\signtool.exe" } |
        Where-Object { Test-Path -LiteralPath $_ } |
        Select-Object -First 1
    if (-not $candidate) {
        throw "signtool.exe not found under $kitsRoot"
    }
    return $candidate
}

function Resolve-DlibPath {
    if ($Dlib -and (Test-Path -LiteralPath $Dlib)) {
        return (Resolve-Path -LiteralPath $Dlib).Path
    }
    $searchRoots = @(
        "${env:ProgramFiles}\Microsoft\Azure Artifact Signing Client Tools",
        "${env:ProgramFiles(x86)}\Microsoft\Azure Artifact Signing Client Tools",
        "${env:ProgramFiles}\Microsoft\Trusted Signing Client Tools",
        "$env:LOCALAPPDATA\Microsoft\Azure Artifact Signing"
    )
    foreach ($root in $searchRoots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $found = Get-ChildItem -Path $root -Recurse -Filter "Azure.CodeSigning.Dlib.dll" -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Select-Object -First 1
        if ($found) { return $found.FullName }
    }
    throw "Azure.CodeSigning.Dlib.dll not found — install Artifact Signing Client Tools or set MEDOUSA_AZURE_SIGNING_DLIB"
}

function Resolve-MetadataPath {
    if ($MetadataFile -and (Test-Path -LiteralPath $MetadataFile)) {
        return (Resolve-Path -LiteralPath $MetadataFile).Path
    }
    $repoDefault = Join-Path (Get-MedousaRepoRoot) "scripts/release/azure-signing-metadata.json"
    if (Test-Path -LiteralPath $repoDefault) {
        return $repoDefault
    }
    throw "metadata.json not found — copy scripts/release/azure-signing-metadata.example.json and fill in Azure values"
}

function Expand-SignTargets {
    param([string[]]$Inputs)
    $resolved = @()
    foreach ($item in $Inputs) {
        if ($item -match '[*?]') {
            $resolved += Get-ChildItem -Path $item -File -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
        } elseif (Test-Path -LiteralPath $item) {
            $resolved += (Resolve-Path -LiteralPath $item).Path
        } else {
            throw "file not found: $item"
        }
    }
    return $resolved | Select-Object -Unique
}

if (-not $Paths -or $Paths.Count -eq 0) {
    Show-Usage
    exit 1
}

$signToolPath = Resolve-SignToolPath
$dlibPath = Resolve-DlibPath
$metadataPath = Resolve-MetadataPath
$targets = Expand-SignTargets -Inputs $Paths

if ($targets.Count -eq 0) {
    throw "no files matched"
}

Write-MedousaLog "signing $($targets.Count) file(s) with Azure Artifact Signing"
Write-MedousaLog "signtool: $signToolPath"
Write-MedousaLog "metadata: $metadataPath"

$signArgs = @(
    "sign", "/v", "/fd", "SHA256",
    "/tr", "http://timestamp.acs.microsoft.com",
    "/td", "SHA256",
    "/dlib", $dlibPath,
    "/dmdf", $metadataPath
)

foreach ($target in $targets) {
    Write-MedousaLog "sign $target"
    & $signToolPath @signArgs $target
    if ($LASTEXITCODE -ne 0) {
        throw "signtool failed for $target (exit $LASTEXITCODE)"
    }
    if ($Verify) {
        & $signToolPath verify /pa /v $target
        if ($LASTEXITCODE -ne 0) {
            throw "verify failed for $target"
        }
    }
}

Write-MedousaLog "signing complete"
