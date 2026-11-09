$ErrorActionPreference='Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    # Tauri's build step needs src-tauri/media (generated, git-ignored), so a
    # fresh checkout prepares it from the pinned runtime (scripts/setup-media.ps1).
    if (!(Test-Path 'src-tauri/media/ffmpeg.exe')) { & (Join-Path $PSScriptRoot 'prepare-bundle.ps1') }
    npm.cmd ci
    if ($LASTEXITCODE) { throw 'npm ci failed' }
    npm.cmd run build
    if ($LASTEXITCODE) { throw 'Frontend build failed' }
    npm.cmd run test:frontend
    if ($LASTEXITCODE) { throw 'Frontend tests failed' }
    cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
    if ($LASTEXITCODE) { throw 'Rust formatting failed' }
    cargo test --manifest-path src-tauri/Cargo.toml --features desktop --locked
    if ($LASTEXITCODE) { throw 'Rust tests failed' }
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features desktop --locked -- -D warnings
    if ($LASTEXITCODE) { throw 'Clippy failed' }
} finally { Pop-Location }
