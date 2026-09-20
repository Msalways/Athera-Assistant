$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $repo '.tools/needle2'
New-Item -ItemType Directory -Force $destination | Out-Null
$revision = '32e9e3a93b205f786929697446ae669cf0a84579'
$base = "https://huggingface.co/Cactus-Compute/needle2/resolve/$revision"
$archive = Join-Path $destination 'windows.zip'
Invoke-WebRequest "$base/python/cactus_needle-2.0.4-py3-none-win_amd64.whl" -OutFile $archive -UseBasicParsing -TimeoutSec 180
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne 'b4803501a109af3782efe112be27515947c108ecec3bf85703ae6b97bb7210e1') { throw 'Needle archive checksum mismatch' }
Expand-Archive $archive -DestinationPath (Join-Path $destination 'windows') -Force
Get-FileHash (Join-Path $destination 'windows/needle/libneedle.dll') -Algorithm SHA256
New-Item -ItemType Directory -Force (Join-Path $destination 'android-arm64') | Out-Null
$entries = Invoke-RestMethod "https://huggingface.co/api/models/Cactus-Compute/needle2/tree/$revision/android-arm64"
foreach ($filename in @('libneedle.a','needle.h')) {
    $file = Join-Path $destination "android-arm64/$filename"
    Invoke-WebRequest "$base/android-arm64/$filename" -OutFile $file -UseBasicParsing -TimeoutSec 180
    $entry = $entries | Where-Object { $_.path -eq "android-arm64/$filename" }
    if ($entry.lfs.oid -and (Get-FileHash $file -Algorithm SHA256).Hash -ne $entry.lfs.oid) { throw 'Needle Android checksum mismatch' }
}
Invoke-WebRequest "$base/LICENSE" -OutFile (Join-Path $destination 'LICENSE') -UseBasicParsing -TimeoutSec 30
