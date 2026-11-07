$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$mediaLock = Get-Content (Join-Path $root 'tools/ffmpeg-lock.json') -Raw | ConvertFrom-Json
$runtimeDir = Join-Path (Join-Path $root 'tools') $mediaLock.runtime_dir
$ffmpeg = Get-Item -LiteralPath (Join-Path $runtimeDir 'ffmpeg.exe') -ErrorAction SilentlyContinue
if (!$ffmpeg) { throw 'Run scripts/setup-media.ps1 first.' }
$ffprobe = Join-Path $ffmpeg.Directory.FullName 'ffprobe.exe'
# Fail before copying resources if Windows blocks either runtime executable.
& $ffmpeg.FullName -version | Out-Null
if ($LASTEXITCODE) { throw 'FFmpeg preflight failed; no bundle prepared.' }
& $ffprobe -version | Out-Null
if ($LASTEXITCODE) { throw 'FFprobe preflight failed; no bundle prepared.' }
$mediaDir = Join-Path $root 'src-tauri/media'
# Remove only the generated bundle resource directory, preventing old DLLs from mixing.
$resolvedRoot = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
$resolvedMedia = [IO.Path]::GetFullPath($mediaDir)
if (!$resolvedMedia.StartsWith($resolvedRoot,[StringComparison]::OrdinalIgnoreCase)) { throw 'Media directory escapes workspace' }
if (Test-Path -LiteralPath $resolvedMedia) { Remove-Item -LiteralPath $resolvedMedia -Recurse -Force }
New-Item -ItemType Directory -Force $mediaDir | Out-Null
Copy-Item -Path (Join-Path $ffmpeg.Directory.FullName '*') -Destination $mediaDir -Force
$packageRoot = $ffmpeg.Directory.Parent.FullName
Get-ChildItem -LiteralPath $packageRoot -File | Copy-Item -Destination $mediaDir -Force
Copy-Item -LiteralPath (Join-Path $root 'THIRD_PARTY_NOTICES.md') -Destination $mediaDir -Force
Copy-Item -LiteralPath (Join-Path $root 'tools/ffmpeg-lock.json') -Destination $mediaDir -Force
& $ffmpeg.FullName -version 2>&1 | Out-File (Join-Path $mediaDir 'BUILD-CONFIGURATION.txt') -Encoding utf8
if ($LASTEXITCODE) { throw 'Could not record FFmpeg build configuration' }
& $ffmpeg.FullName -hide_banner -L 2>&1 | Out-File (Join-Path $mediaDir 'FFMPEG-LICENSE.txt') -Encoding utf8
if ($LASTEXITCODE) { throw 'Could not record FFmpeg license' }
foreach ($notice in @('LICENSE','PRIVACY.md','THIRD_PARTY_NOTICES.md','KNOWN_LIMITATIONS.md','README.md','VALIDATION.md')) {
    Copy-Item -LiteralPath (Join-Path $root $notice) -Destination $mediaDir -Force
}
Write-Host 'Local installer media resources prepared. Read THIRD_PARTY_NOTICES.md before public redistribution.'
