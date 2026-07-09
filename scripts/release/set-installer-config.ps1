# Write apps/medousa-installer/public/installer-config.json from env (baked into the installer at build).
# Usage:
#   $env:MEDOUSA_RELEASE_BASE_URL = "https://releases.entasislabs.com/medousa"
#   .\scripts\release\set-installer-config.ps1
#   cd apps\medousa-installer; npm run tauri:build

$ErrorActionPreference = "Stop"

. "$PSScriptRoot\common.ps1"

$baseUrl = if ($env:MEDOUSA_RELEASE_BASE_URL) { $env:MEDOUSA_RELEASE_BASE_URL.TrimEnd('/') } else { "" }
$channel = if ($env:MEDOUSA_RELEASE_CHANNEL) { $env:MEDOUSA_RELEASE_CHANNEL } else { "stable" }

$out = Join-Path $MEDOUSA_ROOT "apps\medousa-installer\public\installer-config.json"

if (-not $baseUrl) {
    Write-Warning "MEDOUSA_RELEASE_BASE_URL is empty — installer will fall back to GitHub Releases (often 404 in dev)"
}

$config = @{
    releaseBaseUrl = $baseUrl
    releaseChannel = $channel
    bootstrapPath    = "installer-bootstrap.json"
    manifestPath     = "release-manifest.json"
} | ConvertTo-Json -Depth 3

Set-MedousaUtf8Content -Path $out -Content $config
Write-MedousaLog "wrote $out (base=$(if ($baseUrl) { $baseUrl } else { '<empty>' }) channel=$channel)"
