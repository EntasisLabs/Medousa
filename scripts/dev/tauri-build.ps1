# Run `tauri` with the Visual Studio MSVC environment and Medousa cargo cache.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$AppDir,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgs
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "msvc-env.ps1")

$appDir = (Resolve-Path $AppDir).Path
$repoRoot = Get-MedousaRepoRoot -StartDir $appDir

Import-MedousaMsvcBuildEnvironment -RepoRoot $repoRoot | Out-Null

Push-Location $appDir
try {
    if ($TauriArgs.Count -eq 0) {
        & npx tauri build
    } else {
        & npx tauri @TauriArgs
    }
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
