# Run cargo with the Visual Studio 2022 MSVC environment (link.exe + Windows SDK libs).
# Also pins build artifacts and rustc temp files to the repo cache on D: (or wherever
# the repo lives) so Windows builds do not fill C:\Users\...\AppData\Local\Temp.

[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "msvc-env.ps1")

$repoRoot = Get-MedousaRepoRoot
if ($CargoArgs.Count -eq 0) {
    $CargoArgs = @("build")
}

Import-MedousaMsvcBuildEnvironment -RepoRoot $repoRoot | Out-Null

Push-Location $repoRoot
try {
    & cargo @CargoArgs
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
