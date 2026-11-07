param([switch]$Refresh)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$toolsDir = Join-Path $root 'tools'
$lockPath = Join-Path $toolsDir 'ffmpeg-lock.json'
if ($Refresh) {
    $release = Invoke-RestMethod 'https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest'
    $asset = $release.assets | Where-Object name -EQ 'ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip'
    if (!$asset -or !$asset.digest.StartsWith('sha256:')) { throw 'Stable LGPL asset/digest missing' }
    @{ name=$asset.name; browser_download_url=$asset.browser_download_url; digest=$asset.digest; archive='ffmpeg-stable.zip'; runtime_dir='ffmpeg-stable/ffmpeg-n9.0-latest-win64-lgpl-shared-9.0/bin' } | ConvertTo-Json | Set-Content -LiteralPath $lockPath -Encoding UTF8
    Write-Warning 'Media lock changed. Review provenance and run Defender plus all media tests before use.'
}
if (!(Test-Path -LiteralPath $lockPath)) { throw 'Reviewed media lock is required.' }
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
$archive = Join-Path $toolsDir $lock.archive
if (!(Test-Path -LiteralPath $archive)) { Invoke-WebRequest -Uri $lock.browser_download_url -OutFile $archive }
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ('sha256:' + $hash -ne $lock.digest) { throw 'FFmpeg checksum mismatch. Do not run the downloaded runtime. Review the upstream release before refreshing the lock.' }
Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $toolsDir 'ffmpeg-stable') -Force
Write-Host 'Archive matches the pinned SHA-256. Scan the extracted runtime before executing or bundling it.'
