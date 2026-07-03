# Desktop installer bundle (Windows / Linux / macOS). iOS remains Mac-only via build-full-package.sh --ios.
param(
    [switch]$Ios,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

function Show-Usage {
    @"
Usage: scripts/build-full-package.ps1 [-Ios]

Builds:
  1. medousa_daemon sidecar (slim catalog/scheduler)
  2. Optional medousa_local sidecar when prepare:sidecar is invoked with -WithLocalBrain
  3. Medousa desktop app (tauri build)

With -Ios (Mac + Xcode only):
  Runs ios-testflight-build.sh via bash (not supported on native Windows)

Desktop artifacts:
  src-tauri/target/release/bundle/  (or target/<triple>/release/bundle when cross-compiling)
"@
}

foreach ($arg in $args) {
    switch ($arg) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
        "--ios" { $Ios = $true }
    }
}

if ($Help) { Show-Usage; exit 0 }

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
Push-Location $Root
try {
    Write-Host "==> Medousa full package build"
    Write-Host "    app dir: $Root"
    Write-Host ""

    Write-Host "==> Engine sidecar + desktop app"
    & npm run tauri:build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host ""
    Write-Host "[ok] Desktop bundle ready under:"
    $bundleRoot = Join-Path $Root "src-tauri\target"
    if ($env:CARGO_BUILD_TARGET) {
        $bundleRoot = Join-Path $bundleRoot "$($env:CARGO_BUILD_TARGET)\release\bundle"
    } else {
        $bundleRoot = Join-Path $bundleRoot "release\bundle"
    }
    if (Test-Path -LiteralPath $bundleRoot) {
        Get-ChildItem -LiteralPath $bundleRoot -Recurse -Directory -ErrorAction SilentlyContinue |
            Select-Object -First 5 |
            ForEach-Object { Write-Host "  $($_.FullName)" }
    }

    if ($Ios) {
        if ($IsMacOS -or $env:OS -eq "Darwin") {
            Write-Host ""
            Write-Host "==> iOS TestFlight IPA"
            & bash (Join-Path $ScriptDir "ios-testflight-build.sh")
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } else {
            throw "iOS full package build requires macOS (use build-full-package.sh --ios on a Mac)"
        }
    }

    Write-Host ""
    Write-Host "[done] Full package build complete."
} finally {
    Pop-Location
}
