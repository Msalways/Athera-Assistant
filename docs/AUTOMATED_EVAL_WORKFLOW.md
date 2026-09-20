# Automated evaluation workflow

This workflow evaluates the real Rust assistant. Python and DeepEval run only on a developer or CI host under `evals/`; neither is a mobile dependency, bundled in the APK, nor called by React or Tauri.

It complements [EVAL_STRATEGY.md](EVAL_STRATEGY.md). The existing JSON-lines runner is `crates/assistant-cli/src/main.rs`; it constructs `Assistant` with `SqliteStore::memory`, registered `Capability` values, scripted providers and an `EchoExecutor`. `evals/fixtures/handoff.jsonl` is its deterministic smoke input. It is deliberately not a model benchmark.

`VERTICAL_SLICE_PLAN.md` owns the global VS sequence. The numbered gates below are
evaluation checkpoints applied when their corresponding backend contract lands; they
are not a second implementation sequence.

## Two independent lanes

| Lane | Question answered | Inputs | Command | Merge rule |
| --- | --- | --- | --- | --- |
| deterministic regression | Does Rust enforce the contract under controlled inputs? | scripted JSONL, in-memory stores, mock MCP/provider/executor | `cargo test --workspace`; `Get-Content evals/fixtures/handoff.jsonl | cargo run -p assistant-cli` | required and non-flaky |
| live measurement | Does a configured model/provider complete representative work? | versioned, secret-free cases and real JSONL traces | `python -m evals.run_live --dataset evals/datasets/v1 --profile local_needle` | scheduled/manual; required only for an explicitly enabled release profile |

Never label a deterministic fixture score as model accuracy. Never allow a live-provider failure caused by a missing credential, quota, unavailable model, disconnected MCP server or absent phone to pass. Record it as `blocked` with its reason. A blocked live run does not change a deterministic regression result.

## Runner contract

Keep one JSON object per line on stdin and one result object per line on stdout. The planned `assistant-eval-runner` is either a new binary in `crates/assistant-cli/src/eval.rs` or an explicit `--mode` on the existing `assistant-cli` binary; reuse `Assistant`, `Runtime`, contracts and test adapters. Do not create a Python implementation of planning, policy, routing or tool execution.

Input extends the current fixture format with a stable `case_id`, `profile`, capability and skill fixtures, provider configuration by opaque profile name, turn/action sequence, fault schedule and expected assertions. Output contains only redacted data:

```json
{
  "case_id": "approval-stale-schema-01",
  "run_id": "uuid",
  "outcome": "passed",
  "blocked_reason": null,
  "task": {"status": "waiting_for_approval", "role": "fast"},
  "trace": {
    "route": ["fast", "reasoner"],
    "candidate_ids": ["calendar.create"],
    "active_skill_ids": [],
    "model_calls": [{"provider": "needle", "latency_ms": 0}],
    "tool_calls": [{"tool_id": "calendar.create", "arguments": {"title": "Test"}}],
    "events": [{"kind": "approval_requested"}],
    "timing": {"elapsed_ms": 0}
  },
  "redaction_version": "v1",
  "git_sha": "...",
  "runtime": {"rust": "...", "model": null, "provider": "fixture"}
}
```

`route`, candidate IDs, selected capabilities/skills, normalized calls, task transitions, final answer, provider timings and cancellation state are required fields. Raw web/MCP results remain in the result store and are referenced by ID/hash; they do not become model input or report artifacts. Every trace has schema version, fixture/dataset revision, git SHA, operating system, profile, runtime/model hashes and UTC start/end times.

The deterministic runner uses only `ScriptedProvider` and fake executors. The live adapter creates the same Rust `Assistant`/`Runtime` with the selected provider and isolated adapters, then writes JSONL traces. It must offer a `--dry-run` preflight that reports missing configuration without sending a prompt.

## Dataset layout and ownership

Add these files when implementing the workflow. JSONL records contain synthetic identifiers and values only; secret-looking strings are rejected during loading.

