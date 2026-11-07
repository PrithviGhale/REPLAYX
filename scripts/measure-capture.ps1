# Runs `replayx-cli capture-test` and samples CPU time and private memory of
# replayx-cli plus its ffmpeg children once per second. Usage example:
#   powershell -ExecutionPolicy Bypass -File scripts/measure-capture.ps1 20 system stretch1080 fps=60
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$cli = Join-Path $root 'src-tauri/target/release/replayx-cli.exe'
$argList = @('capture-test') + $args
$out = [IO.Path]::GetTempFileName()
$p = Start-Process -FilePath $cli -ArgumentList $argList -WorkingDirectory $root -NoNewWindow -PassThru -RedirectStandardOutput $out
$null = $p.Handle
$cpu = @{}; $peakMem = 0.0; $samples = 0; $start = Get-Date
while (-not $p.HasExited) {
    $procs = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($p.Id) OR ProcessId=$($p.Id)")
    $mem = 0.0
    foreach ($w in $procs) {
        $gp = Get-Process -Id $w.ProcessId -ErrorAction SilentlyContinue
        if ($gp) { $cpu[$w.ProcessId] = $gp.TotalProcessorTime.TotalSeconds; $mem += $gp.PrivateMemorySize64 / 1MB }
    }
    $peakMem = [Math]::Max($peakMem, $mem); $samples++
    Start-Sleep -Milliseconds 1000
}
$wall = ((Get-Date) - $start).TotalSeconds
$total = ($cpu.Values | Measure-Object -Sum).Sum
$text = Get-Content $out -Raw
Remove-Item $out
if ($p.ExitCode -ne 0) { $text; exit 1 }
$json = $text.Substring($text.IndexOf("`n{") + 1) | ConvertFrom-Json
$s = $json.status
'exit={0} wall={1:N1}s cpu={2:N1}s avg={3:N2} cores ({4:N2}% of machine) peak_private_mem={5:N0}MB' -f $p.ExitCode, $wall, $total, ($total / $wall), (100 * $total / $wall / [Environment]::ProcessorCount), $peakMem
'encoder={0} scaling="{1}" output={2}x{3} frames={4} dropped={5} fps={6} audio_discontinuities={7} drift_ppm={8}' -f $s.encoder, $s.scaling, $s.output_width, $s.output_height, $s.frames, $s.dropped_frames, $s.fps, ($s.audio_discontinuities -join '/'), ($s.audio_drift_ppm -join '/')
if ($s.scaling_detail) { "scaling_detail: $($s.scaling_detail)" }
"clips: $($json.first) | $($json.concurrent)"
