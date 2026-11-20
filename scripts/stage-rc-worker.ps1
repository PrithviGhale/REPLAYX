$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $hostLine = rustc -vV | Select-String '^host: '
    if ($LASTEXITCODE -or !$hostLine) { throw 'Could not identify the Rust host target.' }
    $triple = $hostLine.Line.Substring(6)
    & cargo build --manifest-path src-tauri/Cargo.toml --release --locked --bin replayx-native-proto
    if ($LASTEXITCODE) { throw 'Native worker build failed.' }
    $target = (cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 --no-deps --locked | ConvertFrom-Json).target_directory
    if ($LASTEXITCODE -or !$target) { throw 'Could not locate Cargo artifacts.' }
    $binaries = New-Item -ItemType Directory -Force src-tauri/binaries
    $worker = Join-Path $binaries "replayx-native-proto-$triple.exe"
    Copy-Item -LiteralPath (Join-Path $target 'release/replayx-native-proto.exe') -Destination $worker -Force
    [pscustomobject]@{ Target = $target; Worker = $worker; Directory = $binaries.FullName }
} finally { Pop-Location }