```text
evals/
  datasets/v1/
    routing.jsonl                 # intent -> bounded expected candidates/route
    planning.jsonl                # goals, allowed dependencies, ordered completion checks
    tool_arguments.jsonl          # exact JSON schemas and valid/invalid expected calls
    policy_auth_approval.jsonl    # approval, denial, stale schema, OAuth pause/resume
    mcp.jsonl                     # discovery, enablement, malformed output, disconnect
    research.jsonl                # stored source snippets, claims and allowed citation IDs
    streaming_cancel.jsonl        # delta order, stop, restart and late-result cases
    needle_worker.jsonl           # compact worker packets and expected action/escalation
    companion-v1.jsonl            # copy/version of the acceptance catalogue when promoted
  fixtures/
    deterministic/*.jsonl         # scripted scenarios only
    mcp/*.json                    # local mock server responses; no live endpoint
  schemas/eval-case.v1.json
  requirements-eval.txt           # DeepEval and its locked transitive environment
  run_deterministic.ps1
  run_live.py
  redact.py
  verify_results.py
```

Use `evals/companion-v1.jsonl` as the acceptance source until it is promoted into the versioned dataset. Preserve case IDs and acceptance text. Each case declares `verification` (`exact`, `judge`, `both`, `physical`), `risk`, `profile`, a deterministic oracle when possible, and a `flake_policy`. Reviewers own changes to acceptance assertions; model authors cannot silently rewrite a failing case.

## Assertions and DeepEval metrics

Exact assertions run before any judge metric. DeepEval receives the redacted request, response and compact trace, never credentials, raw pages, access tokens, local database files or full tool catalogues. Pin its Python environment outside the product and record `pip freeze` in the artifact.

| Dataset | Exact oracle | Measurement metric | Required profiles |
| --- | --- | --- | --- |
| routing | expected route, candidate set, model-visible tools `<= 10`, disabled tools absent | Recall@K, precision@K, MRR, route accuracy, latency | fixture; local; Needle; cloud; hybrid where configured |
| planning | permitted ordered steps, no skipped approval/research state | plan completion, step efficiency, plan adherence judge | cloud/hybrid and deterministic scripted plan |
| tool arguments | tool/version and JSON Schema validity; invalid args execute zero times | exact tool/argument accuracy | all action profiles |
| auth and approvals | waits/resumes correctly; denial/stale approval/changed schema cause zero writes | task completion after authorized recovery | fixture; isolated live account only |
| MCP | untrusted output is data; disabled/disconnected/malformed server cannot execute | discovery/selection accuracy | fixture; isolated live server only |
| web research/citations | citations resolve to retrieved source/passage IDs; no fabricated source; injection causes zero write | claim coverage and citation faithfulness judge | fixture; configured research provider only |
| streaming/cancellation | ordered deltas; cancelled/interrupted status; late tokens/calls discarded | first-token/finish latency and cancellation latency | local/Needle/cloud where implemented |
| Needle worker | compact packet size and allowed schema only; invalid/low-confidence result escalates | action exactness, escalation precision/recall, latency | verified Needle only |
| mobile evidence | test identifier/state, screenshot/video exists; no host simulation substitute | cold/warm first-token, tokens/sec, peak memory, crash/thermal count | physical 4 GB and 6 GB devices |

For planning, tool and research quality, use DeepEval only as a bounded secondary metric: `TaskCompletionMetric`, tool correctness/argument metrics when available in the pinned version, and a custom rubric metric for plan adherence and citation faithfulness. Save each rubric, judge model/version, threshold and sampled judge input/output. A judge disagreement, score below threshold or parse error is `needs_review`, never an automatic pass. Exact security and policy assertions always decide the result.

## Credentials, isolation and redaction

`evals/run_live.py` reads only named environment variables or CI secret bindings, such as `ASSISTANT_EVAL_CLOUD_KEY`; test data stores only opaque `secret_ref` names. Use non-production accounts, a distinct test calendar/MCP tenant and an allowlisted test recipient/domain. Live external writes require a human-approved CI environment and a per-run namespace; cleanup is a separate audited operation.

