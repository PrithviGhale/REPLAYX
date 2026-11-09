# Builds the FFmpeg corresponding-source package that every public REPLAYX
# release must carry (see THIRD_PARTY_NOTICES.md and docs/RELEASING.md):
#   1. FFmpeg source at the pinned commit + FFmpeg-Builds recipes (../ffmpeg-source.ps1)
#   2. a download plan read from the recipes as text (never executed)
#   3. every library's source at its pinned commit/tag/revision (git archive)
#   4. licence files and SOURCE-README.txt
#   5. one .tar + its SHA-256
# Output: $Out\ffmpeg-corresponding-source-<ffmpeg version>.tar
# Needs git (with git-svn, included in Git for Windows) and Python 3.
param([string]$Out = "$PSScriptRoot\..\..\release\ffmpeg-source", [string]$Work = "$env:TEMP\replayx-ffmpeg-source-work")
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
$lock = Get-Content "$root\tools\ffmpeg-lock.json" -Raw | ConvertFrom-Json
$py = if (Get-Command python -ErrorAction SilentlyContinue) { 'python' } else { 'py' }

& "$root\scripts\ffmpeg-source.ps1" -Out $Out
if ($LASTEXITCODE) { throw 'FFmpeg source download failed' }
$recipes = Join-Path $Work 'ffmpeg-builds'
if (Test-Path $recipes) { Remove-Item -Recurse -Force $recipes }
New-Item -ItemType Directory -Force $recipes | Out-Null
$archive = Get-ChildItem $Out -Filter 'FFmpeg-Builds-*.tar.gz' | Select-Object -First 1
# Windows' own bsdtar: Git's GNU tar (if first on PATH) reads C:\ paths as host names.
& "$env:SystemRoot\System32\tar.exe" -xzf $archive.FullName -C $recipes --strip-components=1
if ($LASTEXITCODE) { throw 'Could not unpack the FFmpeg-Builds recipes' }

$plan = Join-Path $Work 'plan.json'
& $py -I "$PSScriptRoot\dl_plan.py" $recipes $plan
if ($LASTEXITCODE) { throw 'Could not read the recipes' }
& $py -I "$PSScriptRoot\fetch_sources.py" $plan (Join-Path $Out 'deps') (Join-Path $Work 'repos')
if ($LASTEXITCODE) { throw 'Some library sources could not be fetched (see above)' }
& $py -I "$PSScriptRoot\collect_licenses.py" $plan $Out
if ($LASTEXITCODE) { throw 'Could not collect licence files' }
$tar = Join-Path $Out "ffmpeg-corresponding-source-$($lock.ffmpeg_version).tar"
& $py -I "$PSScriptRoot\pack_source.py" $Out $tar | Tee-Object -Variable packed
if ($LASTEXITCODE) { throw 'Could not pack the source archive' }
"$($packed.Split(' ')[0])  $(Split-Path $tar -Leaf)" | Set-Content (Join-Path $Out 'SOURCE-SHA256.txt') -Encoding ascii
Write-Host "Corresponding source ready: $tar"
