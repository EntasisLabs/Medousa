# Build all Medousa release binaries for one Rust target into a staging directory.
param(
    [string]$Target = "",
    [string]$Output = "",
    [switch]$PrintTargetOnly,
    [switch]$WithLocalBrain,
    [switch]$WithoutLocalBrain,
    [switch]$WithoutIroh,
    [switch]$WithIroh,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

function Show-Usage {
    @"
Usage: scripts/release/build.ps1 [options]

Options:
  -Target <triple>       Rust target triple (default: host)
  -Output <dir>          Staging directory (default: dist/build/<target>)
  -PrintTargetOnly       Print resolved target triple and exit
  -WithLocalBrain        Also build medousa_local into <output>/bin/ (default: on)
  -WithoutLocalBrain     Skip medousa_local build
  -WithoutIroh           Omit iroh-transport
  -WithIroh              Include iroh-transport (default)
  -Help                  Show this help
"@
}

foreach ($arg in $args) {
    switch ($arg) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
        "--target" { throw "--target requires a value; use -Target on PowerShell or node release-runner.mjs" }
        "--output" { throw "--output requires a value; use -Output on PowerShell" }
        "--print-target" { $PrintTargetOnly = $true }
        "--with-local-brain" { $WithLocalBrain = $true }
        "--without-local-brain" { $WithoutLocalBrain = $true }
        "--without-iroh" { $WithoutIroh = $true }
        "--with-iroh" { $WithIroh = $true }
    }
}

if ($Help) { Show-Usage; exit 0 }

Assert-MedousaCommand cargo
Assert-MedousaCommand rustc

if (-not $Target) { $Target = Get-MedousaHostTarget }
if ($PrintTargetOnly) { Write-Output $Target; exit 0 }
if (-not $Output) { $Output = Join-Path $MEDOUSA_ROOT "dist\build\$Target" }

$withLocalBrain = $true
if ($WithoutLocalBrain) { $withLocalBrain = $false }
if ($WithLocalBrain) { $withLocalBrain = $true }

$withIroh = $true
if ($WithoutIroh) { $withIroh = $false }
if ($WithIroh) { $withIroh = $true }

$binDir = Join-Path $Output "bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

Assert-MedousaVersionsMatch
$version = Get-MedousaVersion

Write-MedousaLog "building medousa v$version for $Target"
Write-MedousaLog "staging → $binDir"
Write-MedousaLog "phase 1/2: building CLI + daemon + channels ($($MedousaBinaries.Count) binaries)…"

$cargoBuildArgs = @("build", "--release")
$cargoFeatures = @()
if ($withIroh) {
    $cargoFeatures += "iroh-transport"
    Write-MedousaLog "iroh transport enabled (default — runtime opt-out: MEDOUSA_IROH=0)"
}
if ($cargoFeatures.Count -gt 0) {
    $cargoBuildArgs += @("--features", ($cargoFeatures -join ","))
}
if ($Target) {
    $cargoBuildArgs += @("--target", $Target)
}

Push-Location $MEDOUSA_ROOT
try {
    Write-MedousaLog "phase 1/2: cargo build (root workspace, release)…"
    & cargo @cargoBuildArgs `
        --bin medousa `
        --bin medousa_cli `
        --bin medousa_daemon `
        --bin medousa_tui `
        --bin medousa_telegram `
        --bin medousa_discord `
        --bin medousa_slack `
        --bin medousa_mcp_gateway
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-MedousaLog "cargo build (medousa_whatsapp)…"
    $waBuildArgs = @("build", "--release", "--manifest-path", $MEDOUSA_WHATSAPP_MANIFEST)
    if ($Target) { $waBuildArgs += @("--target", $Target) }
    & cargo @waBuildArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$mainRelease = Get-MedousaCargoReleaseDir $Target
$waRelease = Get-MedousaWhatsappCargoReleaseDir $Target

Write-MedousaLog "phase 1/2: staging release binaries → $binDir"
foreach ($bin in $MedousaBinaries) {
    if ($bin -eq "medousa_local") { continue }
    $src = Find-MedousaReleaseBinary -Bin $bin -Target $Target
    if (-not $src) {
        throw "expected binary missing: $bin (searched under $mainRelease and $waRelease)"
    }
    $dst = Join-Path $binDir (Get-MedousaBinaryFilename -Name $bin -Target $Target)
    Copy-Item -Force $src $dst
    Write-MedousaLog "  $bin"
}

@"
MEDOUSA_VERSION=$version
MEDOUSA_TARGET=$Target
MEDOUSA_BIN_DIR=$binDir
MEDOUSA_WITH_IROH=$([int]$withIroh)
MEDOUSA_WITH_LOCAL_BRAIN=$([int]$withLocalBrain)
"@ | Set-Content -Encoding utf8NoBOM (Join-Path $Output "build-meta.env")

$stagedCount = (Get-ChildItem -File $binDir).Count
Write-MedousaLog "phase 1/2 complete — $stagedCount binaries in $binDir"

if ($withLocalBrain) {
    $brainStaging = Join-Path $MEDOUSA_ROOT "dist\build-local-brain\$Target"
    Write-MedousaLog "phase 2/2: building medousa_local offline brain…"
    & "$PSScriptRoot\build-local-brain.ps1" -Target $Target -Output $brainStaging
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $brainSrc = Join-Path $brainStaging "bin\$(Get-MedousaBinaryFilename medousa_local $Target)"
    if (-not (Test-Path -LiteralPath $brainSrc)) {
        throw "medousa_local missing after build-local-brain: $brainSrc"
    }
    Copy-Item -Force $brainSrc (Join-Path $binDir (Get-MedousaBinaryFilename medousa_local $Target))
    Write-MedousaLog "  medousa_local → $binDir"
    & "$PSScriptRoot\package-local-brain.ps1" -Target $Target -Input $brainStaging
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-MedousaLog "phase 2/2 complete — medousa_local in $binDir + separate brain tarball in dist/"
} else {
    Write-MedousaLog "skipping phase 2 (medousa_local) — pass -WithLocalBrain or omit -WithoutLocalBrain"
}

$finalCount = (Get-ChildItem -File $binDir).Count
Write-MedousaLog "done — $finalCount binaries in $binDir"
