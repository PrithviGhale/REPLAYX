# Writes latest.json for the in-app updater next to the signed NSIS installer:
# the version, the release's highlights, and for 64-bit Windows the download
# address on GitHub and the installer's signature. REPLAYX reads it from
# the selected release tag's latest.json (stable and prerelease channels).
# Skips (with a notice) when no update signature was produced.
# -BundleDir: where the NSIS installer is (local builds into another target
# folder); the release workflow uses the default.
param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_][A-Za-z0-9_.-]*$')][string]$Repository,
    [Parameter(Mandatory)][string]$Tag,
    [string]$BundleDir,
    [switch]$RequireSignature,
    [switch]$AllowUnsigned040
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dir = if ($BundleDir) { $BundleDir } else { Join-Path $root 'src-tauri/target/release/bundle/nsis' }
$version = (Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
if ($AllowUnsigned040 -and ($version -cne '0.4.0' -or $Repository -cne 'PrithviGhale/REPLAYX')) {
    throw 'The unsigned exception applies only to PrithviGhale/REPLAYX 0.4.0.'
}
if ("v$version" -ne $Tag) { throw "Tag $Tag does not match package version $version" }
$installers = @(Get-ChildItem -LiteralPath $dir -File -Filter "*_${version}_x64-setup.exe")
if ($installers.Count -ne 1) { throw "Expected one installer for version $version in $dir. Found $($installers.Count)." }
$installer = $installers[0]
if ($installer.VersionInfo.ProductName -ne 'REPLAYX' -or $installer.VersionInfo.ProductVersion -ne $version) {
    throw 'The installer product or version does not match package.json.'
}
$sig = "$($installer.FullName).sig"
$manifestPath = Join-Path $dir 'latest.json'
if (!(Test-Path -LiteralPath $sig -PathType Leaf)) {
    if ($RequireSignature -or $AllowUnsigned040 -or (Test-Path -LiteralPath $manifestPath)) {
        throw 'Update signature is missing. Generate release metadata from a fresh signed bundle.'
    }
    Write-Host 'No update signature. latest.json was not written.'
    exit 0
}
if ($version -notmatch '-') {
    . (Join-Path $PSScriptRoot 'release-authenticode.ps1')
    Assert-ReleaseAuthenticode -Path $installer.FullName -Version $version -Role Installer -AllowUnsigned040:$AllowUnsigned040
}
$hash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash
$signature = (Get-Content -LiteralPath $sig -Raw).Trim()
& node (Join-Path $root 'scripts/verify-update-signature.mjs') $installer.FullName $sig --version $version
if ($LASTEXITCODE) { throw 'Installer signature/version validation failed. latest.json was not written.' }
if ($hash -ne (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash -or
    $signature -cne (Get-Content -LiteralPath $sig -Raw).Trim()) {
    throw 'Installer or signature changed during verification.'
}

# Notes for the update pop-up: the bullet points under "What's New" and
# "Improvements and Fixes" of the release notes, else the changelog section.
$notes = $null
$releaseNotes = Join-Path $root "docs/release-notes/$Tag.md"
if (Test-Path $releaseNotes) {
    $take = $false
    $lines = foreach ($line in Get-Content -LiteralPath $releaseNotes -Encoding utf8) {
        if ($line -match '^## ') { $take = $line -match "^## (What's New|Improvements and Fixes)\s*$"; if ($take) { $line; '' }; continue }
        if ($take -and $line.Trim()) { $line }
    }
    if ($lines) { $notes = ($lines -join "`n").Trim() }
}
if (!$notes) {
    $section = (Get-Content -LiteralPath (Join-Path $root 'CHANGELOG.md') -Raw -Encoding utf8) -split '(?m)^## ' | Where-Object { $_ -like "$version*" } | Select-Object -First 1
    $notes = if ($section) { ($section -replace "^$([regex]::Escape($version))[^\n]*\n", '').Trim() } else { "REPLAYX $version" }
}
[ordered]@{
    version   = $version
    notes     = $notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $signature
            url       = "https://github.com/$Repository/releases/download/$Tag/REPLAYX.exe"
        }
    }
} | ConvertTo-Json -Depth 5 | ForEach-Object {
    # UTF-8 without a byte-order mark: JSON parsers reject a BOM.
    [IO.File]::WriteAllText($manifestPath, $_, (New-Object Text.UTF8Encoding $false))
}
Write-Host "Wrote latest.json for $($installer.Name) (SHA-256 $hash)"
