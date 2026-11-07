$ErrorActionPreference='Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    npm.cmd ci
    if ($LASTEXITCODE) { throw 'npm ci failed' }
    npm.cmd run build
    if ($LASTEXITCODE) { throw 'Frontend build failed' }
    cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
    if ($LASTEXITCODE) { throw 'Rust formatting failed' }
    cargo test --manifest-path src-tauri/Cargo.toml --features desktop --locked
    if ($LASTEXITCODE) { throw 'Rust tests failed' }
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features desktop --locked -- -D warnings
    if ($LASTEXITCODE) { throw 'Clippy failed' }
} finally { Pop-Location }
