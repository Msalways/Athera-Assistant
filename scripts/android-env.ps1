$repo = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'env.ps1')
$env:ANDROID_HOME = Join-Path $repo '.tools/android-sdk'
$env:NDK_HOME = Join-Path $repo '.tools/android-ndk-r27c'
$env:NEEDLE_LIB_DIR = Join-Path $repo '.tools/needle2/android-arm64'
$env:GRADLE_USER_HOME = Join-Path $repo '.tools/gradle'
$env:SMS_KEYSTORE = Join-Path $repo '.tools/keys/needle-dev.jks'
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = 'C:/Program Files/Java/jdk-22' }
$env:PATH = "$env:JAVA_HOME/bin;$env:ANDROID_HOME/platform-tools;$env:PATH"
