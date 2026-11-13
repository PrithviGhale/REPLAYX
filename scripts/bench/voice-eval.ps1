# Unattended Voice Enhancer evaluation. Never opens a microphone: speech is
# generated with Windows' built-in text-to-speech, then mixed with synthetic
# fan noise, keyboard clicks and broadband hiss at headset-like levels, and run
# through `replayx-cli voice-eval` (every preset and suppression strength).
# Output: test-output/voice-eval/<case>/*.wav plus results.txt.
param([string]$Out = "$PSScriptRoot\..\..\test-output\voice-eval")
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
$ffmpeg = Get-ChildItem "$root\tools\ffmpeg-stable" -Recurse -Filter ffmpeg.exe | Select-Object -First 1 -ExpandProperty FullName
$cli = "$root\src-tauri\target\release\replayx-cli.exe"
New-Item -ItemType Directory -Force $Out | Out-Null
$clean = Join-Path $Out 'clean.wav'

Add-Type -AssemblyName System.Speech
$tts = New-Object System.Speech.Synthesis.SpeechSynthesizer
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(48000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$tts.SetOutputToWaveFile("$Out\tts.wav", $fmt)
$tts.Speak("Okay, I'm pushing up the left side now. Watch the sniper on the roof. Peter Piper picked a peck of pickled peppers. She sells sea shells by the sea shore. Nice shot, that was close. Can you hear me clearly? Let's regroup by the bridge and try again.")
$tts.Dispose()
# Headset-like level: speech around -26 dBFS RMS.
& $ffmpeg -hide_banner -v error -y -i "$Out\tts.wav" -af "volume=-8dB" -ar 48000 -ac 1 $clean

$cases = @{
  'fan'      = 'anoisesrc=color=pink:amplitude=0.012:r=48000:seed=3'
  'hiss'     = 'anoisesrc=color=white:amplitude=0.004:r=48000:seed=5'
  # Broadband clicks of a few milliseconds, in bursts, like typing.
  'keyboard' = "aevalsrc='0.3*(2*random(0)-1)*exp(-500*mod(t\,0.17))*lt(mod(t\,0.17)\,0.012)*gt(sin(PI*t*0.9)\,-0.3)':s=48000"
}
$results = @()
foreach ($name in $cases.Keys) {
  $dir = Join-Path $Out $name
  New-Item -ItemType Directory -Force $dir | Out-Null
  $noisy = Join-Path $dir 'input.wav'
  & $ffmpeg -hide_banner -v error -y -i $clean -f lavfi -i $cases[$name] -filter_complex "[1:a]atrim=duration=20[n];[0:a][n]amix=inputs=2:duration=first:normalize=0" -ar 48000 -ac 1 $noisy
  $results += "== $name"
  $results += & $cli voice-eval $noisy $dir $clean
}
$results += "== clean speech (no noise)"
$results += & $cli voice-eval $clean (Join-Path $Out 'clean') $clean
$results | Tee-Object -FilePath (Join-Path $Out 'results.txt')
