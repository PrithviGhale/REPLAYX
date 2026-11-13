# Benchmarks capture/scale/encode chains against the live desktop for a fixed
# duration and reports encoder-process CPU time, speed and dropped frames.
# Writes nothing but a null output. Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/bench/bench-scaling.ps1 [-Seconds 15] [-Fps 60]
param([int]$Seconds = 15, [int]$Fps = 60, [string]$Size = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$lock = Get-Content (Join-Path $root 'tools/ffmpeg-lock.json') -Raw | ConvertFrom-Json
$ffmpeg = Join-Path (Join-Path (Join-Path $root 'tools') $lock.runtime_dir) 'ffmpeg.exe'
if (-not $Size) {
    $d = & (Join-Path $root 'src-tauri/target/release/replayx-cli.exe') displays | ConvertFrom-Json
    $Size = "$($d[0].active_width)x$($d[0].active_height)"
}
$chains = [ordered]@{
    'cpu-scale + nvenc (previous fallback)' = @('hwdownload,format=bgra,scale=1920:1080:flags=bilinear,setsar=1,format=yuv420p', 'h264_nvenc')
    'qsv-vpp + nvenc (hybrid)'              = @('hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12,hwdownload,format=nv12,setsar=1', 'h264_nvenc')
    'qsv-vpp + qsv (zero-copy iGPU)'        = @('hwmap=derive_device=qsv,format=qsv,vpp_qsv=w=1920:h=1080:format=nv12,setsar=1', 'h264_qsv')
    'd3d11 scale + nvenc (dGPU display only)' = @('scale_d3d11=width=1920:height=1080:format=nv12,setsar=1', 'h264_nvenc')
}
foreach ($name in $chains.Keys) {
    $vf, $enc = $chains[$name]
    $log = [IO.Path]::GetTempFileName()
    $args = @('-hide_banner', '-nostats', '-v', 'error', '-progress', $log, '-f', 'lavfi', '-i',
        "ddagrab=output_idx=0:framerate=${Fps}:video_size=$Size", '-vf', $vf, '-c:v', $enc,
        '-b:v', '12M', '-g', "$($Fps*2)", '-bf', '0', '-fps_mode', 'cfr', '-t', "$Seconds", '-f', 'null', '-')
    $p = Start-Process -FilePath $ffmpeg -ArgumentList $args -NoNewWindow -PassThru
    $handle = $p.Handle
    $p.WaitForExit()
    $cpu = $p.TotalProcessorTime.TotalSeconds
    $progress = Get-Content $log -ErrorAction SilentlyContinue
    $last = @{}
    foreach ($line in $progress) { $k, $v = $line -split '=', 2; $last[$k] = $v }
    Remove-Item $log -ErrorAction SilentlyContinue
    $cores = [Environment]::ProcessorCount
    '{0,-42} exit={1} frames={2} drop={3} dup={4} speed={5} cpu={6:N2}s ({7:N1}% of one core, {8:N2}% of machine)' -f `
        $name, $p.ExitCode, $last['frame'], $last['drop_frames'], $last['dup_frames'], $last['speed'], $cpu, `
        (100 * $cpu / $Seconds), (100 * $cpu / $Seconds / $cores)
}
