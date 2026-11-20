<#
.SYNOPSIS
  Capture one Windows Graphics Capture stall in an ETW trace (WPR), to find
  why the desktop compositor pauses for about 0.5 s while REPLAYX captures.

.DESCRIPTION
  Run ONLY from a PowerShell you opened yourself with "Run as administrator"
  (WPR's kernel trace needs it). This script never elevates itself.

  What it does, then stops:
    1. Starts WPR with the built-in CPU, GPU and DesktopComposition profiles
       in memory mode (circular buffers: only the most recent tens of seconds
       are kept, nothing is written to disk while it records).
    2. Runs replayx-gameproxy full screen (a silent GPU workload, Escape quits)
       and a REPLAYX test capture with Windows Graphics Capture (1080p60, H.264,
       no audio) - the conditions in which the stalls were measured.
    3. As soon as the workload reports a frame longer than 200 ms, waits
       4 s (the rest of the stall cluster) and saves the trace to
       test-output\wgc-stall-trace\<time>\trace.etl.
    4. If no stall happens within -Seconds, the trace is discarded (wpr -cancel).
  Whatever happens (an error, Ctrl+C), WPR is stopped and every process this
  script started is ended.

.PARAMETER Seconds
  Longest run, 60-300 (default 200; the stalls came about once a minute).

.EXAMPLE
  # In an administrator PowerShell, from the repository root:
  powershell -ExecutionPolicy Bypass -File scripts\bench\wgc-stall-trace.ps1
#>
param([ValidateRange(60, 300)][int]$Seconds = 200)
$ErrorActionPreference = 'Stop'

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Write-Host 'This needs an administrator PowerShell (WPR kernel tracing). Nothing was started.' -ForegroundColor Yellow
    exit 1
}

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$rel = Join-Path $root 'src-tauri\target\release'
$proxy = Join-Path $rel 'replayx-gameproxy.exe'
$cli = Join-Path $rel 'replayx-cli.exe'
foreach ($exe in $proxy, $cli) { if (-not (Test-Path $exe)) { throw "Missing $exe (build the release tools first)." } }
if ((Get-PSDrive (Split-Path -Qualifier $root).TrimEnd(':')).Free -lt 4GB) { throw 'Less than 4 GB free; the trace can take 1-2 GB.' }
if (Get-Process replayx -ErrorAction SilentlyContinue) { throw 'Quit the REPLAYX app first (one capture at a time).' }
$games = Get-Process FortniteClient-Win64-Shipping, cs2, VALORANT-Win64-Shipping -ErrorAction SilentlyContinue
if ($games) { throw "Close $($games[0].ProcessName) first: this run takes over the screen." }

$out = Join-Path $root ("test-output\wgc-stall-trace\" + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Force $out | Out-Null
$stalls = Join-Path $out 'stalls.txt'
$started = @()
$tracing = $false
try {
    Write-Host 'Starting WPR (CPU, GPU, DesktopComposition; memory mode)...'
    & wpr -start CPU -start GPU -start DesktopComposition | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "wpr -start failed ($LASTEXITCODE)" }
    $tracing = $true

    $started += Start-Process $proxy -PassThru -ArgumentList @(
        '--seconds', ($Seconds + 15), '--iters', '200', '--topmost', '1',
        '--log', (Join-Path $out 'gameproxy.json'), '--stall-log', $stalls)
    Start-Sleep -Seconds 4
    $started += Start-Process $cli -PassThru -WindowStyle Hidden -WorkingDirectory $out `
        -RedirectStandardOutput (Join-Path $out 'capture.log') -RedirectStandardError (Join-Path $out 'capture.err') `
        -ArgumentList @('capture-test', $Seconds, 'silent', 'stretch1080', 'capture_api=wgc', 'codec=h264')

    $deadline = (Get-Date).AddSeconds($Seconds)
    $seen = $null
    while ((Get-Date) -lt $deadline -and -not $started[1].HasExited) {
        if (-not $seen -and (Test-Path $stalls) -and (Get-Item $stalls).Length -gt 0) {
            $seen = Get-Date
            Write-Host 'Stall seen; recording 4 s more...'
        }
        if ($seen -and ((Get-Date) - $seen).TotalSeconds -ge 4) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($seen) {
        Write-Host 'Saving the trace (this can take a minute)...'
        & wpr -stop (Join-Path $out 'trace.etl') 'REPLAYX WGC stall' | Out-Host
        $tracing = $false
        Write-Host "Saved: $out" -ForegroundColor Green
    } else {
        Write-Host 'No stall in this run; the trace was discarded.' -ForegroundColor Yellow
    }
} finally {
    if ($tracing) { & wpr -cancel | Out-Null }
    foreach ($p in $started) { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    Get-CimInstance Win32_Process -Filter "Name='ffmpeg.exe'" |
        Where-Object { $_.CommandLine -like "*$out*" -or $_.ParentProcessId -in $started.Id } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}
