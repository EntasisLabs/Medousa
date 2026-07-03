# Package medousa_local into a release tarball.
param(
    [string]$Target = "",
    [string]$Input = "",
    [string]$DistDir = "",
    [ValidateSet("auto", "metal", "cuda", "cpu")]
    [string]$Backend = "auto",
    [switch]$Help
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

function Show-Usage {
    @"
Usage: scripts/release/package-local-brain.ps1 [options]

Options:
  -Target <triple>   Rust target triple (default: host)
  -Input <dir>       Staging dir from build-local-brain.ps1
  -DistDir <dir>     Output directory (default: dist/)
  -Backend <mode>    auto|metal|cuda|cpu (for archive naming only)
  -Help              Show this help
"@
}

foreach ($arg in $args) {
    switch ($arg) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
        "--target" { throw "--target requires a value; use -Target on PowerShell" }
        "--input" { throw "--input requires a value; use -Input on PowerShell" }
        "--dist" { throw "--dist requires a value; use -DistDir on PowerShell" }
        "--backend" { throw "--backend requires a value; use -Backend on PowerShell" }
    }
}

if ($Help) { Show-Usage; exit 0 }

Assert-MedousaCommand tar
Push-Location $MEDOUSA_ROOT
try {
    $version = Get-MedousaVersion
    if (-not $Target) { $Target = Get-MedousaHostTarget }
    if (-not $Input) { $Input = Join-Path $MEDOUSA_ROOT "dist\build-local-brain\$Target" }
    if (-not $DistDir) { $DistDir = Join-Path $MEDOUSA_ROOT "dist" }

    $binDir = Join-Path $Input "bin"
    if (-not (Test-Path -LiteralPath $binDir)) {
        throw "missing $binDir"
    }

    $archiveName = "medousa_local-${Backend}-v${version}-${Target}.tar.gz"
    $archivePath = Join-Path $DistDir $archiveName
    $basename = "medousa_local-${Backend}-v${version}-${Target}"

    $work = Join-Path $DistDir ".pack-work-$basename"
    if (Test-Path -LiteralPath $work) { Remove-Item -Recurse -Force $work }
    $stageBin = Join-Path $work "$basename\bin"
    New-Item -ItemType Directory -Force -Path $stageBin | Out-Null
    Copy-Item -Recurse -Force "$binDir\*" $stageBin

    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
    Invoke-MedousaTarGz -ArchivePath $archivePath -WorkDir $work -Basename $basename
    Remove-Item -Recurse -Force $work

    $hash = Get-MedousaSha256File $archivePath
    Update-MedousaChecksumsFile -DistDir $DistDir -ArchiveName $archiveName -Hash $hash
    Write-MedousaLog "wrote $archivePath"
} finally {
    Pop-Location
}
