$repo = Split-Path $PSScriptRoot -Parent
$env:CARGO_HOME = Join-Path $repo '.tools/cargo'
$env:RUSTUP_HOME = Join-Path $repo '.tools/rustup'
$nodeDir = Join-Path $repo '.tools/node-v24.21.0-win-x64'
$androidSdk = Join-Path $repo '.tools/android-sdk'
$androidNdk = Join-Path $androidSdk 'ndk/27.2.12479018'
if (Test-Path -LiteralPath (Join-Path $nodeDir 'node.exe')) {
    $env:PATH = "$nodeDir;$env:PATH"
}
if (Test-Path -LiteralPath $androidSdk) {
    $env:ANDROID_HOME = $androidSdk
    $env:ANDROID_SDK_ROOT = $androidSdk
}
if (Test-Path -LiteralPath $androidNdk) {
    $env:ANDROID_NDK_HOME = $androidNdk
}
$env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"
