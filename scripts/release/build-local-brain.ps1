# Build medousa_local (Offline brain) for one Rust target.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

$Target = ""
$Output = ""
$Backend = "auto"

function Show-Usage {
    Write-Host @'
Usage: scripts/release/build-local-brain.ps1 [options]

Options:
  --target <triple>     Rust target triple (default: host)
  --output <dir>        Staging directory (default: dist/build-local-brain/<target>)
  --backend auto|metal|cuda|cpu
  -h, --help            Show this help

Builds medousa_local with embedded inference into <output>/bin/.
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
        "--output" {
            $i++
            if ($i -ge $args.Count) { throw "--output requires a value" }
            $Output = $args[$i]
        }
        "--backend" {
            $i++
            if ($i -ge $args.Count) { throw "--backend requires a value" }
            $Backend = $args[$i]
        }
        default { throw "error: unknown argument: $($args[$i])" }
    }
}

function Resolve-InferenceFeature {
    param([string]$BackendName, [string]$TargetTriple)
    switch ($BackendName) {
        "metal" { return "embedded-inference-metal" }
        "cuda" { return "embedded-inference-cuda" }
        "cpu" { return "embedded-inference" }
        "auto" {
            if ($TargetTriple -like "*-apple-*") { return "embedded-inference-metal" }
            return "embedded-inference"
        }
        default { throw "error: unknown backend $BackendName" }
    }
}

if (-not $Target) { $Target = Get-MedousaHostTarget }
if (-not $Output) { $Output = Join-Path $MEDOUSA_ROOT "dist\build-local-brain\$Target" }

$feature = Resolve-InferenceFeature -BackendName $Backend -TargetTriple $Target
$binDir = Join-Path $Output "bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

Write-MedousaLog "building medousa_local ($feature) for $Target"
$localBuildArgs = @(
    "build", "--release", "-p", "medousa", "--bin", "medousa_local",
    "--features", $feature
)
if ($Target) { $localBuildArgs += @("--target", $Target) }
Invoke-MedousaCargo @localBuildArgs

$src = Find-MedousaReleaseBinary -Bin "medousa_local" -Target $Target
if (-not $src) { throw "release binary not found: medousa_local for $Target" }
$dst = Join-Path $binDir (Get-MedousaBinaryFilename -Name "medousa_local" -Target $Target)
Copy-Item -Force $src $dst

$version = Get-MedousaVersion
@"
MEDOUSA_VERSION=$version
MEDOUSA_TARGET=$Target
MEDOUSA_BACKEND=$Backend
MEDOUSA_BIN_DIR=$binDir
"@ | Set-MedousaUtf8Content -Path (Join-Path $Output "build-meta.env")

Write-MedousaLog "done - $dst"
