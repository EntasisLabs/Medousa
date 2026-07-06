# Run `tauri build` with the Visual Studio MSVC environment and Medousa cargo cache.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$HomeDir,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgs
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "msvc-env.ps1")

$homeDir = (Resolve-Path $HomeDir).Path
$repoRoot = Get-MedousaRepoRoot -StartDir $homeDir

Import-MedousaMsvcBuildEnvironment -RepoRoot $repoRoot | Out-Null

Push-Location $homeDir
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