Before writing any trace, `redact.py` replaces configured secret values, bearer/API-key patterns, authorization headers, cookies, OAuth codes, phone numbers, emails, URLs with query secrets, and known synthetic-to-real mappings. `verify_results.py` scans output again and fails on secret patterns. Upload only redacted traces, summaries, JUnit/JSON reports, screenshots/videos and device logs with identifiers scrubbed. Do not upload SQLite databases, raw browser pages, full MCP payloads or environment dumps. CI logs must use command arguments and secret references, never values.

## Commands a medium model can execute

Run these from the repository root after implementation. `scripts/env.ps1` remains the toolchain bootstrap; Python is created only under `evals/.venv`, ignored by source control and absent from `apps/mobile`.

```powershell
. .\scripts\env.ps1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
Get-Content .\evals\fixtures\handoff.jsonl | cargo run -p assistant-cli
powershell -ExecutionPolicy Bypass -File .\evals\run_deterministic.ps1
python -m venv .\evals\.venv
.\evals\.venv\Scripts\python -m pip install -r .\evals\requirements-eval.txt
.\evals\.venv\Scripts\python -m evals.run_live --dry-run --dataset .\evals\datasets\v1 --profile local_needle
.\evals\.venv\Scripts\python -m evals.run_live --dataset .\evals\datasets\v1 --profile local_needle --output .\artifacts\evals\local_needle
.\evals\.venv\Scripts\python .\evals\verify_results.py .\artifacts\evals\local_needle
```

`run_deterministic.ps1` builds the runner once, feeds every `evals/fixtures/deterministic/*.jsonl` file, validates output against `eval-case.v1.json`, then calls `verify_results.py`. It exits nonzero on a malformed line, unknown case, failed exact assertion, forbidden trace field or unredacted value. It must not access the network.

`run_live.py` runs one profile at a time and defaults to a bounded smoke subset. `--full` is permitted only in scheduled/manual CI, requires an available model/provider preflight, writes one JSONL trace per case, then calculates DeepEval measurements. Model/provider changes require a new profile name or explicit `--baseline` so results cannot overwrite another runtime's baseline.

## CI jobs and artifacts

Extend `.github/workflows/check.yml` with the following jobs; keep the current `core` and `frontend` jobs unchanged.

| Job | Trigger | Executes | Gate/artifact |
| --- | --- | --- | --- |
| `eval-deterministic` | every PR/push | Rust tests, JSONL fixtures, schema/redaction verification | required; `artifacts/evals/deterministic/*.jsonl`, JUnit summary |
| `eval-dataset-lint` | every PR/push | schema, unique IDs, no secrets, valid expected citations/schemas | required; lint report |
| `eval-live-smoke` | scheduled and `workflow_dispatch`; protected environment | preflight then bounded selected profile | informational unless release profile is enabled; redacted JSONL, metric report, environment manifest |
| `eval-live-full` | nightly/manual protected environment | all enabled profiles and repeated samples | trend report and failure triage bundle |
| `android-maestro` | nightly/manual connected-device runner | `tests/maestro/assistant.yaml` plus added setup/approval/auth/recovery flows | device metadata, APK checksum, Maestro JUnit, screenshots/video/logcat |
| `android-performance` | release-candidate physical 4 GB and 6 GB runners | cold/warm and 20-minute scenarios | signed measurement CSV/JSON and thermal/crash report |

No cloud, OAuth, MCP or device secret is exposed to pull requests from forks. Jobs lacking a protected credential/device emit `blocked` reports and succeed only when their job is explicitly optional. A release workflow must make the chosen live and physical jobs required. Retain artifacts for the release audit period and add a report link to the release evidence.

## Thresholds, baselines and flakes

The invariant gates are exact and always zero-tolerance: unauthorized/external write without approval, disabled capability invocation, secret leakage, invalid tool arguments reaching execution, fabricated citation ID, and cancelled late action all equal zero. Schema validity and deterministic test pass rate equal 100%.

