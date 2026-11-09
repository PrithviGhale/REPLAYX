# Installs the reviewed FFmpeg runtime pinned in tools/ffmpeg-lock.json.
#
#   setup-media.ps1                      download (if needed), verify SHA-256, extract
#   setup-media.ps1 -ArchivePath X.zip   use a local copy/mirror instead of downloading
#   setup-media.ps1 -RefreshTag TAG      re-pin to a dated FFmpeg-Builds release (review required)
#
# The lock always points at an immutable dated release ("autobuild-..."), never
# "latest". A checksum proves the file matches the lock, not that it is safe:
# scan and test any newly pinned runtime before using or bundling it.
param([string]$ArchivePath = '', [string]$RefreshTag = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$toolsDir = Join-Path $root 'tools'
$lockPath = Join-Path $toolsDir 'ffmpeg-lock.json'

if ($RefreshTag) {
    if ($RefreshTag -notmatch '^autobuild-\d{4}-\d{2}-\d{2}-\d{2}-\d{2}$') { throw 'Use a dated autobuild-YYYY-MM-DD-HH-MM tag, not latest.' }
    $release = Invoke-RestMethod "https://api.github.com/repos/FFmpeg-Builds/FFmpeg-Builds/releases/tags/$RefreshTag"
    $asset = $release.assets | Where-Object { $_.name -match '^ffmpeg-n9\.\d+(\.\d+)?-\d+-g[0-9a-f]+-win64-lgpl-shared-9\.\d+\.zip$' } | Select-Object -First 1
    if (!$asset -or !$asset.digest -or !$asset.digest.StartsWith('sha256:')) { throw 'No versioned 9.x LGPL shared asset with a SHA-256 digest in that release.' }
    $folder = $asset.name -replace '\.zip$', ''
    $version = ($asset.name -replace '^ffmpeg-', '') -replace '-win64.*$', ''
    [ordered]@{
        name = $asset.name; browser_download_url = $asset.browser_download_url; release_tag = $RefreshTag; ffmpeg_version = $version
        license = 'CHECK ffmpeg -L and -buildconf: expected LGPL-3.0-or-later, no --enable-gpl or --enable-nonfree'
        source = "https://github.com/FFmpeg/FFmpeg ; build scripts https://github.com/FFmpeg-Builds/FFmpeg-Builds"
        digest = $asset.digest; size = $asset.size; archive = $asset.name; runtime_dir = "ffmpeg-stable/$folder/bin"
        reviewed = 'NOT REVIEWED: run a Defender scan, the full media test suite and update this field before use.'
    } | ConvertTo-Json | Set-Content -LiteralPath $lockPath -Encoding UTF8
    Write-Warning "Lock re-pinned to $($asset.name). Review provenance, scan, run media tests, then rebuild REPLAYX."
}

if (!(Test-Path -LiteralPath $lockPath)) { throw 'Reviewed media lock is required.' }
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
if ($lock.browser_download_url -match '/download/latest/') { throw 'The lock points at a floating "latest" asset. Re-pin with -RefreshTag.' }
$archive = Join-Path $toolsDir $lock.archive
if ($ArchivePath) {
    Copy-Item -LiteralPath $ArchivePath -Destination $archive -Force
} elseif (!(Test-Path -LiteralPath $archive)) {
    # GitHub's download servers occasionally answer 5xx; retry with backoff
    # (a CI run once failed on a single 504). The checksum below still decides.
    $delays = @(0, 5, 15, 30, 60)
    for ($attempt = 1; $attempt -le $delays.Count; $attempt++) {
        Start-Sleep -Seconds $delays[$attempt - 1]
        try {
            Invoke-WebRequest -Uri $lock.browser_download_url -OutFile "$archive.partial" -UseBasicParsing
            Move-Item -LiteralPath "$archive.partial" -Destination $archive -Force
            break
        } catch {
            Remove-Item -LiteralPath "$archive.partial" -ErrorAction SilentlyContinue
            if ($attempt -eq $delays.Count) {
                throw "Could not download $($lock.name) after $attempt attempts: $($_.Exception.Message). If the release is unavailable, obtain the same file (SHA-256 $($lock.digest)) and pass -ArchivePath."
            }
            Write-Warning "Download attempt $attempt failed ($($_.Exception.Message)); retrying."
        }
    }
}
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ('sha256:' + $hash -ne $lock.digest) {
    # Never keep (or cache) a file that doesn't match the lock.
    Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
    throw "FFmpeg checksum mismatch ($hash). The file was deleted; do not run it. Review the upstream release."
}
Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $toolsDir 'ffmpeg-stable') -Force
$bin = Join-Path $toolsDir $lock.runtime_dir
if (!(Test-Path (Join-Path $bin 'ffmpeg.exe'))) { throw "Archive did not contain $($lock.runtime_dir)." }
Write-Host "FFmpeg $($lock.ffmpeg_version) verified against the pinned SHA-256 and extracted to $bin."
