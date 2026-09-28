$repo = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'env.ps1')
$env:ANDROID_HOME = Join-Path $repo '.tools/android-sdk'
$env:NDK_HOME = Join-Path $repo '.tools/android-ndk-r27c'
$env:NEEDLE_LIB_DIR = Join-Path $repo '.tools/needle2/android-arm64'
$gnuBin = 'C:\msys64\mingw64\bin'
if (Test-Path -LiteralPath $gnuBin) {
    $env:PATH = "$gnuBin;$env:PATH"
}
$env:GRADLE_USER_HOME = Join-Path $repo '.tools/gradle'
$env:ATHERA_KEYSTORE = Join-Path $repo '.tools/keys/needle-dev.jks'
$env:SMS_KEYSTORE = $env:ATHERA_KEYSTORE
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = 'C:/Program Files/Java/jdk-22' }
$androidToolchain = Join-Path $env:NDK_HOME 'toolchains/llvm/prebuilt/windows-x86_64/bin'
$androidClang = Join-Path $androidToolchain 'aarch64-linux-android31-clang.cmd'
$env:CC_aarch64_linux_android = $androidClang
$env:AR_aarch64_linux_android = Join-Path $androidToolchain 'llvm-ar.exe'
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $androidClang
$env:PATH = "$env:JAVA_HOME/bin;$env:ANDROID_HOME/platform-tools;$androidToolchain;$env:PATH"
