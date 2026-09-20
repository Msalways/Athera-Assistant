# Companion evaluations

`companion-v1.jsonl` contains 60 acceptance scenarios, ten in each of six categories.
Bracketed steps are test setup instructions, never messages to send to a model.
All initial results are `not_run`. This catalog is not an accuracy report.

Run the deterministic task harness without a model, cloud key, MCP server or phone:

```powershell
. .\scripts\env.ps1
Get-Content .\evals\fixtures\setup.jsonl | cargo run -p assistant-cli
Get-Content .\evals\fixtures\handoff.jsonl | cargo run -p assistant-cli
powershell -ExecutionPolicy Bypass -File .\evals\run_deterministic.ps1
```

The runner emits one JSON object per input line. `result: "pass"` means the real
core persisted a completed task. `result: "fail"` is emitted for malformed input,
failed tasks and unfinished tasks, and the process exits nonzero after all lines are
reported. Existing `task`, `events`, and `mode` fields remain available for fixture
consumers. `results` contains persisted tool records. A fixture can provide exact
`expected.status`, `expected.tool_ids`, and `expected.source_urls`; a mismatch fails
before any judge metric runs. `parallel-search.jsonl` exercises canonical web routing,
prompt-injection-shaped source text, raw/context separation and cited source output.

The deterministic script also starts the compiled loopback bridge in explicit fixture
mode and compares its normalized streaming event sequence with the JSON-lines runner.
Generated run/worker UUIDs are canonicalized; event kinds, ordering, status, steps,
messages, deltas and output blocks must match exactly.

Run `powershell -ExecutionPolicy Bypass -File .\scripts\bootstrap.ps1` to install
the repository-local Rust/Node prerequisites, then `npm ci` if dependencies are not
present. `scripts\check.ps1` reports missing prerequisites without printing any
environment or credential values. Android SDK/NDK/device requirements are reported
as blocked prerequisites; they are not evidence that an Android build passed.
Run `cargo test --workspace` for policy, cancellation, storage, context and credential
regressions. These fixtures execute Rust software but do not measure model quality.

Install the pinned host-only evaluator and run its offline smoke against the same
compiled Rust runner:

```powershell
python -m venv .\evals\.venv
.\evals\.venv\Scripts\python -m pip install -r .\evals\requirements-eval.txt
powershell -ExecutionPolicy Bypass -File .\evals\run_deepeval_smoke.ps1
```

This smoke uses DeepEval's test runner with an exact local metric. It makes no model
or network call and does not claim answer quality. `run_live.py --dry-run` separately
records missing model profiles, datasets, credentials, or runner support as blocked.

For each scenario, record configuration (local only, local + Needle, optional cloud),
model/runtime hashes, actual output, software/tool events, pass/fail and reviewer.
Record unsupported or unavailable capabilities as blocked, never pass. External-write
scenarios require isolated test accounts and the normal approval flow.

For device runs record phone model, Android version, physical RAM, cold/warm state,
prompt/output token counts, first-token latency, tokens/sec, whole-app peak memory,
thermal observations and crashes. Run the 6 GB and 4 GB tiers separately. Preserve
raw reports outside model prompts and redact credentials before saving fixtures.

No comparative model scores or physical-device performance are claimed yet.
