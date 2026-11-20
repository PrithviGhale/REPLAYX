param(
    [Parameter(Mandatory)][string]$Candidate,
    [string]$UpgradeFrom,
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9_-]+$')][string]$Name
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$inputs = [ordered]@{'Candidate-setup.exe' = $Candidate}
if ($UpgradeFrom) { $inputs['Previous-setup.exe'] = $UpgradeFrom }
$entries = foreach ($pair in $inputs.GetEnumerator()) {
    $file = Get-Item -LiteralPath $pair.Value
    if ($file.PSIsContainer -or $file.Extension -ne '.exe' -or $file.VersionInfo.ProductName -notlike '*REPLAYX*') {
        throw "Expected a REPLAYX Windows setup executable: $($pair.Value)"
    }
    [pscustomobject]@{
        source = $file.FullName
        name = $pair.Key
        version = $file.VersionInfo.ProductVersion
        sha256 = (Get-FileHash -LiteralPath $file.FullName).Hash
        signature = (Get-AuthenticodeSignature -LiteralPath $file.FullName).Status.ToString()
    }
}
$kit = Join-Path $root "test-output\installer-qa\$Name"
if (Test-Path -LiteralPath $kit) { throw "Choose a fresh kit name. Existing kit preserved: $kit" }
$payload = New-Item -ItemType Directory -Path (Join-Path $kit 'payload')
foreach ($entry in $entries) {
    $destination = Join-Path $payload.FullName $entry.name
    Copy-Item -LiteralPath $entry.source -Destination $destination
    if ((Get-FileHash -LiteralPath $destination).Hash -ne $entry.sha256) { throw 'Setup copy failed verification.' }
}
ConvertTo-Json -InputObject @($entries) -ErrorAction Stop | Set-Content -LiteralPath (Join-Path $kit 'manifest.json') -Encoding utf8
$escaped = [Security.SecurityElement]::Escape($payload.FullName)
@"
<Configuration>
  <VGpu>Disable</VGpu>
  <Networking>Disable</Networking>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <PrinterRedirection>Disable</PrinterRedirection>
  <ProtectedClient>Enable</ProtectedClient>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$escaped</HostFolder>
      <SandboxFolder>C:\ReplayxQA</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
  </MappedFolders>
</Configuration>
"@ | Set-Content -LiteralPath (Join-Path $kit 'REPLAYX-QA.wsb') -Encoding utf8
Write-Output $kit
