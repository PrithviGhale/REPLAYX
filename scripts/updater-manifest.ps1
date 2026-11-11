# Writes latest.json for the in-app updater next to the signed NSIS installer:
# the version, the release's highlights, and for 64-bit Windows the download
# address on GitHub and the installer's signature. REPLAYX reads it from
# https://github.com/<repo>/releases/latest/download/latest.json .
# Skips (with a notice) when no update signature was produced.
param([Parameter(Mandatory)][string]$Repository, [Parameter(Mandatory)][string]$Tag)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dir = Join-Path $root 'src-tauri/target/release/bundle/nsis'
$version = (Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
if ("v$version" -ne $Tag) { throw "Tag $Tag does not match package version $version" }
$installer = Get-ChildItem $dir -Filter "*_${version}_x64-setup.exe" | Select-Object -First 1
if (!$installer) { throw "No installer for version $version in $dir" }
$sig = "$($installer.FullName).sig"
if (!(Test-Path $sig)) { Write-Host 'No update signature (signing key not configured); latest.json not written.'; exit 0 }

# Notes for the update pop-up: the bullet points under "What's New" and
# "Improvements and Fixes" of the release notes, else the changelog section.
$notes = $null
$releaseNotes = Join-Path $root "docs/release-notes/$Tag.md"
if (Test-Path $releaseNotes) {
    $take = $false
    $lines = foreach ($line in Get-Content $releaseNotes) {
        if ($line -match '^## ') { $take = $line -match "^## (What's New|Improvements and Fixes)\s*$"; if ($take) { $line; '' }; continue }
        if ($take -and $line.Trim()) { $line }
    }
    if ($lines) { $notes = ($lines -join "`n").Trim() }
}
if (!$notes) {
    $section = (Get-Content (Join-Path $root 'CHANGELOG.md') -Raw) -split '(?m)^## ' | Where-Object { $_ -like "$version*" } | Select-Object -First 1
    $notes = if ($section) { ($section -replace "^$([regex]::Escape($version))[^\n]*\n", '').Trim() } else { "REPLAYX $version" }
}
[ordered]@{
    version   = $version
    notes     = $notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = (Get-Content $sig -Raw).Trim()
            url       = "https://github.com/$Repository/releases/download/$Tag/$($installer.Name)"
        }
    }
} | ConvertTo-Json -Depth 5 | ForEach-Object {
    # UTF-8 without a byte-order mark: JSON parsers reject a BOM.
    [IO.File]::WriteAllText((Join-Path $dir 'latest.json'), $_, (New-Object Text.UTF8Encoding $false))
}
Write-Host "Wrote latest.json for $($installer.Name)"
