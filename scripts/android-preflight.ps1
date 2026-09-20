param(
    [string]$Sdk = $env:ANDROID_HOME,
    [string]$Output = (Join-Path $PSScriptRoot '../artifacts/android-preflight.json')
)
$ErrorActionPreference = 'Stop'
if (-not $Sdk) { $Sdk = Join-Path $env:LOCALAPPDATA 'Android/Sdk' }
$blockers = @()
$sdkPresent = Test-Path -LiteralPath $Sdk
$ndks = @()
$platforms = @()
$devices = @()
if ($sdkPresent) {
    $ndkDirectory = Join-Path $Sdk 'ndk'
    if (Test-Path -LiteralPath $ndkDirectory) {
        $ndks = @(Get-ChildItem -LiteralPath $ndkDirectory -Directory | Select-Object -ExpandProperty Name)
    }
    $platformDirectory = Join-Path $Sdk 'platforms'
    if (Test-Path -LiteralPath $platformDirectory) {
        $platforms = @(Get-ChildItem -LiteralPath $platformDirectory -Directory | Select-Object -ExpandProperty Name)
    }
    $adbPath = Join-Path $Sdk 'platform-tools/adb.exe'
    if (Test-Path -LiteralPath $adbPath) {
        $outputLines = @(& $adbPath devices)
        if ($LASTEXITCODE -ne 0) { throw 'adb device discovery failed' }
        foreach ($line in $outputLines) {
            if ($line -match '^([^\s]+)\s+device$') {
                $deviceId = $Matches[1]
                $api = (& $adbPath -s $deviceId shell getprop ro.build.version.sdk).Trim()
                $abi = (& $adbPath -s $deviceId shell getprop ro.product.cpu.abi).Trim()
                $model = (& $adbPath -s $deviceId shell getprop ro.product.model).Trim()
                $emulator = (& $adbPath -s $deviceId shell getprop ro.kernel.qemu).Trim() -eq '1'
                $devices += [ordered]@{ model = $model; api = $api; abi = $abi; emulator = $emulator; eligible = (([int]$api -ge 31) -and ($abi -eq 'arm64-v8a') -and (-not $emulator)) }
            }
        }
    } else { $blockers += 'Android platform-tools/adb is missing' }
} else { $blockers += 'Android SDK is missing' }
if ($ndks.Count -eq 0) { $blockers += 'Android NDK is missing' }
if (-not ($devices | Where-Object { $_.eligible })) { $blockers += 'No authorized physical Android 12+ ARM64 device is connected' }
$report = [ordered]@{
    recorded_at_utc = [DateTime]::UtcNow.ToString('o')
    sdk_present = $sdkPresent
    ndk_versions = $ndks
    platforms = $platforms
    devices = $devices
    blockers = $blockers
    acceptance = 'not_run'
    note = 'Preflight checks prerequisites only. It does not prove APK installation, inference performance, RAM tier support or stability.'
}
$destination = [IO.Path]::GetFullPath($Output)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
$report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $destination
$report | ConvertTo-Json -Depth 5
