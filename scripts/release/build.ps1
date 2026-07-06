# Build all Medousa release binaries for one Rust target into a staging directory.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

$Target = ""
$Output = ""
$PrintTargetOnly = $false
$WithLocalBrain = $true
$WithIroh = $true

function Show-Usage {
    Write-Host @'
Usage: scripts/release/build.ps1 [options]

Options:
  --target <triple>     Rust target triple (default: host)
  --output <dir>        Staging directory (default: dist/build/<target>)
  --print-target        Print resolved target triple and exit
  --with-local-brain    Also build medousa_local into <output>/bin/ (default: on)
  --without-local-brain Skip medousa_local (mistralrs) build
  --without-iroh        Omit iroh-transport (LAN-only pairing)
  --with-iroh           Include iroh-transport (default)
  -h, --help            Show this help

Builds all release binaries into <output>/bin/:
  medousa, medousa_cli, medousa_daemon, medousa_tui, channel adapters, medousa_mcp_gateway, medousa_whatsapp

By default also builds medousa_local (offline brain) into the same <output>/bin/ and packages
a separate medousa_local-*.tar.gz. Use --without-local-brain to skip the slow mistralrs build.

Iroh gateway is on at runtime when built with iroh-transport (opt out with MEDOUSA_IROH=0).
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
        "--print-target" { $PrintTargetOnly = $true }
        "--with-local-brain" { $WithLocalBrain = $true }
        "--without-local-brain" { $WithLocalBrain = $false }
        "--without-iroh" { $WithIroh = $false }
        "--with-iroh" { $WithIroh = $true }
        default { throw "error: unknown argument: $($args[$i])" }
    }
}

Assert-MedousaCommand cargo
Assert-MedousaCommand rustc

if (-not $Target) { $Target = Get-MedousaHostTarget }
if ($PrintTargetOnly) { Write-Output $Target; exit 0 }
if (-not $Output) { $Output = Join-Path $MEDOUSA_ROOT "dist\build\$Target" }

$binDir = Join-Path $Output "bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

Assert-MedousaVersionsMatch
$version = Get-MedousaVersion

Write-MedousaLog "building medousa v$version for $Target"
Write-MedousaLog "staging -> $binDir"
Write-MedousaLog "phase 1/2: building CLI + daemon + channels ($($MedousaBinaries.Count) binaries)..."
Write-MedousaLog "  bins: $($MedousaBinaries -join ' ')"

$cargoBuildArgs = @("build", "--release")
$cargoFeatures = @()
if ($WithIroh) {
    $cargoFeatures += "iroh-transport"
    Write-MedousaLog "iroh transport enabled (default - runtime opt-out: MEDOUSA_IROH=0)"
}
if ($cargoFeatures.Count -gt 0) {
    $cargoBuildArgs += @("--features", ($cargoFeatures -join ","))
}
if ($Target) {
    $cargoBuildArgs += @("--target", $Target)
}

Write-MedousaLog "phase 1/2: cargo build (root workspace, release)..."
Invoke-MedousaCargo @cargoBuildArgs `
    --bin medousa `
    --bin medousa_cli `
    --bin medousa_daemon `
    --bin medousa_tui `
    --bin medousa_telegram `
    --bin medousa_discord `
    --bin medousa_slack `
    --bin medousa_mcp_gateway

Write-MedousaLog "cargo build (medousa_whatsapp)..."
$waBuildArgs = @("build", "--release", "--manifest-path", $MEDOUSA_WHATSAPP_MANIFEST)
if ($Target) { $waBuildArgs += @("--target", $Target) }
Invoke-MedousaCargo @waBuildArgs

$mainRelease = Get-MedousaCargoReleaseDir $Target
$waRelease = Get-MedousaWhatsappCargoReleaseDir $Target

Write-MedousaLog "phase 1/2: staging release binaries -> $binDir"
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
MEDOUSA_WITH_IROH=$([int]$WithIroh)
MEDOUSA_WITH_LOCAL_BRAIN=$([int]$WithLocalBrain)
"@ | Set-MedousaUtf8Content -Path (Join-Path $Output "build-meta.env")

$stagedCount = (Get-ChildItem -File $binDir).Count
Write-MedousaLog "phase 1/2 complete - $stagedCount binaries in $binDir"

if ($WithLocalBrain) {
    $brainStaging = Join-Path $MEDOUSA_ROOT "dist\build-local-brain\$Target"
    Write-MedousaLog "phase 2/2: building medousa_local offline brain (mistralrs - slow, separate from daemon)..."
    & "$PSScriptRoot\build-local-brain.ps1" --target $Target --output $brainStaging
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $brainSrc = Join-Path $brainStaging "bin\$(Get-MedousaBinaryFilename medousa_local $Target)"
    if (-not (Test-Path -LiteralPath $brainSrc)) {
        throw "medousa_local missing after build-local-brain: $brainSrc"
    }
    Copy-Item -Force $brainSrc (Join-Path $binDir (Get-MedousaBinaryFilename medousa_local $Target))
    Write-MedousaLog "  medousa_local -> $binDir"
    & "$PSScriptRoot\package-local-brain.ps1" --target $Target --input $brainStaging
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-MedousaLog "phase 2/2 complete - medousa_local in $binDir + separate brain tarball in dist/"
} else {
    Write-MedousaLog "skipping phase 2 (medousa_local) - pass --with-local-brain or omit --without-local-brain"
}

$finalCount = (Get-ChildItem -File $binDir).Count
Write-MedousaLog "done - $finalCount binaries in $binDir"
