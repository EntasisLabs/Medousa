# Package a single Medousa component (engine, adapter, cli, etc.) into a release tarball.
param(
    [Parameter(Mandatory = $true)]
    [string]$Package,
    [string]$Target = "",
    [string]$Input = "",
    [string]$DistDir = "",
    [switch]$Help
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

function Show-Usage {
    @"
Usage: scripts/release/package-component.ps1 [options]

Options:
  -Package <id>      Component package id (engine, cli, adapter-telegram, …)
  -Target <triple>   Rust target triple (default: host)
  -Input <dir>       Staging dir from build.ps1 (contains bin/)
  -DistDir <dir>     Output directory (default: dist/)
  -Help              Show this help
"@
}

foreach ($arg in $args) {
    switch ($arg) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
        "--package" { throw "--package requires a value; use -Package on PowerShell" }
        "--target" { throw "--target requires a value; use -Target on PowerShell" }
        "--input" { throw "--input requires a value; use -Input on PowerShell" }
        "--dist" { throw "--dist requires a value; use -DistDir on PowerShell" }
    }
}

if ($Help) { Show-Usage; exit 0 }
if (-not $Package) { throw "--package is required" }

Assert-MedousaCommand tar
Push-Location $MEDOUSA_ROOT
try {
    Assert-MedousaVersionsMatch
    $version = Get-MedousaVersion

    if (-not $Target) {
        if ($Input) {
            $meta = Import-MedousaBuildMetaEnv $Input
            if ($meta.MEDOUSA_TARGET) { $Target = $meta.MEDOUSA_TARGET }
        }
        if (-not $Target) { $Target = Get-MedousaHostTarget }
    }

    if (-not $Input) { $Input = Join-Path $MEDOUSA_ROOT "dist\build\$Target" }
    if (-not $DistDir) { $DistDir = Join-Path $MEDOUSA_ROOT "dist" }

    $binDir = Join-Path $Input "bin"
    if (-not (Test-Path -LiteralPath $binDir)) {
        throw "missing bin directory: $binDir (run build.ps1 first)"
    }

    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

    $archiveName = Get-MedousaComponentArchiveName $Package $version $Target
    $archivePath = Join-Path $DistDir $archiveName
    $basename = Get-MedousaComponentBasename $Package $version $Target

    Write-MedousaLog "packaging component $Package → $archiveName"

    $work = Join-Path $DistDir ".pack-work-$basename"
    if (Test-Path -LiteralPath $work) { Remove-Item -Recurse -Force $work }
    $stageBin = Join-Path $work "$basename\bin"
    New-Item -ItemType Directory -Force -Path $stageBin | Out-Null

    $componentBins = Get-MedousaComponentBinaries $Package
    foreach ($bin in $componentBins) {
        $file = Get-MedousaBinaryFilename $bin $Target
        $src = Join-Path $binDir $file
        if (-not (Test-Path -LiteralPath $src)) {
            throw "missing binary for ${Package}: $src"
        }
        Copy-Item -Force $src (Join-Path $stageBin $file)
    }

    $manifestPath = Join-Path $work "$basename\install-manifest.json"
    Write-MedousaComponentInstallManifest -BinDir $stageBin -PackageId $Package -Version $version -Target $Target -OutPath $manifestPath

    Invoke-MedousaTarGz -ArchivePath $archivePath -WorkDir $work -Basename $basename
    Remove-Item -Recurse -Force $work

    $hash = Get-MedousaSha256File $archivePath
    Write-MedousaLog "sha256: $hash"
    Update-MedousaChecksumsFile -DistDir $DistDir -ArchiveName $archiveName -Hash $hash
    Write-MedousaLog "wrote $archivePath"
} finally {
    Pop-Location
}
