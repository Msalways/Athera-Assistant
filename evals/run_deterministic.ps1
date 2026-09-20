$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    . (Join-Path $repo 'scripts/env.ps1')
}

$fixtures = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'fixtures') -Filter '*.jsonl' -File | Sort-Object Name)
if ($fixtures.Count -eq 0) { throw 'No deterministic fixtures found. Restore evals/fixtures/*.jsonl.' }

cargo build -p assistant-cli --bins
if ($LASTEXITCODE -ne 0) { throw 'Could not build assistant-cli. Run cargo test --workspace for details.' }

$runner = Join-Path $repo 'target/debug/assistant-cli.exe'
if (-not (Test-Path -LiteralPath $runner)) { throw 'assistant-cli binary is missing after build. Run cargo build -p assistant-cli.' }

foreach ($fixture in $fixtures) {
    Write-Host "Running deterministic fixture: $($fixture.Name)"
    Get-Content -LiteralPath $fixture.FullName | & $runner
    if ($LASTEXITCODE -ne 0) { throw "Fixture failed: $($fixture.Name)" }
}

$bridge = Join-Path $repo 'target/debug/assistant-dev.exe'
if (-not (Test-Path -LiteralPath $bridge)) { throw 'assistant-dev binary is missing after build.' }
python (Join-Path $PSScriptRoot 'verify_bridge_parity.py') $runner $bridge (Join-Path $PSScriptRoot 'fixtures/streaming-output.jsonl')
if ($LASTEXITCODE -ne 0) { throw 'JSON-lines/browser bridge event parity failed' }
