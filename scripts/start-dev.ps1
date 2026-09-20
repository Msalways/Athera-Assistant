$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
npm.cmd --prefix $repo run dev
exit $LASTEXITCODE
