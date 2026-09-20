$ErrorActionPreference = 'Stop'
. $PSScriptRoot\env.ps1
$repo = Split-Path $PSScriptRoot -Parent

function Require-Command([string]$Name, [string]$Fix) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing $Name. $Fix"
    }
}

Require-Command cargo 'Run powershell -ExecutionPolicy Bypass -File .\scripts\bootstrap.ps1.'
Require-Command node 'Run powershell -ExecutionPolicy Bypass -File .\scripts\bootstrap.ps1.'
Require-Command npm.cmd 'Run powershell -ExecutionPolicy Bypass -File .\scripts\bootstrap.ps1.'
if (-not (Test-Path (Join-Path $PSScriptRoot '..\node_modules'))) {
    throw 'Missing JavaScript dependencies. Run npm ci from the repository root.'
}

if (-not (Get-Command java -ErrorAction SilentlyContinue)) {
    Write-Warning 'Android prerequisite unavailable: java. Install JDK 21+ and set JAVA_HOME before building Android.'
}
if (-not (Test-Path (Join-Path $repo '.tools/android-sdk/platform-tools/adb.exe'))) {
    Write-Warning 'Android prerequisite unavailable: adb. Install Android SDK Platform Tools, then run .\scripts\android-preflight.ps1.'
}
if (-not (Test-Path (Join-Path $repo '.tools/android-sdk/ndk/27.2.12479018'))) {
    Write-Warning 'Android prerequisite unavailable: NDK r27c. Install it, then run .\scripts\android-preflight.ps1.'
}
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'Rust format failed' }
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Rust lint failed' }
cargo test --workspace
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
& (Join-Path $PSScriptRoot '..\evals\run_deterministic.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Deterministic evaluation failed' }
foreach ($check in @('format:check','lint','test','build')) {
    npm.cmd run $check
    if ($LASTEXITCODE -ne 0) { throw "Frontend $check failed" }
}