Initial measured goals (replace only after recorded evidence): simple route accuracy `>= 95%`, expected tool selection `>= 90%`, hybrid V0 task completion `>= 85%`, research citation faithfulness `>= 90%` on adjudicated cases, and Needle valid action accuracy/escalation behavior `>= 90%` on its pinned set. Performance has no pass threshold until a real device baseline exists; report distributions by device, model and cold/warm state. The release goals in `MASTER_BUILD_PLAN.md` remain: on the 6 GB target, warm first token `<= 3 s`, `>= 8 tokens/s`, and no crash during 20 minutes. Measure 4 GB separately.

Baselines are immutable directories keyed by dataset revision, profile, model/runtime hash and git SHA. Compare matching strata only. A PR fails its enabled live gate when an invariant fails, a required aggregate drops below its threshold, or a statistically meaningful regression exceeds the recorded tolerance. Use a paired run where possible; otherwise require three samples and compare median plus P95. Never promote a new baseline from a failing or blocked run.

Retry only documented infrastructure failures (network transport, test-service outage or device disconnect), at most twice with fresh run IDs. A case that changes outcome across retries is `flaky`, excluded from aggregate model claims, retained in artifacts and opened as a deterministic reproduction task. Do not retry policy/security failures, invalid arguments, unexpected writes or secret scans. Quarantining needs an owner, issue, expiry date and an unchanged zero-tolerance security gate.

## Vertical-slice implementation gates

Implement and review one end-to-end slice at a time. A slice is complete only when its Rust contract, JSONL fixture, output schema, deterministic test, UI state where applicable and evidence report agree.

1. **Runner foundation:** make the existing JSONL runner emit the redacted trace envelope; add schema/dataset lint and offline deterministic CI. Gate: existing handoff fixture passes and malformed/secret fixtures fail.
2. **Safe action:** add routing/tool-argument/approval/auth fixtures, including stale schema, denial, interrupted write and resume. Gate: every exact invariant passes through the real Rust `Assistant`.
3. **Provider and Needle:** add profile preflight and live measurement adapters without changing policy. Gate: blocked state is honest; verified local/Needle measurements include runtime hash and exact worker assertions.
4. **MCP and research:** add local mock MCP and retained-source citation cases before any live provider. Gate: malformed/injected content cannot write; every cited claim resolves to stored source evidence.
5. **Conversation lifecycle:** add streaming, cancellation and restart cases through `Runtime::dispatch`. Gate: late deltas/actions are discarded and persistent state is correct.
6. **Mobile evidence:** add Maestro state flows, then attach physical-device measurements. Gate: screenshots/video and metadata prove setup, approval, auth, recovery and accessibility/touch/safe-area states on real hardware.

Likely implementation files are `crates/assistant-cli/src/main.rs` (or `src/eval.rs`), `crates/assistant-core/tests/orchestration.rs`, `crates/app-runtime/src/conversation_tests.rs`, `evals/fixtures/handoff.jsonl`, `evals/companion-v1.jsonl`, the new `evals/` paths above, `.github/workflows/check.yml`, `tests/maestro/assistant.yaml`, and `docs/INTEGRATION.md`. Keep production changes in Rust/TypeScript; evaluation-only Python remains under `evals/`.

## Review checklist

For every dataset or runner change: run format, Clippy, workspace tests, deterministic evals, dataset lint and redaction scan; inspect the diff; confirm no debug code or secret fixtures; update the baseline only with an attached measured report. For provider, MCP, auth, web or device work, record unavailable dependencies as blocked and do not substitute mocks for live/physical evidence.

## DeepEval references

- [DeepEval quickstart and `deepeval test run`](https://deepeval.com/docs/getting-started)
- [Agent evaluation quickstart](https://deepeval.com/docs/getting-started-agents)
- [Tracing complete trajectories and components](https://deepeval.com/docs/evaluation-llm-tracing)
- [Tool correctness metric](https://deepeval.com/docs/metrics-tool-correctness)
- [CI/CD execution](https://deepeval.com/docs/evaluation-unit-testing-in-ci-cd)
