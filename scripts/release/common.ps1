# Medousa release scripts — shared constants and helpers (Windows PowerShell).
# Dot-source from other scripts: . "$PSScriptRoot/common.ps1"

$ErrorActionPreference = "Stop"

$script:MedousaGithubRepo = if ($env:MEDOUSA_GITHUB_REPO) { $env:MEDOUSA_GITHUB_REPO } else { "EntasisLabs/Medousa" }
$script:MedousaReleaseBaseUrl = $env:MEDOUSA_RELEASE_BASE_URL
$script:MedousaReleaseChannel = if ($env:MEDOUSA_RELEASE_CHANNEL) { $env:MEDOUSA_RELEASE_CHANNEL } else { "stable" }

$script:MedousaBinaries = @(
    "medousa",
    "medousa_cli",
    "medousa_daemon",
    "medousa_local",
    "medousa_tui",
    "medousa_telegram",
    "medousa_discord",
    "medousa_slack",
    "medousa_mcp_gateway",
    "medousa_whatsapp"
)

$script:MedousaComponentIds = @(
    "engine",
    "cli",
    "adapter-telegram",
    "adapter-discord",
    "adapter-slack",
    "adapter-whatsapp",
    "mcp-gateway"
)

function Get-MedousaRepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
}

$script:MEDOUSA_ROOT = Get-MedousaRepoRoot
$script:MEDOUSA_WHATSAPP_MANIFEST = "adapters/medousa-whatsapp/Cargo.toml"

function Get-MedousaParseCargoVersion([string]$TomlPath) {
    $line = Select-String -Path $TomlPath -Pattern '^version = "(.*)"' | Select-Object -First 1
    if (-not $line) { throw "version not found in $TomlPath" }
    return $line.Matches.Groups[1].Value
}

function Get-MedousaVersion {
    $root = Get-MedousaRepoRoot
    return Get-MedousaParseCargoVersion (Join-Path $root "Cargo.toml")
}

function Get-MedousaWhatsappVersion {
    $root = Get-MedousaRepoRoot
    return Get-MedousaParseCargoVersion (Join-Path $root "adapters/medousa-whatsapp/Cargo.toml")
}

function Assert-MedousaVersionsMatch {
    $rootV = Get-MedousaVersion
    $waV = Get-MedousaWhatsappVersion
    if ($rootV -ne $waV) {
        throw "version mismatch — root Cargo.toml ($rootV) != whatsapp ($waV)"
    }
}

function Get-MedousaHostTarget {
    $hostLine = (& rustc -vV | Select-String -Pattern '^host: ').Line
    if (-not $hostLine) { throw "failed to read host target from rustc -vV" }
    return $hostLine.Substring(6).Trim()
}

function Test-MedousaWindowsMsvcTarget([string]$Target) {
    return $Target -like "*-pc-windows-msvc"
}

function Get-MedousaBinaryFilename([string]$Name, [string]$Target) {
    if (Test-MedousaWindowsMsvcTarget $Target) { return "$Name.exe" }
    return $Name
}

function Get-MedousaAssetBasename([string]$Version, [string]$Target) {
    return "medousa-v$Version-$Target"
}

function Get-MedousaAssetArchiveName([string]$Version, [string]$Target) {
    return "$(Get-MedousaAssetBasename $Version $Target).tar.gz"
}

function Get-MedousaComponentBinaries([string]$PackageId) {
    switch ($PackageId) {
        "engine" { return @("medousa", "medousa_daemon") }
        "cli" { return @("medousa_cli", "medousa_tui") }
        "adapter-telegram" { return @("medousa_telegram") }
        "adapter-discord" { return @("medousa_discord") }
        "adapter-slack" { return @("medousa_slack") }
        "adapter-whatsapp" { return @("medousa_whatsapp") }
        "mcp-gateway" { return @("medousa_mcp_gateway") }
        default { throw "unknown component package: $PackageId" }
    }
}

function Get-MedousaComponentBasename([string]$PackageId, [string]$Version, [string]$Target) {
    return "$PackageId-v$Version-$Target"
}

function Get-MedousaComponentArchiveName([string]$PackageId, [string]$Version, [string]$Target) {
    return "$(Get-MedousaComponentBasename $PackageId $Version $Target).tar.gz"
}

function Get-MedousaDefaultCargoTargetDir {
    $root = Get-MedousaRepoRoot
    return (Join-Path (Split-Path -Parent $root) ".cache/cargo-target")
}

function Get-MedousaCargoTargetRoot {
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    if ($env:MEDOUSA_CARGO_TARGET_DIR) { return $env:MEDOUSA_CARGO_TARGET_DIR }
    return Get-MedousaDefaultCargoTargetDir
}

function Get-MedousaCargoReleaseDir([string]$Target) {
    $base = Get-MedousaCargoTargetRoot
    if ($Target) { return Join-Path $base "$Target/release" }
    return Join-Path $base "release"
}

function Get-MedousaWhatsappCargoReleaseDir([string]$Target) {
    return Get-MedousaCargoReleaseDir $Target
}

