# Package a single Medousa component (engine, adapter, cli, etc.) into a release tarball.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

$PackageId = ""
$Target = ""
$InputDir = ""
$DistDir = ""

function Show-Usage {
    Write-Host @'
Usage: scripts/release/package-component.ps1 [options]

Options:
  --package <id>      Component package id (engine, cli, adapter-telegram, ...)
  --target <triple>   Rust target triple (default: host)
  --input <dir>       Staging dir from build.ps1 (contains bin/)
  --dist <dir>        Output directory (default: dist/)
  -h, --help          Show this help

Creates dist/<package>-vX.Y.Z-<target>.tar.gz and appends to dist/SHA256SUMS.
'@
}

for ($i = 0; $i -lt $args.Count; $i++) {
    switch ($args[$i]) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
        "--package" {
            $i++
            if ($i -ge $args.Count) { throw "--package requires a value" }
            $PackageId = $args[$i]
        }
        "--target" {
            $i++
            if ($i -ge $args.Count) { throw "--target requires a value" }
            $Target = $args[$i]
        }
        "--input" {
            $i++
            if ($i -ge $args.Count) { throw "--input requires a value" }
            $InputDir = $args[$i]
        }
        "--dist" {
            $i++
            if ($i -ge $args.Count) { throw "--dist requires a value" }
            $DistDir = $args[$i]
        }
        default { throw "error: unknown argument: $($args[$i])" }
    }
}

if (-not $PackageId) { throw "error: --package is required" }

Assert-MedousaCommand tar
Push-Location $MEDOUSA_ROOT
try {
    Assert-MedousaVersionsMatch
    $version = Get-MedousaVersion

    if (-not $Target) {
        if ($InputDir) {
            $meta = Import-MedousaBuildMetaEnv $InputDir
            if ($meta.MEDOUSA_TARGET) { $Target = $meta.MEDOUSA_TARGET }
        }
        if (-not $Target) { $Target = Get-MedousaHostTarget }
    }

    if (-not $InputDir) { $InputDir = Join-Path $MEDOUSA_ROOT "dist\build\$Target" }
    if (-not $DistDir) { $DistDir = Join-Path $MEDOUSA_ROOT "dist" }

    $binDir = Join-Path $InputDir "bin"
    if (-not (Test-Path -LiteralPath $binDir)) {
        throw "missing bin directory: $binDir (run build.ps1 first)"
    }

    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

    $archiveName = Get-MedousaComponentArchiveName $PackageId $version $Target
    $archivePath = Join-Path $DistDir $archiveName
    $basename = Get-MedousaComponentBasename $PackageId $version $Target

    Write-MedousaLog "packaging component $PackageId -> $archiveName"

    $work = Join-Path $DistDir ".pack-work-$basename"
    if (Test-Path -LiteralPath $work) { Remove-Item -Recurse -Force $work }
    $stageBin = Join-Path $work "$basename\bin"
    New-Item -ItemType Directory -Force -Path $stageBin | Out-Null

    $componentBins = Get-MedousaComponentBinaries $PackageId
    foreach ($bin in $componentBins) {
        $file = Get-MedousaBinaryFilename $bin $Target
        $src = Join-Path $binDir $file
        if (-not (Test-Path -LiteralPath $src)) {
            throw "missing binary for ${PackageId}: $src"
        }
        Copy-Item -Force $src (Join-Path $stageBin $file)
    }

    $manifestPath = Join-Path $work "$basename\install-manifest.json"
    Write-MedousaComponentInstallManifest -BinDir $stageBin -PackageId $PackageId -Version $version -Target $Target -OutPath $manifestPath

    Invoke-MedousaTarGz -ArchivePath $archivePath -WorkDir $work -Basename $basename
    Remove-Item -Recurse -Force $work

    $hash = Get-MedousaSha256File $archivePath
    Write-MedousaLog "sha256: $hash"
    Update-MedousaChecksumsFile -DistDir $DistDir -ArchiveName $archiveName -Hash $hash
    Write-MedousaLog "wrote $archivePath"
} finally {
    Pop-Location
}
