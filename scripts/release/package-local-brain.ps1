# Package medousa_local into a release tarball.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

$Target = ""
$InputDir = ""
$DistDir = ""
$Backend = "auto"

function Show-Usage {
    Write-Host @'
Usage: scripts/release/package-local-brain.ps1 [options]

Options:
  --target <triple>   Rust target triple (default: host)
  --input <dir>       Staging dir from build-local-brain.ps1
  --dist <dir>        Output directory (default: dist/)
  --backend <mode>    auto|metal|cuda|cpu (for archive naming only)
  -h, --help          Show this help
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
        "--backend" {
            $i++
            if ($i -ge $args.Count) { throw "--backend requires a value" }
            $Backend = $args[$i]
        }
        default { throw "error: unknown argument: $($args[$i])" }
    }
}

Assert-MedousaCommand tar
Push-Location $MEDOUSA_ROOT
try {
    $version = Get-MedousaVersion
    if (-not $Target) { $Target = Get-MedousaHostTarget }
    if (-not $InputDir) { $InputDir = Join-Path $MEDOUSA_ROOT "dist\build-local-brain\$Target" }
    if (-not $DistDir) { $DistDir = Join-Path $MEDOUSA_ROOT "dist" }

    $binDir = Join-Path $InputDir "bin"
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
