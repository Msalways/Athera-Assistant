$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'android-env.ps1')
Set-Location $repo
if (-not (Test-Path -LiteralPath $env:ATHERA_KEYSTORE)) { throw 'Local signing key missing. Generate the development key before building.' }
$runtimeDirectory = Join-Path $repo 'apps/mobile/src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a'
New-Item -ItemType Directory -Force $runtimeDirectory | Out-Null
Copy-Item -LiteralPath (Join-Path $env:NDK_HOME 'toolchains/llvm/prebuilt/windows-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so') -Destination $runtimeDirectory

& npm.cmd run build -w apps/mobile
if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' }
if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target/android-arm64'
}
& cargo +stable-x86_64-pc-windows-gnu build --manifest-path apps/mobile/src-tauri/Cargo.toml --target aarch64-linux-android --release --features custom-protocol
if ($LASTEXITCODE -ne 0) { throw 'Android Rust build failed' }
$nativeLibrary = Join-Path $env:CARGO_TARGET_DIR 'aarch64-linux-android/release/libassistant_mobile_shell.so'
if (-not (Test-Path -LiteralPath $nativeLibrary)) { throw 'Android native library was not produced' }
Copy-Item -LiteralPath $nativeLibrary -Destination $runtimeDirectory -Force

$androidProject = Join-Path $repo 'apps/mobile/src-tauri/gen/android'
$oauthRedirectHost = if ($env:OAUTH_REDIRECT_HOST) { $env:OAUTH_REDIRECT_HOST } else { 'assistant.example.com' }
$oauthRedirectPath = if ($env:OAUTH_REDIRECT_PATH) { $env:OAUTH_REDIRECT_PATH } else { '/oauth/callback' }
Push-Location $androidProject
try {
    & .\gradlew.bat ':app:assembleArm64Release' "-PoauthRedirectHost=$oauthRedirectHost" "-PoauthRedirectPath=$oauthRedirectPath"
    if ($LASTEXITCODE -ne 0) { throw 'Android Gradle package failed' }
} finally {
    Pop-Location
}

$apk = Get-ChildItem 'apps/mobile/src-tauri/gen/android/app/build/outputs/apk' -Recurse -Filter '*release.apk' | Select-Object -First 1
if (-not $apk) { throw 'Signed release APK not found' }
$destination = Join-Path $repo 'artifacts/athera-mobile.apk'
Copy-Item -LiteralPath $apk.FullName -Destination $destination -Force
& (Join-Path $env:ANDROID_HOME 'build-tools/36.0.0/apksigner.bat') verify --verbose --print-certs $destination
if ($LASTEXITCODE -ne 0) { throw 'APK signature verification failed' }
$hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  athera-mobile.apk" | Set-Content (Join-Path $repo 'artifacts/athera-mobile.apk.sha256')
Get-Item -LiteralPath $destination | Select-Object FullName,Length
Write-Output "SHA-256: $hash"
