# Package a build staging directory into a release tarball and SHA256SUMS entry.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

$Target = ""
$InputDir = ""
$DistDir = ""

function Show-Usage {
    Write-Host @'
Usage: scripts/release/package.ps1 [options]

Options:
  --target <triple>   Rust target triple (default: host, or from build-meta.env)
  --input <dir>       Staging dir from build.ps1 (contains bin/ and build-meta.env)
  --dist <dir>        Output directory for archives (default: dist/)
  -h, --help          Show this help

Creates dist/medousa-vX.Y.Z-<target>.tar.gz and appends to dist/SHA256SUMS.
'@
}

for ($i = 0; $i -lt $args.Count; $i++) {
    switch ($args[$i]) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
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
