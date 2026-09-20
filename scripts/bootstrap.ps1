$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$toolDir = Join-Path $repo '.tools'
New-Item -ItemType Directory -Force $toolDir | Out-Null
$nodeVersion = 'v24.21.0'
$nodeFolder = "node-$nodeVersion-win-x64"
if (!(Test-Path (Join-Path $toolDir "$nodeFolder/node.exe"))) {
    $archive = "$nodeFolder.zip"
    Invoke-WebRequest "https://nodejs.org/dist/$nodeVersion/$archive" -OutFile (Join-Path $toolDir $archive) -UseBasicParsing
    $checksums = (Invoke-WebRequest "https://nodejs.org/dist/$nodeVersion/SHASUMS256.txt" -UseBasicParsing).Content
    $expected = ($checksums -split "`n" | Where-Object { $_.EndsWith("  $archive") }) -split '\s+'
    if (!$expected -or (Get-FileHash (Join-Path $toolDir $archive) -Algorithm SHA256).Hash -ne $expected[0]) { throw 'Node checksum mismatch' }
    Expand-Archive (Join-Path $toolDir $archive) -DestinationPath $toolDir -Force
}
$env:CARGO_HOME = Join-Path $toolDir 'cargo'
$env:RUSTUP_HOME = Join-Path $toolDir 'rustup'
if (!(Test-Path (Join-Path $env:CARGO_HOME 'bin/rustup.exe'))) {
    Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile (Join-Path $toolDir 'rustup-init.exe') -UseBasicParsing
    & (Join-Path $toolDir 'rustup-init.exe') -y --no-modify-path --profile minimal --default-toolchain stable
    if ($LASTEXITCODE -ne 0) { throw 'Rust installation failed' }
}
$env:PATH = "$(Join-Path $toolDir $nodeFolder);$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"
rustup component add rustfmt clippy
if ($LASTEXITCODE -ne 0) { throw 'Rust components failed' }
if (!(Test-Path (Join-Path $repo 'node_modules'))) {
    Write-Host 'Installing JavaScript dependencies with npm ci.'
    npm.cmd ci
    if ($LASTEXITCODE -ne 0) { throw 'JavaScript dependency installation failed. Check network access, then run npm ci from the repository root.' }
}
if (-not (Get-Command java -ErrorAction SilentlyContinue)) {
    Write-Warning 'Android builds are blocked: install JDK 21+ and set JAVA_HOME.'
}
if (!(Test-Path (Join-Path $toolDir 'android-sdk'))) {
    Write-Warning 'Android builds are blocked: install the Android SDK, then run .\scripts\android-preflight.ps1.'
}
if (!(Test-Path (Join-Path $toolDir 'android-sdk/ndk/27.2.12479018'))) {
    Write-Warning 'Android builds are blocked: install Android NDK r27c, then run .\scripts\android-preflight.ps1.'
}
node --version
cargo --version
