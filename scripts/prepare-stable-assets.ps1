param(
    [Parameter(Mandatory)][string]$BundleDir,
    [Parameter(Mandatory)][string]$BaselineDir,
    [Parameter(Mandatory)][string]$FfmpegSource,
    [Parameter(Mandatory)][string]$Output,
    [switch]$AllowUnsigned040
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Split-Path $PSScriptRoot -Parent
$version = (Get-Content -LiteralPath (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
if ($version -ne '0.4.0') { throw 'Use the verified 0.4.0 release source.' }
if (Test-Path -LiteralPath $Output) { throw 'Choose a fresh output directory.' }
$bundle = (Resolve-Path -LiteralPath $BundleDir).Path
$setup = Join-Path $bundle 'REPLAYX_0.4.0_x64-setup.exe'
$manifest = Join-Path $bundle 'latest.json'
$sha = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash

$pins = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'installer-qa/rc3-pins.json') -Raw | ConvertFrom-Json
$baselines = [ordered]@{}
foreach ($pin in $pins.members | Where-Object name -NE 'Candidate-setup.exe') {
    $file = Get-Item -LiteralPath (Join-Path $BaselineDir $pin.name)
    if ($file.Length -ne $pin.bytes -or (Get-FileHash -LiteralPath $file.FullName).Hash -ne $pin.sha256) {
        throw "Unexpected QA baseline bytes for $($pin.name)."
    }
    $baselines[$pin.name] = $file.FullName
}

$source = (Get-Item -LiteralPath $FfmpegSource).FullName
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
if ($sourceHash -ne '41EC89FFCEF1F690E58C5DE25008021E167C50F4768DB94FE5C741ECB9DAA090') {
    throw 'Use the verified corresponding-source archive for the 0.4.0 runtime pin.'
}
& (Join-Path $PSScriptRoot 'updater-manifest.ps1') -Repository 'PrithviGhale/REPLAYX' -Tag 'v0.4.0' -BundleDir $bundle -RequireSignature -AllowUnsigned040:$AllowUnsigned040
if ((Get-FileHash -LiteralPath $setup).Hash -ne $sha) { throw 'Installer bytes changed during metadata generation.' }

$out = (New-Item -ItemType Directory -Path $Output).FullName
$release = (New-Item -ItemType Directory -Path (Join-Path $out 'release-assets')).FullName
$publicSetup = Join-Path $release 'REPLAYX.exe'
$publicSource = Join-Path $release 'FFmpeg-source.tar'
Copy-Item -LiteralPath $setup -Destination $publicSetup
Copy-Item -LiteralPath $source -Destination $publicSource
Copy-Item -LiteralPath $manifest -Destination $release
if ((Get-FileHash -LiteralPath $publicSetup).Hash -ne $sha -or (Get-FileHash -LiteralPath $publicSource).Hash -ne $sourceHash) {
    throw 'Release asset copy failed digest verification.'
}
@("$($sha.ToLowerInvariant())  REPLAYX.exe", "$($sourceHash.ToLowerInvariant())  FFmpeg-source.tar") |
    Set-Content -LiteralPath (Join-Path $out 'SHA256SUMS.txt') -Encoding ascii

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$archive = Join-Path $out 'REPLAYX-installer-qa-0.4.0.zip'
$zip = [IO.Compression.ZipFile]::Open($archive, [IO.Compression.ZipArchiveMode]::Create)
try {
    $files = [ordered]@{'Candidate-setup.exe' = $publicSetup; 'latest.json' = (Join-Path $release 'latest.json')}
    foreach ($entry in $baselines.GetEnumerator()) { $files[$entry.Key] = $entry.Value }
    foreach ($entry in $files.GetEnumerator()) {
        [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $entry.Value, $entry.Key, [IO.Compression.CompressionLevel]::NoCompression)
    }
} finally { $zip.Dispose() }
$archiveHash = (Get-FileHash -LiteralPath $archive).Hash
& (Join-Path $PSScriptRoot 'installer-qa/check.ps1') -Archive $archive -ArchiveSha256 $archiveHash -CandidateVersion '0.4.0' -Scenario 'clean' -Output (Join-Path $out 'offline-integrity') -IntegrityOnly -AllowUnsigned040:$AllowUnsigned040
[ordered]@{version = $version; installer_sha256 = $sha; ffmpeg_source_sha256 = $sourceHash; qa_zip_sha256 = $archiveHash} |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $out 'verified-digests.json') -Encoding utf8
