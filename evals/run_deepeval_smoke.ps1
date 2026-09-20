$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    . (Join-Path $repo 'scripts/env.ps1')
}

$venvBin = if ($IsLinux -or $IsMacOS) { '.venv/bin' } else { '.venv/Scripts' }
$deepEvalName = if ($IsLinux -or $IsMacOS) { 'deepeval' } else { 'deepeval.exe' }
$deepEval = Join-Path (Join-Path $PSScriptRoot $venvBin) $deepEvalName
if (-not (Test-Path -LiteralPath $deepEval)) {
    throw 'Missing evals/.venv. Create it and install evals/requirements-eval.txt first.'
}

cargo build -p assistant-cli
if ($LASTEXITCODE -ne 0) { throw 'Could not build assistant-cli for DeepEval.' }
$env:PYTHONUTF8 = '1'
$env:NO_COLOR = '1'
$env:DEEPEVAL_TELEMETRY_OPT_OUT = '1'
& $deepEval test run (Join-Path $PSScriptRoot 'deepeval_smoke.py')
if ($LASTEXITCODE -ne 0) { throw 'DeepEval real-runner smoke failed.' }
