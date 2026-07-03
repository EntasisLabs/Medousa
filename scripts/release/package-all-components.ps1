# Package all component tarballs plus the full-suite medousa-v* archive.
param(
    [string]$Target = "",
    [string]$Input = "",
    [string]$DistDir = "",
    [switch]$Help
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

function Show-Usage {
    @"
Usage: scripts/release/package-all-components.ps1 [options]

Options:
  -Target <triple>   Rust target triple (default: host)
  -Input <dir>       Staging dir from build.ps1
  -DistDir <dir>     Output directory (default: dist/)
  -Help              Show this help
"@
}

foreach ($arg in $args) {
    switch ($arg) {
        { $_ -in @("-h", "--help") } { Show-Usage; exit 0 }
    }
}

if ($Help) { Show-Usage; exit 0 }

$commonArgs = @()
if ($Target) { $commonArgs += @("-Target", $Target) }
if ($Input) { $commonArgs += @("-Input", $Input) }
if ($DistDir) { $commonArgs += @("-DistDir", $DistDir) }

foreach ($packageId in $MedousaComponentIds) {
    & "$PSScriptRoot\package-component.ps1" -Package $packageId @commonArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

& "$PSScriptRoot\package.ps1" @commonArgs
exit $LASTEXITCODE
