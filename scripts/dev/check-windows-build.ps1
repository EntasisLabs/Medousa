# Verify Windows dev prerequisites for building Medousa (engine + Tauri app).
# Medousa release and desktop builds target x86_64-pc-windows-msvc (Visual Studio linker).

[CmdletBinding()]
param(
    [switch]$FixToolchain
)

$ErrorActionPreference = "Stop"

function Write-Check([string]$Label, [bool]$Ok, [string]$Detail) {
    $mark = if ($Ok) { "[ok]" } else { "[!!]" }
    Write-Host "$mark $Label"
    if ($Detail) { Write-Host "    $Detail" }
}

function Find-LinkExe {
    $candidates = @()

    $linkCmd = Get-Command link.exe -ErrorAction SilentlyContinue
    if ($linkCmd) { $candidates += $linkCmd.Source }

    $vcvarsPaths = @(
        "${env:ProgramFiles}\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
    )
    foreach ($vcvars in $vcvarsPaths) {
        if (-not (Test-Path $vcvars)) { continue }
        $installRoot = Split-Path (Split-Path (Split-Path (Split-Path $vcvars))) -Parent
        $link = Get-ChildItem -Path (Join-Path $installRoot "VC\Tools\MSVC") -Filter link.exe -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\HostX64\\x64\\link\.exe$' } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($link) { $candidates += $link.FullName }
    }

    return ($candidates | Select-Object -First 1)
}

$failures = 0

Write-Host ""
Write-Host "Medousa Windows build environment check"
Write-Host "======================================="
Write-Host ""

# Rust
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Check "Rust toolchain" $false "rustc not on PATH - install from https://rustup.rs"
    $failures++
} else {
    $hostTarget = (& rustc -vV | Select-String -Pattern '^host: ').Line.Substring(6).Trim()
    $activeToolchain = (& rustup show active-toolchain 2>$null) -replace '\s+\(.*$', ''
    $msvcHost = $hostTarget -eq "x86_64-pc-windows-msvc"

    if (-not $msvcHost -and $FixToolchain) {
        Write-Host "    switching default toolchain to stable-x86_64-pc-windows-msvc ..."
        rustup default stable-x86_64-pc-windows-msvc | Out-Null
        rustup target add x86_64-pc-windows-msvc | Out-Null
        $hostTarget = (& rustc -vV | Select-String -Pattern '^host: ').Line.Substring(6).Trim()
        $activeToolchain = (& rustup show active-toolchain 2>$null) -replace '\s+\(.*$', ''
        $msvcHost = $hostTarget -eq "x86_64-pc-windows-msvc"
    }

    Write-Check "Rust host target (MSVC)" $msvcHost "host=$hostTarget toolchain=$activeToolchain"
    if (-not $msvcHost) {
        Write-Host "    Medousa Windows builds require the MSVC Rust host, not GNU/MinGW."
        Write-Host "    Fix: rustup default stable-x86_64-pc-windows-msvc"
        Write-Host "    (GNU toolchain needs MinGW dlltool.exe and is not used for releases.)"
        $failures++
    }
}

# MSVC linker
$linkExe = Find-LinkExe
if (-not $linkExe) {
    $linkCmd = Get-Command link.exe -ErrorAction SilentlyContinue
    if ($linkCmd) { $linkExe = $linkCmd.Source }
}

$hasLinker = [bool]$linkExe
Write-Check "MSVC linker (link.exe)" $hasLinker $(if ($linkExe) { $linkExe } else { "not found" })
if (-not $hasLinker) {
    Write-Host "    Install Visual Studio 2022 Build Tools with the Desktop development with C++ workload:"
    Write-Host '    winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"'
    $failures++
} else {
    Write-Host "    Tip: run builds via .\scripts\dev\cargo-msvc.ps1 so LIB/PATH include the Windows SDK."
}

# Node (Tauri frontend)
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Check "Node.js" $false "required for apps/medousa-home (npm install / tauri build)"
    $failures++
} else {
    $nodeV = node --version
    Write-Check "Node.js" $true $nodeV
}

# WebView2 (runtime - usually preinstalled on Win10+)
$webviewKey = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
$hasWebView2 = Test-Path $webviewKey
Write-Check "WebView2 runtime" $hasWebView2 $(if ($hasWebView2) { "installed" } else { "install from https://developer.microsoft.com/microsoft-edge/webview2/" })

# Cargo cache location (same layout as Linux/Mac: sibling .cache next to repo)
$repoRoot = if (Test-Path (Join-Path $PWD "Cargo.toml")) { (Resolve-Path $PWD).Path } else { $null }
if ($repoRoot) {
    $cargoTarget = (Resolve-Path (Join-Path $repoRoot "..\.cache\cargo-target") -ErrorAction SilentlyContinue).Path
    if ($cargoTarget) {
        $drive = $cargoTarget.Substring(0, 1)
        $freeGb = [math]::Round((Get-PSDrive $drive).Free / 1GB, 1)
        Write-Check "Cargo target dir" $true "$cargoTarget (${freeGb} GB free on ${drive}:)"
        if ($freeGb -lt 30) {
            Write-Host "    Warning: debug builds need ~30GB+ free on the drive holding .cache/cargo-target."
        }
    }
}

Write-Host ""
if ($failures -eq 0) {
    Write-Host "All required checks passed. Try:"
    Write-Host "  .\scripts\dev\cargo-msvc.ps1 build --bin medousa_daemon"
    Write-Host "  cd apps\medousa-home; npm install; npm run tauri:build"
    exit 0
}

Write-Host "$failures required check(s) failed. Fix the items above, then re-run this script."
if (-not $FixToolchain) {
    Write-Host "Tip: re-run with -FixToolchain to switch rustup default to MSVC automatically."
}
exit 1
