$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'android-env.ps1')
Set-Location $repo
if (-not (Test-Path -LiteralPath $env:SMS_KEYSTORE)) { throw 'Local signing key missing. Generate the development key before building.' }
$runtimeDirectory = Join-Path $repo 'apps/mobile/src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a'
New-Item -ItemType Directory -Force $runtimeDirectory | Out-Null
Copy-Item -LiteralPath (Join-Path $env:NDK_HOME 'toolchains/llvm/prebuilt/windows-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so') -Destination $runtimeDirectory
& (Join-Path $repo 'node_modules/.bin/tauri.cmd') android build --target aarch64 --apk --ci
if ($LASTEXITCODE -ne 0) { throw 'Android build failed' }
$apk = Get-ChildItem 'apps/mobile/src-tauri/gen/android/app/build/outputs/apk' -Recurse -Filter '*release.apk' | Select-Object -First 1
if (-not $apk) { throw 'Signed release APK not found' }
$destination = Join-Path $repo 'artifacts/needle-sms-test.apk'
Copy-Item -LiteralPath $apk.FullName -Destination $destination
& (Join-Path $env:ANDROID_HOME 'build-tools/36.0.0/apksigner.bat') verify --verbose --print-certs $destination
if ($LASTEXITCODE -ne 0) { throw 'APK signature verification failed' }
$hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  needle-sms-test.apk" | Set-Content (Join-Path $repo 'artifacts/needle-sms-test.apk.sha256')
Get-Item -LiteralPath $destination | Select-Object FullName,Length
Write-Output "SHA-256: $hash"
