# Load a preview page in headless Edge via the DevTools protocol and report
# exceptions, console errors and whether the main thread still responds.
param([string]$Url = 'http://127.0.0.1:1430/dev/preview.html?state=replay#audio', [int]$Port = 9333, [int]$Seconds = 8, [string]$Screenshot = '', [int]$Width = 1280, [int]$Height = 820, [string]$Click = '')
# Chrome, not Edge: a new Edge profile signs in with the Windows account and
# syncs the user's extensions and data into it (seen 2026-11-08).
$edge = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
$profile = Join-Path $env:TEMP "rx-cdp-$([guid]::NewGuid())"
$p = Start-Process $edge -ArgumentList "--headless=new", "--disable-gpu", "--disable-extensions", "--disable-sync", "--no-first-run", "--user-data-dir=$profile", "--remote-debugging-port=$Port", "about:blank" -PassThru
try {
    $target = $null
    for ($i = 0; $i -lt 40 -and -not $target; $i++) {
        Start-Sleep -Milliseconds 250
        try { $target = (Invoke-RestMethod "http://127.0.0.1:$Port/json") | Where-Object { $_.type -eq 'page' } | Select-Object -First 1 } catch {}
    }
    if (-not $target) { throw 'DevTools endpoint not available' }
    $ws = New-Object System.Net.WebSockets.ClientWebSocket
    $ct = [Threading.CancellationToken]::None
    $ws.ConnectAsync([Uri]$target.webSocketDebuggerUrl, $ct).Wait()
    $id = 0
    function Send($method, $params = @{}) {
        $script:id++
        $json = @{ id = $script:id; method = $method; params = $params } | ConvertTo-Json -Depth 6 -Compress
        $bytes = [Text.Encoding]::UTF8.GetBytes($json)
        $ws.SendAsync([ArraySegment[byte]]$bytes, 'Text', $true, $ct).Wait()
        return $script:id
    }
    # One outstanding receive; waiting with a timeout never cancels the socket.
    $script:buffer = New-Object byte[] 4194304
    $script:pending = $null
    $script:text = New-Object Text.StringBuilder
    function Receive([int]$timeoutMs) {
        while ($true) {
            if (-not $script:pending) { $script:pending = $ws.ReceiveAsync([ArraySegment[byte]]$script:buffer, $ct) }
            if (-not $script:pending.Wait($timeoutMs)) { return $null }
            $r = $script:pending.Result
            $script:pending = $null
            [void]$script:text.Append([Text.Encoding]::UTF8.GetString($script:buffer, 0, $r.Count))
            if ($r.EndOfMessage) {
                $json = $script:text.ToString(); [void]$script:text.Clear()
                return $json | ConvertFrom-Json
            }
        }
    }
    Send 'Runtime.enable' | Out-Null
    Send 'Log.enable' | Out-Null
    Send 'Emulation.setDeviceMetricsOverride' @{ width = $Width; height = $Height; deviceScaleFactor = 1; mobile = $false } | Out-Null
    Send 'Emulation.setEmulatedMedia' @{ features = @(@{ name = 'prefers-reduced-motion'; value = 'reduce' }) } | Out-Null
    Send 'Page.navigate' @{ url = $Url } | Out-Null
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        $m = Receive 500
        if (-not $m) { continue }
        switch ($m.method) {
            'Runtime.exceptionThrown' { "EXCEPTION: $($m.params.exceptionDetails.exception.description)" }
            'Runtime.consoleAPICalled' { if ($m.params.type -in 'error', 'warning') { "CONSOLE $($m.params.type): $(($m.params.args | ForEach-Object { if ($null -ne $_.value) { $_.value } else { $_.description } }) -join ' ')" } }
            'Log.entryAdded' { if ($m.params.entry.level -in 'error', 'warning') { "LOG: $($m.params.entry.text)" } }
        }
    }
    $probe = Send 'Runtime.evaluate' @{ expression = 'document.querySelectorAll(".card").length + " cards; " + document.title'; returnByValue = $true }
    $answered = $false
    $until = (Get-Date).AddSeconds(4)
    while ((Get-Date) -lt $until) {
        $m = Receive 500
        if ($m -and $m.id -eq $probe) { "PAGE RESPONDS: $($m.result.result.value)"; $answered = $true; break }
    }
    if (-not $answered) { 'PAGE DID NOT RESPOND (main thread busy or frozen)' }
    if ($Click) {
        # Optional: click the first element matching a CSS selector, then settle.
        Send 'Runtime.evaluate' @{ expression = "document.querySelector('$Click')?.click()" } | Out-Null
        $until = (Get-Date).AddSeconds(2); while ((Get-Date) -lt $until) { [void](Receive 300) }
    }
    if ($Screenshot) {
        $shot = Send 'Page.captureScreenshot' @{ format = 'png' }
        $until = (Get-Date).AddSeconds(10)
        while ((Get-Date) -lt $until) {
            $m = Receive 1000
            if ($m -and $m.id -eq $shot) { [IO.File]::WriteAllBytes($Screenshot, [Convert]::FromBase64String($m.result.data)); "saved $Screenshot"; break }
        }
    }
} finally {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" | Where-Object { $_.CommandLine -like "*$profile*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue
}