function Find-MedousaReleaseBinary([string]$Bin, [string]$Target) {
    $file = Get-MedousaBinaryFilename $Bin $Target
    $candidates = @(
        (Join-Path (Get-MedousaCargoReleaseDir $Target) $file),
        (Join-Path (Get-MedousaWhatsappCargoReleaseDir $Target) $file),
        (Join-Path $MEDOUSA_ROOT "target/release/$file"),
        (Join-Path $MEDOUSA_ROOT "target/$Target/release/$file")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    return $null
}

function Import-MedousaBuildMetaEnv([string]$InputDir) {
    $metaPath = Join-Path $InputDir "build-meta.env"
    if (-not (Test-Path -LiteralPath $metaPath)) { return @{} }
    $values = @{}
    Get-Content -LiteralPath $metaPath | ForEach-Object {
        if ($_ -match '^\s*#' -or $_ -match '^\s*$') { return }
        $pair = $_ -split '=', 2
        if ($pair.Count -eq 2) {
            $values[$pair[0].Trim()] = $pair[1].Trim()
        }
    }
    return $values
}

function Get-MedousaReadManifestField([string]$ManifestPath, [string]$Field) {
    $line = Select-String -Path $ManifestPath -Pattern "`"$Field`": `"([^`"]*)`"" | Select-Object -First 1
    if (-not $line) { return "" }
    return $line.Matches.Groups[1].Value
}

function Write-MedousaLog([string]$Message) {
    Write-Host "[medousa-release] $Message"
}

function Assert-MedousaCommand([string]$Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        throw "required command not found: $Command"
    }
}

function Get-MedousaSha256File([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-MedousaComponentSetIdForBinaries([string]$BinDir, [string]$Target, [string[]]$Bins) {
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $buffer = New-Object System.Collections.Generic.List[byte]
        foreach ($bin in $Bins) {
            $file = Get-MedousaBinaryFilename $bin $Target
            $path = Join-Path $BinDir $file
            if (-not (Test-Path -LiteralPath $path)) {
                throw "missing binary for component set: $path"
            }
            $bytes = [System.IO.File]::ReadAllBytes($path)
            $buffer.AddRange($bytes)
        }
        $hash = $hasher.ComputeHash($buffer.ToArray())
        return ([BitConverter]::ToString($hash) -replace "-", "").ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Write-MedousaComponentInstallManifest {
    param(
        [string]$BinDir,
        [string]$PackageId,
        [string]$Version,
        [string]$Target,
        [string]$OutPath,
        [string]$BuiltAt = $null
    )
    if (-not $BuiltAt) {
        $BuiltAt = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    }
    $bins = Get-MedousaComponentBinaries $PackageId
    $componentSetId = Get-MedousaComponentSetIdForBinaries $BinDir $Target $bins
    $binJson = ($bins | ForEach-Object { "`"$_`"" }) -join ", "
    @"
{
  "schema_version": 2,
  "product": "medousa",
  "package_id": "$PackageId",
  "version": "$Version",
  "target": "$Target",
  "built_at": "$BuiltAt",
  "binaries": [$binJson],
  "component_set_id": "$componentSetId"
}
"@ | Set-Content -LiteralPath $OutPath -Encoding utf8NoBOM
}

function Write-MedousaInstallManifest {
    param(
        [string]$BinDir,
        [string]$Version,
        [string]$Target,
        [string]$OutPath,
        [string]$BuiltAt = $null
    )
    if (-not $BuiltAt) {
        $BuiltAt = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    }
    $componentSetId = Get-MedousaComponentSetIdForBinaries $BinDir $Target $script:MedousaBinaries
    $binJson = ($script:MedousaBinaries | ForEach-Object { "`"$_`"" }) -join ", "
    @"
{
  "schema_version": 1,
  "product": "medousa",
  "version": "$Version",
  "target": "$Target",
  "built_at": "$BuiltAt",
  "binaries": [$binJson],
  "component_set_id": "$componentSetId"
}
"@ | Set-Content -LiteralPath $OutPath -Encoding utf8NoBOM
}

function Update-MedousaChecksumsFile([string]$DistDir, [string]$ArchiveName, [string]$Hash) {
    $checksumsFile = Join-Path $DistDir "SHA256SUMS"
    $lines = @()
    if (Test-Path -LiteralPath $checksumsFile) {
        $lines = Get-Content -LiteralPath $checksumsFile | Where-Object { $_ -notmatch "  $([regex]::Escape($ArchiveName))$" }
    }
    $lines += "$Hash  $ArchiveName"
    $lines | Set-Content -LiteralPath $checksumsFile -Encoding utf8NoBOM
}

function Invoke-MedousaTarGz([string]$ArchivePath, [string]$WorkDir, [string]$Basename) {
    Assert-MedousaCommand tar
    if (Test-Path -LiteralPath $ArchivePath) {
        Remove-Item -LiteralPath $ArchivePath -Force
    }
    & tar -czf $ArchivePath -C $WorkDir $Basename
    if ($LASTEXITCODE -ne 0) { throw "tar failed creating $ArchivePath" }
}
