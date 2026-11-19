# Run on an authorized Windows signing runner. Configuration is supplied by the runner.
param([Parameter(Mandatory, Position=0)][string]$Artifact)
$ErrorActionPreference = 'Stop'

$paths = @($env:REPLAYX_SIGNTOOL_PATH, $env:REPLAYX_ARTIFACT_SIGNING_DLIB, $env:REPLAYX_ARTIFACT_SIGNING_METADATA)
foreach ($path in $paths) {
    if (!$path -or !(Test-Path -LiteralPath $path -PathType Leaf)) {
        throw 'Configure SignTool, the Artifact Signing dlib and metadata on the signing runner.'
    }
}
$signTool = (Get-Item -LiteralPath $paths[0]).FullName
$artifactPath = (Get-Item -LiteralPath $Artifact).FullName
if ([IO.Path]::GetExtension($artifactPath) -ne '.exe') { throw 'The signing adapter expects a Windows executable.' }
& $signTool sign /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 /dlib $paths[1] /dmdf $paths[2] $artifactPath
if ($LASTEXITCODE -ne 0) { throw 'Microsoft Artifact Signing failed.' }
& $signTool verify /pa /tw $artifactPath
if ($LASTEXITCODE -ne 0) { throw 'Timestamped Authenticode verification failed.' }
