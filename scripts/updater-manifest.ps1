# Writes latest.json for the Tauri updater next to the signed NSIS installer.
# Skips (with a notice) when no updater signature was produced.
param([Parameter(Mandatory)][string]$Repository, [Parameter(Mandatory)][string]$Tag)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dir = Join-Path $root 'src-tauri/target/release/bundle/nsis'
$version = (Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
$installer = Get-ChildItem $dir -Filter "*_${version}_x64-setup.exe" | Select-Object -First 1
if (!$installer) { throw "No installer for version $version in $dir" }
$sig = "$($installer.FullName).sig"
if (!(Test-Path $sig)) { Write-Host 'No updater signature (signing secrets not configured); latest.json not written.'; exit 0 }
$notes = (Get-Content (Join-Path $root 'CHANGELOG.md') -Raw) -split '(?m)^## ' | Where-Object { $_ -like "$version*" } | Select-Object -First 1
[ordered]@{
    version   = $version
    notes     = if ($notes) { ($notes -replace "^$([regex]::Escape($version))[^\n]*\n", '').Trim() } else { "REPLAYX $version" }
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = (Get-Content $sig -Raw).Trim()
            url       = "https://github.com/$Repository/releases/download/$Tag/$($installer.Name)"
        }
    }
} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $dir 'latest.json') -Encoding UTF8
Write-Host "Wrote latest.json for $($installer.Name)"
