# Builds the FFmpeg corresponding-source package that every public REPLAYX
# release must carry (see THIRD_PARTY_NOTICES.md and docs/RELEASING.md):
#   1. FFmpeg source at the pinned commit + FFmpeg-Builds recipes (../ffmpeg-source.ps1)
#   2. a download plan read from the recipes as text (never executed)
#   3. every library's source at its pinned commit/tag/revision (git archive;
#      Subversion sources with `svn export`, or `git svn` where svn is missing)
#   4. licence files and SOURCE-README.txt
#   5. one .tar + its SHA-256
# Output: <Out>/ffmpeg-corresponding-source-<ffmpeg version>.tar
# Runs in Windows PowerShell or PowerShell 7 on Windows and Linux; needs git
# and Python 3 (and svn or git-svn for LAME).
param([string]$Out = '', [string]$Work = '')
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$root = (Resolve-Path (Join-Path $here (Join-Path '..' '..'))).Path
if (!$Out) { $Out = Join-Path (Join-Path $root 'release') 'ffmpeg-source' }
if (!$Work) { $Work = Join-Path ([IO.Path]::GetTempPath()) 'replayx-ffmpeg-source-work' }
New-Item -ItemType Directory -Force $Out, $Work | Out-Null
$Out = (Resolve-Path $Out).Path
$Work = (Resolve-Path $Work).Path
$lock = Get-Content (Join-Path (Join-Path $root 'tools') 'ffmpeg-lock.json') -Raw | ConvertFrom-Json
$py = @('python3', 'python', 'py') | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue } | Select-Object -First 1
if (!$py) { throw 'Python 3 is required' }
# Windows' own bsdtar: Git's GNU tar (if first on PATH) reads C:\ paths as host names.
$winTar = if ($env:SystemRoot) { Join-Path (Join-Path $env:SystemRoot 'System32') 'tar.exe' } else { '' }
$tar = if ($winTar -and (Test-Path $winTar)) { $winTar } else { 'tar' }

& (Join-Path (Join-Path $root 'scripts') 'ffmpeg-source.ps1') -Out $Out
if (!$?) { throw 'FFmpeg source download failed' }
$recipes = Join-Path $Work 'ffmpeg-builds'
if (Test-Path $recipes) { Remove-Item -Recurse -Force $recipes }
New-Item -ItemType Directory -Force $recipes | Out-Null
$archive = Get-ChildItem $Out -Filter 'FFmpeg-Builds-*.tar.gz' | Select-Object -First 1
& $tar -xzf $archive.FullName -C $recipes --strip-components=1
if ($LASTEXITCODE) { throw 'Could not unpack the FFmpeg-Builds recipes' }

$plan = Join-Path $Work 'plan.json'
& $py -I (Join-Path $here 'dl_plan.py') $recipes $plan
if ($LASTEXITCODE) { throw 'Could not read the recipes' }
& $py -I (Join-Path $here 'fetch_sources.py') $plan (Join-Path $Out 'deps') (Join-Path $Work 'repos')
if ($LASTEXITCODE) { throw 'Some library sources could not be fetched (see above)' }
& $py -I (Join-Path $here 'collect_licenses.py') $plan $Out
if ($LASTEXITCODE) { throw 'Could not collect licence files' }
$tarFile = Join-Path $Out "ffmpeg-corresponding-source-$($lock.ffmpeg_version).tar"
$packed = & $py -I (Join-Path $here 'pack_source.py') $Out $tarFile
if ($LASTEXITCODE) { throw 'Could not pack the source archive' }
Write-Host $packed
"$($packed.Split(' ')[0])  $(Split-Path $tarFile -Leaf)" | Set-Content (Join-Path $Out 'SOURCE-SHA256.txt') -Encoding ascii
Write-Host "Corresponding source ready: $tarFile"
