param(
    [Parameter(Mandatory)][string]$Archive,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ArchiveSha256,
    [Parameter(Mandatory)][ValidateSet('0.4.0-rc.3','0.4.0')][string]$CandidateVersion,
    [ValidateSet('clean','alpha4','alpha9','rc1','rc2')][string]$Scenario = 'clean',
    [Parameter(Mandatory)][string]$Output,
    [switch]$IntegrityOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (!$IntegrityOnly) {
    if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or !$env:RUNNER_TEMP) {
        throw 'Installation requires a disposable GitHub-hosted runner. Use -IntegrityOnly locally.'
    }
    $os = Get-CimInstance Win32_OperatingSystem
    $prefix = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\') + '\'
    if ($os.ProductType -eq 1 -or $os.Caption -notmatch 'Windows Server' -or
        ![IO.Path]::GetFullPath($Output).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Expected Windows Server and a work directory under RUNNER_TEMP.'
    }
}
if (Test-Path -LiteralPath $Output) { throw 'Choose a fresh output directory.' }
$work = (New-Item -ItemType Directory -Path $Output).FullName
$logs = (New-Item -ItemType Directory -Path (Join-Path $work 'logs')).FullName
$checks = [ordered]@{}
$evidence = [ordered]@{ scenario = $Scenario; candidate = $CandidateVersion; archive_sha256 = $ArchiveSha256; integrity_only = [bool]$IntegrityOnly; steps = $checks }
$pins = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rc3-pins.json') -Raw | ConvertFrom-Json
$phase = 'archive integrity'

function Assert-Hash([string]$Path, [string]$Expected) {
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    if ($actual -ne $Expected) { throw "SHA-256 mismatch for $Path (got $actual)." }
}

function Get-Payload {
    $file = (Resolve-Path -LiteralPath $Archive).Path
    Assert-Hash $file $ArchiveSha256
    $expected = @($pins.members.name)
    if ($CandidateVersion -eq '0.4.0') { $expected += 'latest.json' }
    $destination = (New-Item -ItemType Directory -Path (Join-Path $work 'input')).FullName
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($file)
    try {
        if ($zip.Entries.Count -ne $expected.Count) { throw 'Unexpected archive member count.' }
        foreach ($name in $expected) {
            $members = @($zip.Entries | Where-Object { $_.FullName -ceq $name })
            if ($members.Count -ne 1) { throw "Expected exactly one flat archive member named $name." }
            $member = $members[0]
            $pin = $pins.members | Where-Object name -CEQ $name
            $stableCandidate = $name -eq 'Candidate-setup.exe' -and $CandidateVersion -eq '0.4.0'
            $size = if ($name -eq 'latest.json') { 1048576 } elseif ($stableCandidate) { 536870912 } else { $pin.bytes }
            if ($member.Length -le 0 -or $member.Length -gt $size -or (!$stableCandidate -and $name -ne 'latest.json' -and $member.Length -ne $size)) {
                throw "Unexpected archive size for $name."
            }
            $path = Join-Path $destination $name
            [IO.Compression.ZipFileExtensions]::ExtractToFile($member, $path, $false)
            if ($member.Length -ne (Get-Item -LiteralPath $path).Length) { throw "Extraction size changed for $name." }
            if (!$stableCandidate -and $name -ne 'latest.json') { Assert-Hash $path $pin.sha256 }
            if ($name.EndsWith('.exe')) {
                $wanted = if ($stableCandidate) { $CandidateVersion } else { $pin.version }
                $info = (Get-Item -LiteralPath $path).VersionInfo
                if ($info.ProductName -ne 'REPLAYX' -or $info.ProductVersion -ne $wanted) {
                    throw "Unexpected product or version for $name. Expected REPLAYX $wanted."
                }
            }
        }
    } finally { $zip.Dispose() }
    return $destination
}

function Assert-Signed([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate) {
        throw "Expected a valid timestamped Authenticode signature on $Path. Got $($signature.Status)."
    }
}

function Invoke-CheckedExe([string]$Path, [string[]]$Arguments, [string]$Name, [int]$TimeoutSeconds) {
    $process = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logs "$Name.stdout.txt") -RedirectStandardError (Join-Path $logs "$Name.stderr.txt")
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $exitCode = $null
    try {
        if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill()
            throw "$Name exceeded the $TimeoutSeconds-second timeout."
        }
        $exitCode = $process.ExitCode
        if ($exitCode -ne 0) { throw "$Name exited with code $exitCode." }
    } finally {
        [ordered]@{ file = $Path; elapsed_ms = $clock.ElapsedMilliseconds; exit_code = $exitCode } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $logs "$Name.process.json")
        $process.Dispose()
    }
}

