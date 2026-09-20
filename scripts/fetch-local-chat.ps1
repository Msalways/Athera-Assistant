param([switch]$Android)

$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$vendorRoot = Join-Path $repo 'vendor/local-chat'
$sourceRoot = Join-Path $vendorRoot 'llama.cpp'
$tag = 'b10886'
$commit = 'f1b6fbf35cfa010b0a8d6301fdfccbb7f41bd903'
$cmake = (Get-Command cmake -ErrorAction SilentlyContinue).Source
if (-not $cmake) {
    $cmakeRoot = Join-Path $repo '.tools/cmake'
    $cmake = Join-Path $cmakeRoot 'cmake-4.4.3-windows-x86_64/bin/cmake.exe'
    if (-not (Test-Path -LiteralPath $cmake)) {
        New-Item -ItemType Directory -Path $cmakeRoot -Force | Out-Null
        $cmakeArchive = Join-Path $cmakeRoot 'cmake.zip'
        Invoke-WebRequest -UseBasicParsing 'https://github.com/Kitware/CMake/releases/download/v4.4.3/cmake-4.4.3-windows-x86_64.zip' -OutFile $cmakeArchive
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $cmakeArchive).Hash.ToLowerInvariant() -ne '4d52ebab7193a698651639ed80d8d04fd903358843572cf44c7fd234cb7c26ab') { throw 'CMake archive checksum mismatch' }
        tar.exe -xf $cmakeArchive -C $cmakeRoot
        if ($LASTEXITCODE -ne 0) { throw 'CMake extraction failed' }
    }
}
$ninjaDirectory = Join-Path $repo '.tools/ninja'
if (-not (Test-Path -LiteralPath (Join-Path $ninjaDirectory 'ninja.exe'))) {
    New-Item -ItemType Directory -Path $ninjaDirectory -Force | Out-Null
    $archive = Join-Path $ninjaDirectory 'ninja-win.zip'
    Invoke-WebRequest -UseBasicParsing 'https://github.com/ninja-build/ninja/releases/download/v1.13.2/ninja-win.zip' -OutFile $archive
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant() -ne '07fc8261b42b20e71d1720b39068c2e14ffcee6396b76fb7a795fb460b78dc65') { throw 'Ninja archive checksum mismatch' }
    Expand-Archive -LiteralPath $archive -DestinationPath $ninjaDirectory -Force
}
$env:PATH = "$ninjaDirectory;$env:PATH"

if (-not (Test-Path (Join-Path $sourceRoot '.git'))) {
    git clone --filter=blob:none --branch $tag --depth 1 https://github.com/ggml-org/llama.cpp.git $sourceRoot
    if ($LASTEXITCODE -ne 0) { throw 'Failed to clone llama.cpp.' }
}
git -C $sourceRoot fetch --depth 1 origin "refs/tags/${tag}:refs/tags/${tag}"
if ($LASTEXITCODE -ne 0) { throw 'Failed to fetch the pinned llama.cpp tag.' }
git -C $sourceRoot checkout --detach $tag
if ($LASTEXITCODE -ne 0) { throw 'Failed to check out the pinned llama.cpp tag.' }
if ((git -C $sourceRoot rev-parse HEAD).Trim() -ne $commit) {
    throw 'The llama.cpp tag did not resolve to the pinned commit.'
}

if ($Android) {
    if (-not $env:ANDROID_NDK_HOME) { throw 'ANDROID_NDK_HOME is required for an Android build.' }
    $buildRoot = Join-Path $vendorRoot 'build-android-arm64'
    & $cmake -G Ninja -S $vendorRoot -B $buildRoot -DCMAKE_BUILD_TYPE=Release `
        -DCMAKE_TOOLCHAIN_FILE="$env:ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake" `
        -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-31 `
        -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=OFF
    if ($LASTEXITCODE -ne 0) { throw 'Android CMake configuration failed.' }
} else {
    $buildRoot = Join-Path $vendorRoot 'build-host'
    & $cmake -G 'Visual Studio 17 2022' -A x64 -S $vendorRoot -B $buildRoot -DGGML_OPENMP=OFF -DGGML_NATIVE=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=OFF
    if ($LASTEXITCODE -ne 0) { throw 'Host CMake configuration failed.' }
}
if ($Android) {
    & $cmake --build $buildRoot --config Release --parallel 4 --target athera_local_chat
    if ($LASTEXITCODE -ne 0) { throw 'Android native shim build failed.' }
} else {
    & $cmake --build $buildRoot --config Release --parallel 4 --target athera-chat athera_local_chat
    if ($LASTEXITCODE -ne 0) { throw 'Host local-chat build failed.' }
}

if (-not $Android) {
    $binary = if ($env:OS -eq 'Windows_NT') {
        Join-Path $buildRoot 'Release/athera-chat.exe'
    } else {
        Join-Path $buildRoot 'athera-chat'
    }
    $hash = (Get-FileHash -Algorithm SHA256 $binary).Hash.ToLowerInvariant()
    Set-Content -NoNewline -Path "$binary.sha256" -Value $hash
}
