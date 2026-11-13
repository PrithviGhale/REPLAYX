# Runs pacing experiments with the silent motion fixture on screen.
# Only run on an authorized, free desktop. No audio, no microphone.
param([string]$Runs = 'ddagrab:round,ddagrab:old,gfx:round,gfx:none', [int]$Seconds = 20, [string]$Tag = (Get-Date -Format 'HHmmss'))
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Set-Location $root
foreach ($run in $Runs.Split(',')) {
    $source, $mode = $run.Split(':')
    $name = "m-$Tag-$source-$mode"
    $log = Join-Path $root "test-output/pacing/$name-fixture.json"
    # Start-Process ignores Set-Location: pass absolute paths and the working directory.
    $fixture = Start-Process python -WorkingDirectory $root -ArgumentList '-I', "`"$root\scripts\bench\motion-fixture.py`"", '--seconds', ($Seconds + 6), '--log', "`"$log`"" -PassThru
    Start-Sleep -Seconds 3
    python -I scripts/bench/bench-pacing.py --seconds $Seconds --mode $mode --source $source --encoder qsv --name $name | Out-Null
    $fixture.WaitForExit(($Seconds + 15) * 1000) | Out-Null
    if (!$fixture.HasExited) { Stop-Process -Id $fixture.Id -Force }
    $clip = Join-Path $root "test-output/pacing/$name/capture.mkv"
    if (Test-Path $clip) {
        python -I scripts/bench/motion-check.py $clip --fixture-log $log | Set-Content (Join-Path $root "test-output/pacing/$name/motion.json")
        $r = Get-Content (Join-Path $root "test-output/pacing/$name/motion.json") -Raw | ConvertFrom-Json
        '{0,-22} frames={1} repeated={2} ({3}%) skipped={4} longest_repeat={5} fixture_missed={6}' -f "$source/$mode", $r.frames_analysed, $r.repeated_frames, $r.repeat_pct, $r.skipped_ticks, $r.longest_repeat_run, $r.fixture_missed_ticks
    } else { "$source/$mode : no capture produced (see test-output/pacing/$name/ffmpeg.log)" }
    Start-Sleep -Seconds 2
}