function Get-Registration {
    foreach ($hive in @('HKEY_CURRENT_USER','HKEY_LOCAL_MACHINE')) {
        foreach ($software in @('Software','Software\WOW6432Node')) {
            $key = "Registry::$hive\$software\Microsoft\Windows\CurrentVersion\Uninstall"
            if (!(Test-Path -LiteralPath $key)) { continue }
            foreach ($item in (Get-ItemProperty "$key\*")) {
                if ($item.PSObject.Properties['DisplayName'] -and $item.DisplayName -eq 'REPLAYX') {
                    [pscustomobject]@{ hive = $hive; key = $item.PSPath; version = $item.DisplayVersion;
                        location = $item.InstallLocation; uninstall = $item.UninstallString }
                }
            }
        }
    }
}

function Assert-Installed([string]$Version) {
    $entries = @(Get-Registration)
    ConvertTo-Json -InputObject $entries -Depth 4 | Set-Content -LiteralPath (Join-Path $logs "registration-$Version.json")
    if ($entries.Count -ne 1 -or $entries[0].hive -ne 'HKEY_CURRENT_USER' -or $entries[0].version -ne $Version) {
        throw "Expected one per-user REPLAYX registration at $Version."
    }
    if ($entries[0].location.Trim('"') -ne $installDir -or $entries[0].uninstall.Trim('"') -ne $uninstaller) {
        throw 'Unexpected install location or uninstaller path.'
    }
    if (!(Test-Path -LiteralPath $app -PathType Leaf) -or !(Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw 'Installed executable or uninstaller is missing.'
    }
    if ((Get-Item -LiteralPath $app).VersionInfo.ProductVersion -ne $Version) { throw 'Installed executable version mismatch.' }
}

function Assert-Fixture {
    $observed = Get-Content -LiteralPath $settingsPath -Raw -Encoding utf8 | ConvertFrom-Json
    foreach ($key in @('output_dir','replay_seconds','replay_hotkey')) {
        if ($observed.$key -ne $settings[$key]) { throw "Setting $key changed." }
    }
    if ($Scenario -in 'rc1','rc2') {
        if ($observed.recording_engine -ne $settings.recording_engine -or $observed.crosshair.color -ne $settings.crosshair.color) {
            throw 'The engine preference or Crosshair color changed.'
        }
    }
    Assert-Hash $savedClip $fixturePin.sha256
}

function Test-Startup([string]$Name) {
    if (![Environment]::UserInteractive -or (Get-Process -Id $PID).SessionId -eq 0) {
        $checks[$Name] = 'BLOCKED - runner lacks an interactive Windows session'
        return
    }
    $process = Start-Process -FilePath $app -WorkingDirectory $installDir -PassThru -RedirectStandardOutput (Join-Path $logs "$Name.stdout.txt") -RedirectStandardError (Join-Path $logs "$Name.stderr.txt")
    try {
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $handle = 0
        $webviews = @()
        while ($clock.Elapsed.TotalSeconds -lt 20) {
            $process.Refresh()
            if ($process.HasExited) { throw "$Name exited early with code $($process.ExitCode)." }
            $handle = $process.MainWindowHandle.ToInt64()
            $webviews = @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe' AND ParentProcessId=$($process.Id)" |
                Select-Object Name, ProcessId, ParentProcessId)
            if ($handle -ne 0 -and $webviews.Count -gt 0 -and $clock.Elapsed.TotalSeconds -ge 5) { break }
            Start-Sleep -Milliseconds 700
        }
        [ordered]@{ pid = $process.Id; elapsed_ms = $clock.ElapsedMilliseconds; main_window = $handle; webview_children = $webviews } |
            ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $logs "$Name.json")
        if ($handle -eq 0 -or $webviews.Count -eq 0) { throw "$Name did not produce a main window and WebView2 child." }
        $checks[$Name] = 'PASS - process, window and WebView2 child'
    } finally {
        if (!$process.HasExited) {
            [void]$process.CloseMainWindow()
            if (!$process.WaitForExit(3000)) {
                $checks["$Name exit"] = 'Owned test process stopped after window close did not exit'
                $process.Kill()
                [void]$process.WaitForExit(3000)
            }
        }
        $process.Dispose()
    }
}

