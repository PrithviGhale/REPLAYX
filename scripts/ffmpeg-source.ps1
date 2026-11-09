# Prepare the FFmpeg corresponding-source package that must accompany a public
# release of REPLAYX's bundled LGPL FFmpeg build (see THIRD_PARTY_NOTICES.md):
# the exact FFmpeg source snapshot and the FFmpeg-Builds recipes (which pin
# every bundled library's source) at the tag recorded in tools/ffmpeg-lock.json.
# Output: release/ffmpeg-source/ with the archives and SHA256SUMS.txt.
# Downloads only source archives; installs nothing.
param([string]$Out = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (!$Out) { $Out = Join-Path (Join-Path $root 'release') 'ffmpeg-source' }
$lock = Get-Content (Join-Path (Join-Path $root 'tools') 'ffmpeg-lock.json') -Raw | ConvertFrom-Json
$commit = 'b204e94e90'
$buildsTag = 'autobuild-2026-11-06-12-50'
New-Item -ItemType Directory -Force $Out | Out-Null
$items = @(
  # git.ffmpeg.org answers scripts with a bot check; GitHub hosts the official mirror.
  @{ Name = "ffmpeg-$commit.tar.gz"; Url = "https://github.com/FFmpeg/FFmpeg/archive/$commit.tar.gz" },
  @{ Name = "FFmpeg-Builds-$buildsTag.tar.gz"; Url = "https://github.com/FFmpeg-Builds/FFmpeg-Builds/archive/refs/tags/$buildsTag.tar.gz" }
)
foreach ($i in $items) {
  $dest = Join-Path $Out $i.Name
  if (-not (Test-Path $dest)) {
    Write-Host "Downloading $($i.Url)"
    Invoke-WebRequest -Uri $i.Url -OutFile "$dest.part" -UseBasicParsing
    # Must be a real gzip archive (not an HTML error or bot-check page).
    $magic = [System.IO.File]::ReadAllBytes("$dest.part")[0..1]
    if ($magic[0] -ne 0x1f -or $magic[1] -ne 0x8b) { Remove-Item "$dest.part"; throw "$($i.Url) did not return a gzip archive" }
    Move-Item "$dest.part" $dest
  }
}
@"
FFmpeg corresponding source for REPLAYX releases
FFmpeg commit: $commit (n9.0.2-47-gb204e94e90), LGPL-3.0-or-later build (no --enable-gpl / --enable-nonfree)
Binary build: FFmpeg-Builds tag $buildsTag (win64 LGPL shared), recorded in tools/ffmpeg-lock.json
The FFmpeg-Builds archive contains the scripts that fetch and build every bundled library at pinned versions.
The configure line of the shipped binaries is in media/BUILD-CONFIGURATION.txt inside the installation.
"@ | Set-Content (Join-Path $Out 'README.txt') -Encoding utf8
Get-ChildItem $Out -File -Filter *.tar.gz | ForEach-Object { "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower())  $($_.Name)" } |
  Set-Content (Join-Path $Out 'SHA256SUMS.txt') -Encoding ascii
Get-ChildItem $Out | Select-Object Name, Length
