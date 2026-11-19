param([switch]$Generated,[switch]$PhaseLock)
$ErrorActionPreference='Stop'
$launcher=Join-Path $PSScriptRoot 'desktop-native.mjs'
$launchArgs=@($launcher)
if ($Generated) { $launchArgs+='--generated' }
if ($PhaseLock) { $launchArgs+='--phase-lock' }
node @launchArgs
exit $LASTEXITCODE
