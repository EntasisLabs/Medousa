# Package all component tarballs plus the full-suite medousa-v* archive.

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\common.ps1"

$Target = ""
$InputDir = ""
$DistDir = ""

function Show-Usage {
    Write-Host @'
Usage: scripts/release/package-all-components.ps1 [options]

Options:
  --target <triple>   Rust target triple (default: host)
  --input <dir>       Staging dir from build.ps1
  --dist <dir>        Output directory (default: dist/)
  -h, --help          Show this help

Packages engine, cli, each adapter, mcp-gateway, and the full medousa-v* suite tarball.
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

$commonArgs = @()
if ($Target) { $commonArgs += @("--target", $Target) }
if ($InputDir) { $commonArgs += @("--input", $InputDir) }
if ($DistDir) { $commonArgs += @("--dist", $DistDir) }

foreach ($packageId in $MedousaComponentIds) {
    & "$PSScriptRoot\package-component.ps1" --package $packageId @commonArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

& "$PSScriptRoot\package.ps1" @commonArgs
exit $LASTEXITCODE