try {
    $payload = Get-Payload
    $candidate = Join-Path $payload 'Candidate-setup.exe'
    $checks[$phase] = 'PASS'
    if ($CandidateVersion -eq '0.4.0') {
        $phase = 'stable signatures'
        Assert-Signed $candidate
        $feed = Get-Content -LiteralPath (Join-Path $payload 'latest.json') -Raw | ConvertFrom-Json
        $platform = $feed.platforms.'windows-x86_64'
        if ($feed.version -ne $CandidateVersion -or $platform.url -cne "https://github.com/PrithviGhale/REPLAYX/releases/download/v$CandidateVersion/REPLAYX.exe") {
            throw 'The signed update manifest has the wrong release version or installer URL.'
        }
        $sig = Join-Path $logs 'verified-updater.sig'
        Set-Content -LiteralPath $sig -Value $platform.signature -Encoding ascii
        & node (Join-Path $PSScriptRoot '../verify-update-signature.mjs') $candidate $sig --version $CandidateVersion
        if ($LASTEXITCODE -ne 0) { throw 'Stable updater signature verification failed.' }
        $checks[$phase] = 'PASS - final installer and version-bound manifest'
    }
    if ($IntegrityOnly) { return }

    $phase = 'runner isolation'
    $webviewKey = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $webview = @(foreach ($root in @('HKLM:\SOFTWARE\WOW6432Node','HKLM:\SOFTWARE','HKCU:\SOFTWARE')) {
        $key = Join-Path $root $webviewKey
        if (Test-Path -LiteralPath $key) {
            $item = Get-ItemProperty -LiteralPath $key
            if ($item.PSObject.Properties['pv']) { $item.pv }
        }
    })
    $evidence.host = [ordered]@{
        os = $os.Caption; build = $os.BuildNumber; interactive = [Environment]::UserInteractive
        image = $env:ImageVersion; preinstalled_webview2 = $webview
    }
    $installDir = Join-Path $env:LOCALAPPDATA 'REPLAYX'
    $data = Join-Path $env:LOCALAPPDATA 'io.replayx.desktop'
    $settingsPath = Join-Path $data 'settings.json'
    $app = Join-Path $installDir 'replayx.exe'
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    if ($env:REPLAYX_DATA_DIR -or $env:REPLAYX_NATIVE_ALPHA -or @(Get-Registration).Count -or
        (Test-Path -LiteralPath $installDir) -or (Test-Path -LiteralPath $data) -or
        (Test-Path -LiteralPath (Join-Path $env:APPDATA 'io.replayx.desktop')) -or
        (Get-Process -Name replayx -ErrorAction SilentlyContinue)) {
        throw 'Test requires a clean runner profile without installed REPLAYX or an app-data override.'
    }
    $checks[$phase] = 'PASS'

    $legacyPrograms = @('replayx-cli.exe','replayx-gameproxy.exe','replayx-dda-profile.exe','replayx-wgc-profile.exe')
    $expectedLegacyPrograms = if ($Scenario -eq 'alpha4') { $legacyPrograms[0..1] } elseif ($Scenario -eq 'alpha9') { $legacyPrograms }
    $baselines = @{ alpha4 = 'Alpha4-setup.exe'; alpha9 = 'Alpha9-setup.exe'; rc1 = 'RC1-setup.exe'; rc2 = 'RC2-setup.exe' }
    if ($Scenario -ne 'clean') {
        $phase = 'baseline install'
        $baseline = $baselines[$Scenario]
        Invoke-CheckedExe (Join-Path $payload $baseline) @('/S') $phase 120
        Assert-Installed ($pins.members | Where-Object name -EQ $baseline).version
        foreach ($legacy in $expectedLegacyPrograms) {
            if (!(Test-Path -LiteralPath (Join-Path $installDir $legacy) -PathType Leaf)) {
                throw "Expected the archived baseline to install $legacy."
            }
        }
        $checks[$phase] = 'PASS'
    }

    $phase = 'guest fixture'
    $clips = (New-Item -ItemType Directory -Path (Join-Path $work 'clips')).FullName
    $savedClip = Join-Path $clips 'Generated-sample.mkv'
    $fixturePin = $pins.members | Where-Object name -EQ 'Generated-sample.mkv'
    Copy-Item -LiteralPath (Join-Path $payload $fixturePin.name) -Destination $savedClip
    $settings = [ordered]@{
        output_dir = $clips; replay_hotkey = 'Control+Alt+F10'
        replay_seconds = if ($Scenario -eq 'alpha4') { 180 } else { 75 }
        close_to_tray = $false; check_updates = $false; tour_done = $true; launch_at_startup = $false
        updates = @{ auto_check = $false; auto_download = $false; auto_install = $false }
    }
    if ($Scenario -eq 'clean') { $settings.settings_version = 6 }
    if ($Scenario -in 'rc1','rc2') {
        $settings.recording_engine = 'native'
        $settings.crosshair = @{ enabled = $false; color = '#22AAEE' }
    }

    if ($Scenario -ne 'clean') {
        New-Item -ItemType Directory -Path $data -Force | Out-Null
        $settings | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $settingsPath -Encoding utf8
        $phase = 'baseline startup'
        Test-Startup $phase
        Assert-Fixture
    }
    $phase = if ($Scenario -eq 'clean') { 'clean install' } else { 'upgrade install' }
    Invoke-CheckedExe $candidate @('/S') $phase 120
    Assert-Installed $CandidateVersion
    $evidence.legacy_programs_before_upgrade = @($expectedLegacyPrograms | Where-Object { $_ })
    $evidence.legacy_programs_after_upgrade = @($legacyPrograms | Where-Object {
        Test-Path -LiteralPath (Join-Path $installDir $_) -PathType Leaf
    })
    if ($evidence.legacy_programs_after_upgrade.Count) { throw 'The installer left obsolete REPLAYX diagnostic executables.' }
    if ($Scenario -eq 'clean') {
        New-Item -ItemType Directory -Path $data -Force | Out-Null
        $settings | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $settingsPath -Encoding utf8
    }
    Assert-Fixture
    $checks[$phase] = 'PASS'

    $phase = 'installed payload'
    $worker = Join-Path $installDir 'replayx-native-proto.exe'
    if ($CandidateVersion -eq '0.4.0-rc.3') {
        Assert-Hash $app $pins.rc3_app_sha256
        Assert-Hash $worker $pins.rc3_worker_sha256
    } else {
        Assert-Signed $app
        Assert-Signed $worker
    }
    foreach ($media in $pins.runtime) { Assert-Hash (Join-Path $installDir $media.name) $media.sha256 }
    Invoke-CheckedExe (Join-Path $installDir 'media/ffmpeg.exe') @('-v','error','-xerror','-nostdin','-progress','pipe:1','-i',('"' + $savedClip + '"'),'-map','0','-f','null','NUL') 'installed FFmpeg full decode' 45
    $checks[$phase] = 'PASS'

    $phase = 'candidate startup'
    Test-Startup $phase
    Assert-Fixture
    $phase = 'uninstall and data retention'
    Invoke-CheckedExe $uninstaller @('/S') 'silent uninstall' 120
    # NSIS launches a temporary uninstaller. Its file deletion precedes registry
    # and shortcut removal, so wait for all three outcomes on the runner.
    $uninstallClock = [Diagnostics.Stopwatch]::StartNew()
    do {
        $registration = @(Get-Registration)
        $installationPresent = Test-Path -LiteralPath $installDir
        $shortcuts = @(foreach ($folder in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('DesktopDirectory'))) {
            if ($folder -and (Test-Path -LiteralPath $folder)) {
                Get-ChildItem -LiteralPath $folder -Recurse -File -Filter 'REPLAYX.lnk' | Select-Object -ExpandProperty FullName
            }
        })
        if (!$registration.Count -and !$installationPresent -and !$shortcuts.Count) { break }
        Start-Sleep -Milliseconds 500
    } while ($uninstallClock.Elapsed.TotalSeconds -lt 45)
    $evidence.uninstall_cleanup_ms = $uninstallClock.ElapsedMilliseconds
    $evidence.registration_remaining = $registration
    $evidence.shortcuts_remaining = $shortcuts
    $evidence.install_files_remaining = @(if ($installationPresent) {
        Get-ChildItem -LiteralPath $installDir -Recurse -Force | Select-Object -ExpandProperty FullName
    })
    Assert-Fixture
    $checks['settings and recording after uninstall'] = 'PASS'
    if ($registration.Count -or $installationPresent -or $shortcuts.Count) {
        throw 'Uninstall left application registration, files, or shortcuts after the cleanup timeout.'
    }
    $checks[$phase] = 'PASS'
} catch {
    $checks[$phase] = "FAIL - $($_.Exception.Message)"
    $_ | Out-String | Set-Content -LiteralPath (Join-Path $logs 'failure.txt')
    throw
} finally {
    $evidence | ConvertTo-Json -Depth 9 | Set-Content -LiteralPath (Join-Path $logs 'result.json') -Encoding utf8
    if ($env:GITHUB_STEP_SUMMARY) {
        $summary = @("### REPLAYX installer QA - $Scenario", '', '| Check | Result |', '|---|---|')
        foreach ($entry in $checks.GetEnumerator()) { $summary += "| $($entry.Key) | $($entry.Value -replace '[\r\n|]', ' ') |" }
        $summary += '', 'Scope: Windows Server x64, silent installer lifecycle, and startup signals where available. Windows 11 Home, WebView2 interaction, capture, and production updater installation require separate evidence.'
        $summary | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8
    }
}
