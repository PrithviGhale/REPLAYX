param(
    [string]$CertificateThumbprint,
    [uri]$TimestampUrl,
    [string]$SignToolPath,
    [string]$SigningScriptPath,
    [switch]$OfflineWebView2,
    [switch]$UpdaterArtifacts
)
$ErrorActionPreference = 'Stop'
function Invoke-BuildTool([string]$Tool, [string[]]$Arguments) {
    # Windows PowerShell 5.1 turns redirected native stderr into ErrorRecords.
    # Cargo writes routine progress there. Use the process exit code for failure.
    $ErrorActionPreference = 'Continue'
    & $Tool @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Tool exited with code $LASTEXITCODE." }
}
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
$configPath = $null
$previousSignTool = $env:TAURI_WINDOWS_SIGNTOOL_PATH
try {
    $version = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
    if ($SigningScriptPath -and ($CertificateThumbprint -or $TimestampUrl)) {
        throw 'Choose either a signing script or a certificate thumbprint and timestamp URL.'
    }
    if ($UpdaterArtifacts -and $version -notmatch '-' -and !$CertificateThumbprint -and !$SigningScriptPath) {
        throw 'Stable updater artifacts require Windows Authenticode signing.'
    }
    $signScript = $null
    if ($SigningScriptPath) {
        $signScript = (Get-Item -LiteralPath $SigningScriptPath).FullName
        if ([IO.Path]::GetExtension($signScript) -ne '.ps1') { throw 'The signing adapter must be a PowerShell script.' }
        $shell = (Get-Command powershell.exe -ErrorAction Stop).Source
        $signingArgs = @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', $signScript)
    }
    if ($CertificateThumbprint -or $TimestampUrl) {
        if ($CertificateThumbprint -notmatch '^[a-fA-F0-9]{40}$' -or $TimestampUrl.Scheme -notin @('http', 'https')) {
            throw 'Provide both a 40-character certificate thumbprint and an RFC 3161 timestamp URL.'
        }
        if (!$SignToolPath) { throw "Provide the Windows SDK signtool.exe path to sign the bundled worker." }
        $signTool = (Get-Item -LiteralPath $SignToolPath).FullName
        $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
        if (!$certificate.HasPrivateKey -or $certificate.NotBefore -gt (Get-Date) -or $certificate.NotAfter -le (Get-Date)) {
            throw 'The configured signing certificate needs a usable private key and must be current.'
        }
        $usage = $certificate.Extensions | Where-Object { $_ -is [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension] }
        if ('1.3.6.1.5.5.7.3.3' -notin $usage.EnhancedKeyUsages.Value) {
            throw 'The certificate must explicitly allow code signing.'
        }
        $env:TAURI_WINDOWS_SIGNTOOL_PATH = $signTool
    }
    $stage = & (Join-Path $PSScriptRoot 'stage-rc-worker.ps1')
    $target = $stage.Target
    $worker = $stage.Worker
    $binaries = $stage.Directory
    Invoke-BuildTool cargo @('build', '--manifest-path', 'src-tauri/Cargo.toml', '--release', '--locked', '--bin', 'replayx-cli')
    $configArguments = @()
    $windowsConfig = @{}
    if ($OfflineWebView2) {
        $windowsConfig.webviewInstallMode = @{ type = 'offlineInstaller'; silent = $true }
    }
    if ($CertificateThumbprint) {
        & $signTool sign /sha1 $CertificateThumbprint /fd SHA256 /tr $TimestampUrl.AbsoluteUri /td SHA256 $worker
        if ($LASTEXITCODE) { throw "Native worker signing failed." }
        $windowsConfig += @{
            certificateThumbprint = $CertificateThumbprint
            digestAlgorithm = 'sha256'
            timestampUrl = $TimestampUrl.AbsoluteUri
            tsp = $true
        }
    }
    if ($signScript) {
        Invoke-BuildTool $shell ($signingArgs + @($worker))
        $windowsConfig.signCommand = @{ cmd = $shell; args = $signingArgs + @('%1') }
    }
    $bundleConfig = @{}
    if ($windowsConfig.Count) { $bundleConfig.windows = $windowsConfig }
    if ($UpdaterArtifacts) { $bundleConfig.createUpdaterArtifacts = $true }
    if ($bundleConfig.Count) {
        $config = @{ bundle = $bundleConfig } | ConvertTo-Json -Depth 5 -Compress
        $configPath = Join-Path $binaries ("bundle-options-" + [Guid]::NewGuid().ToString() + ".json")
        [IO.File]::WriteAllText($configPath, $config)
        $configArguments = @('--config', $configPath)
    }
    Invoke-BuildTool npm.cmd (@('run', 'tauri', '--', 'build', '--bundles', 'nsis', '--ci') + $configArguments + @('--', '--no-default-features', '--locked'))
    if ($CertificateThumbprint -or $signScript -or $UpdaterArtifacts) {
        $installers = @(Get-ChildItem -LiteralPath (Join-Path $target 'release/bundle/nsis') -File -Filter "*_${version}_x64-setup.exe")
        if ($installers.Count -ne 1) { throw "Expected exactly one NSIS installer for $version." }
        if ($CertificateThumbprint -or $signScript) {
            foreach ($file in @((Join-Path $target 'release/replayx.exe'), $worker, $installers[0].FullName)) {
                if ($CertificateThumbprint) { Invoke-BuildTool $signTool @('verify', '/pa', '/tw', $file) }
                $signature = Get-AuthenticodeSignature -LiteralPath $file
                if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate) {
                    throw "Missing valid timestamped Authenticode signature on $file."
                }
                if ($CertificateThumbprint -and $signature.SignerCertificate.Thumbprint -ne $CertificateThumbprint) {
                    throw "Unexpected Authenticode publisher on $file."
                }
            }
        }
        if ($UpdaterArtifacts) {
            $installer = $installers[0].FullName
            Invoke-BuildTool node @((Join-Path $PSScriptRoot 'verify-update-signature.mjs'), $installer, "$installer.sig", '--version', $version)
        }
    }
} finally {
    $env:TAURI_WINDOWS_SIGNTOOL_PATH = $previousSignTool
    if ($configPath) { Remove-Item -LiteralPath $configPath -ErrorAction SilentlyContinue }
    Pop-Location
}
