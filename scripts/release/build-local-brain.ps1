# Build medousa_local for a single Rust target.
param(
    [string]$Target = "",
    [string]$Output = "",
    [ValidateSet("auto", "metal", "cuda", "cpu")]
    [string]$Backend = "auto"
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

if (-not $Target) { $Target = Get-MedousaHostTarget }
if (-not $Output) { $Output = Join-Path $MEDOUSA_ROOT "dist\build-local-brain\$Target" }

function Resolve-InferenceFeature {
    param([string]$Backend, [string]$Target)
    switch ($Backend) {
        "metal" { return "embedded-inference-metal" }
        "cuda"  { return "embedded-inference-cuda" }
        "cpu"   { return "embedded-inference" }
        "auto" {
            if ($Target -like "*-apple-*") { return "embedded-inference-metal" }
            return "embedded-inference"
        }
        default { throw "unknown backend $Backend" }
    }
}

$feature = Resolve-InferenceFeature -Backend $Backend -Target $Target
$binDir = Join-Path $Output "bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

Write-MedousaLog "building medousa_local ($feature) for $Target"
Push-Location $MEDOUSA_ROOT
try {
    & cargo build --release -p medousa --bin medousa_local --features $feature --target $Target
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

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
"@ | Set-Content -Encoding utf8NoBOM (Join-Path $Output "build-meta.env")

Write-MedousaLog "done — $dst"
