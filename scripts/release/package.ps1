# Package a build staging directory into a release tarball and SHA256SUMS entry.
param(
    [string]$Target = "",
    [string]$Input = "",
    [string]$DistDir = "",
    [switch]$Help
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

function Show-Usage {
    @"
Usage: scripts/release/package.ps1 [options]

Options:
  -Target <triple>   Rust target triple (default: host, or from build-meta.env)
  -Input <dir>       Staging dir from build.ps1 (contains bin/ and build-meta.env)
  -DistDir <dir>     Output directory for archives (default: dist/)
  -Help              Show this help
"@
}

foreach ($arg in $args) {
    switch ($arg) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
        "--target" { throw "--target requires a value; use -Target on PowerShell" }
        "--input" { throw "--input requires a value; use -Input on PowerShell" }
        "--dist" { throw "--dist requires a value; use -DistDir on PowerShell" }
    }
}

if ($Help) { Show-Usage; exit 0 }

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

    $archiveName = Get-MedousaAssetArchiveName $version $Target
    $archivePath = Join-Path $DistDir $archiveName
    $basename = Get-MedousaAssetBasename $version $Target

    Write-MedousaLog "packaging $archiveName"

    $work = Join-Path $DistDir ".pack-work-$basename"
    if (Test-Path -LiteralPath $work) { Remove-Item -Recurse -Force $work }
    $stageBin = Join-Path $work "$basename\bin"
    New-Item -ItemType Directory -Force -Path $stageBin | Out-Null
    Copy-Item -Recurse -Force "$binDir\*" $stageBin

    $manifestPath = Join-Path $work "$basename\install-manifest.json"
    Write-MedousaInstallManifest -BinDir $stageBin -Version $version -Target $Target -OutPath $manifestPath
    $setId = Get-MedousaReadManifestField $manifestPath "component_set_id"
    Write-MedousaLog "install-manifest component_set_id=$setId"

    Invoke-MedousaTarGz -ArchivePath $archivePath -WorkDir $work -Basename $basename
    Remove-Item -Recurse -Force $work

    $hash = Get-MedousaSha256File $archivePath
    Write-MedousaLog "sha256: $hash"
    Update-MedousaChecksumsFile -DistDir $DistDir -ArchiveName $archiveName -Hash $hash

    Write-MedousaLog "wrote $archivePath"
    Write-MedousaLog "updated $(Join-Path $DistDir 'SHA256SUMS')"
} finally {
    Pop-Location
}
