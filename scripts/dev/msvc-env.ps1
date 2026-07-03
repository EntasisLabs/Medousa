# Shared Visual Studio MSVC + Medousa cargo cache environment for Windows builds.

function Get-MedousaRepoRoot {
    param([string]$StartDir = $PWD)

    $dir = (Resolve-Path $StartDir).Path
    while ($dir) {
        if (Test-Path (Join-Path $dir "Cargo.toml")) {
            return $dir
        }
        $parent = Split-Path $dir -Parent
        if ($parent -eq $dir) { break }
        $dir = $parent
    }
    throw "Could not find Medousa repo root (Cargo.toml) from $StartDir"
}

function Test-VcvarsScript([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return $false }
    $dir = Split-Path -Parent $Path
    return Test-Path -LiteralPath (Join-Path $dir "vcvarsall.bat")
}

function Get-Vcvars64Candidates {
    return @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat",
        "${env:ProgramFiles}\Microsoft Visual Studio\18\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
    )
}

function Resolve-Vcvars64Path {
    $vcvars = Get-Vcvars64Candidates | Where-Object { Test-VcvarsScript $_ } | Select-Object -First 1
    if (-not $vcvars) {
        throw @"
Could not find vcvars64.bat. Install Visual Studio 2022 Build Tools with the C++ workload:
  winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
"@
    }
    return $vcvars
}

function Import-VcvarsEnvironment([string]$VcvarsPath) {
    $envDump = Join-Path ([System.IO.Path]::GetTempPath()) ("medousa-vcvars-" + [Guid]::NewGuid().ToString("N") + ".env")
    try {
        # Use the system temp dir for the env dump (before we redirect TMP/TEMP to .cache).
        # Avoid piping cmd output; that can corrupt $LASTEXITCODE on Windows PowerShell.
        $cmdLine = 'call "' + $VcvarsPath + '" >nul 2>&1 && set > "' + $envDump + '"'
        cmd.exe /c $cmdLine
        if ($LASTEXITCODE -ne 0) {
            throw "vcvars64.bat failed (exit $LASTEXITCODE): $VcvarsPath"
        }
        if (-not (Test-Path -LiteralPath $envDump)) {
            throw "vcvars64.bat did not produce environment dump: $VcvarsPath"
        }
        Get-Content -LiteralPath $envDump | ForEach-Object {
            $idx = $_.IndexOf("=")
            if ($idx -lt 1) { return }
            $name = $_.Substring(0, $idx)
            $value = $_.Substring($idx + 1)
            Set-Item -Path "Env:$name" -Value $value
        }
    }
    finally {
        Remove-Item -LiteralPath $envDump -Force -ErrorAction SilentlyContinue
    }
}

function Set-MedousaCargoCacheEnvironment([string]$RepoRoot) {
    $cacheRoot = Join-Path (Split-Path $RepoRoot -Parent) ".cache"
    $cargoTarget = Join-Path $cacheRoot "cargo-target"
    $tmpDir = Join-Path $cacheRoot "tmp"
    New-Item -ItemType Directory -Force -Path $cargoTarget, $tmpDir | Out-Null

    $env:CARGO_TARGET_DIR = $cargoTarget
    $env:MEDOUSA_CARGO_TARGET_DIR = $cargoTarget
    $env:TMP = $tmpDir
    $env:TEMP = $tmpDir

    return @{
        CargoTarget = $cargoTarget
        TmpDir = $tmpDir
    }
}

function Import-MedousaMsvcBuildEnvironment {
    param([string]$RepoRoot)

    $vcvars = Resolve-Vcvars64Path
    Import-VcvarsEnvironment $vcvars
    $cache = Set-MedousaCargoCacheEnvironment $RepoRoot

    Write-Host "cargo target: $($cache.CargoTarget)"
    Write-Host "rustc temp:   $($cache.TmpDir)"
    Write-Host "vcvars:       $vcvars"

    return $vcvars
}
