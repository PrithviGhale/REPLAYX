# Capture headless screenshots of the mocked UI preview (no visible window).
# Requires the preview server: npx vite --config dev/vite.preview.ts
param([string]$Out = "$PSScriptRoot\screens", [string]$Pages = 'home,replay,capture,audio,clips,performance,settings', [string]$State = 'replay', [string]$Size = '1280,820')
# Chrome, not Edge: a new Edge profile signs in with the Windows account and
# syncs the user's extensions and data into it (seen 2026-11-08).
$edge = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
New-Item -ItemType Directory -Force $Out | Out-Null
foreach ($page in $Pages.Split(',')) {
    $file = Join-Path $Out "$page-$State.png"
    # Fresh profile each time so a rebuilt preview is never served from cache.
    $profile = Join-Path $env:TEMP "replayx-preview-edge-$([guid]::NewGuid())"
    & $edge --headless=new --disable-gpu --hide-scrollbars --disable-extensions --disable-sync --no-first-run --force-prefers-reduced-motion --user-data-dir=$profile --window-size=$Size `
        --timeout=12000 --screenshot=$file "http://127.0.0.1:1430/dev/preview.html?state=$State#$page" 2>$null | Out-Null
    Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue
    if (Test-Path $file) { "saved $file" } else { "failed $page" }
}
