# AETHRA vertical-slice execution plan

## Authority and outcome

`MASTER_BUILD_PLAN.md` defines product scope. This document defines implementation
order and team workflow. `TASK_INDEX.md` is the only status/evidence ledger. The
backend, mobile and evaluation playbooks linked below provide file-level instructions.

The target is an Android-first assistant in which the Rust runtime owns task state,
policy, context, provider routing, tools and recovery. React renders typed state.
Needle compiles small local action packets, cloud models handle broader reasoning,
and external MCP/native providers remain replaceable adapters.

Work proceeds as vertical slices. Every functional slice starts with a locked Rust
contract, runs through the real runtime, appears in a mobile state, and produces
automated evidence. A backend component is not complete merely because it compiles.

## Current baseline

Already present:

- typed model, task, tool, approval and persistence contracts;
- deterministic Rust action loop, capability retrieval and bounded context;
- local conversation and Needle/cloud adapter boundaries;
- Streamable HTTP MCP tool discovery/calls without authentication;
- mobile conversation, task, configuration and research shells;
- JSON-lines runner plus a 60-scenario catalog;
- Rust/frontend CI checks.

Open foundations:

- durable Android/server secret stores and versioned MCP connection contracts;
- MCP OAuth, credential profiles, stdio host transport and legacy SSE compatibility;
- a real web-search provider connection and normalized source ingestion;
- one common stream for text, tool, worker, approval and structured-output events;
- bounded graph workers and durable schedules;
- DeepEval adapter, datasets, reports and automated quality jobs;
- Android native capabilities, foreground voice, document ingestion and device proof.

## Execution lanes

Three medium-reasoning workers may run concurrently after the root coordinator locks
the slice contract:

| Lane | Owns | May run when |
| --- | --- | --- |
| Backend | Rust contracts, core, adapters, storage, Kotlin bridge | Current slice contract is locked |
| Mobile | React components, styles, accessibility and state fixtures | Current slice event/output fixture is committed |
| Evaluation | JSONL scenarios, Python DeepEval adapter, reports and CI | Current slice runner schema is committed |

The coordinator alone changes shared contracts during a slice. Backend works on slice
`N`; mobile and evaluation may finish slice `N-1`. This one-slice stagger creates the
requested back-and-forth loop without making UI or eval workers guess unfinished
payloads. No two workers edit the same file.

## Slice cycle

Every slice uses the same seven checkpoints:

1. **Contract lock:** specify observable behavior, typed request/result/events,
   security rules, fixtures and migration compatibility.
2. **Backend red:** add the smallest failing Rust integration test through the real
   `Runtime` or JSON-lines runner.
3. **Backend green:** implement the behavior behind existing contracts and adapters.
4. **Fixture handoff:** commit sanitized snapshots/event streams for mobile and eval.
5. **Mobile integration:** render all success, progress, empty, denial, cancellation
   and recovery states at 390x844.
6. **Evaluation:** run exact assertions first, then applicable DeepEval quality
   metrics. Live jobs write measured reports without changing fixtures.
7. **Rendezvous:** run the integrated checks, inspect the diff, record evidence and
   update the slice row in `TASK_INDEX.md`.

If a checkpoint fails, work returns to the owner of the failing layer. A judge score
cannot override a deterministic safety failure. A mocked result cannot satisfy a live
provider or physical-device gate.

## Vertical slices

### VS00 — reproducible development and evaluation setup

**User-visible proof:** none; this is the one foundation slice allowed before feature
slices.

Backend first:

- verify repository-local Rust, Node 24, JDK, Android SDK/NDK and Cargo Android target;
- pin a Python evaluation environment and DeepEval version outside the APK;
- make `scripts/check.ps1` run deterministic backend/frontend checks;
- make a separate eval command build the Rust runner before starting Python;
- record tool versions and blocked components without treating them as passes.

Mobile in parallel: inventory current screens at 390x844 and create state fixtures for
streaming, auth, approval, offline, error and structured outputs. No visual redesign
may invent backend fields.

Evaluation in parallel: establish dataset schema, report schema and deterministic vs
live job separation. Do not require a paid judge or provider for pull-request checks.

Gate: a clean checkout can run deterministic checks using documented commands; the
DeepEval smoke drives the real JSON-lines binary with a scripted provider; secrets do
not appear in outputs.

### VS01 — common execution stream and structured output

**Demo:** submit a local fixture request, watch text and execution events stream, stop
it, then render the final Markdown/list/table output from the same persisted run.

Backend locks `RunEventEnvelope` ordering, event IDs, run/worker IDs, terminal events,
reconnect cursor and `aethra.output.v1` blocks. Rust validates blocks and persists the
authoritative stream. Model/provider deltas are normalized before leaving adapters.

Mobile renders conversation separately from an expandable execution panel. It handles
text deltas, tool/worker progress, block upserts, reconnect deduplication, cancellation
and reduced motion.

Evaluation asserts ordering, exactly one terminal event, cancellation of late deltas,
schema validity and reconnection without duplication. DeepEval is not used for these
deterministic protocol properties.

Gate: the JSON-lines runner and browser bridge emit identical normalized events for
the same fixture, and the rendered UI passes state tests plus visual review.

### VS02 — public Parallel search with cited evidence

**Demo:** ask a current factual question; AETHRA selects only `web.search`/`web.open`,
streams research progress and renders a cited response with source cards.

Backend adds the versioned MCP connection/transport/auth manifest, connects explicitly
to Parallel's anonymous Search MCP over Streamable HTTP, normalizes `web_search` and
`web_fetch`, validates structured content and stores raw results outside model context.
Only bounded excerpts and source IDs reach the planner.

Mobile provides a Parallel Search preset clearly labelled anonymous and shows query,
source, retrieval time, partial failure and unsourced-result states.

Evaluation uses recorded MCP fixtures in pull requests and a scheduled/manual live
smoke for endpoint drift. Exact source/result assertions precede answer relevance and
faithfulness metrics.

Gate: one real live query is recorded with citations and no credential; malformed,
oversized and injected results fail safely.

### VS03 — authenticated MCP connections

**Demo:** connect Parallel Task through OAuth or an endpoint-bound bearer credential,
deny once, reconnect, resume the same task and complete a read-only tool call.

Backend implements secret references, connection-origin binding, OAuth discovery,
Authorization Code with PKCE, preregistration/client metadata/DCR selection, refresh,
step-up scopes and one bounded resume. Static bearer and reviewed API-key headers use
the same secret resolver. Android tokens use Keystore; service credentials remain on
the server. stdio environment credentials are injected only into approved host child
processes.

Mobile asks for the transport and auth profile explicitly, opens OAuth in a Custom
Tab, displays scopes and states, and never receives a token from Rust.

Evaluation covers denial, bad state, wrong redirect, expired/rotated token, changed
endpoint, insufficient scope, revoked access, secret redaction and resume-once.

Gate: live Parallel OAuth and bearer Task connections each list tools; all discovered
tools remain disabled until reviewed. Unknown auth schemes fail closed.

### VS04 — bounded graph and parallel workers

**Demo:** one research request launches independent search/open workers, streams their
states concurrently, tolerates one read-only failure and produces one final output.

Backend implements the smallest durable DAG needed: typed node dependencies, leases,
budgets, cancellation propagation, idempotency keys, result references and join rules.
The cloud planner proposes a graph; Rust validates and schedules it. Needle receives
only a single bounded local action packet, never the full graph or chat history.

Mobile renders a compact active-run summary and an expandable graph/activity view with
running, waiting, failed, cancelled and completed workers.

Evaluation asserts dependency order, maximum concurrency, time/step budgets, partial
failure policy, deterministic joins, restart recovery and no duplicate external write.
DeepEval scores plan quality, adherence, task completion and step efficiency only on
captured real trajectories.

Gate: two read-only workers run concurrently through the real runtime, cancellation
stops descendants, and restart resumes only safe/idempotent nodes.

### VS05 — rich Parallel research and monitoring

**Demo:** run a deeper cited company research task, render Markdown/table/chart blocks,
then create an approved monitor whose signed event becomes a notification.

Backend adds a vendor-contained Parallel adapter for `/v1/search`, `/v1/extract`, Task,
FindAll and Monitor. Long tasks persist provider run IDs and complete through verified,
deduplicated server webhooks. Chart blocks reference validated numeric datasets.

Mobile renders citations, tables, charts, timelines, progress, cost/limit disclosure
and monitor controls. It distinguishes market-data quotes from web/news monitoring.

Evaluation covers source attribution, contradictory evidence, webhook signatures,
duplicates, out-of-order events, stale data, polling fallback and structured rendering.

Gate: a live Task result and a signed webhook fixture pass. Monitor creation requires
explicit approval and can be paused/deleted deterministically.

### VS06 — supervised phone action worker

**Demo:** resolve a user-selected contact candidate, draft an SMS, show exact approval,
then use Needle to execute the bounded on-device action and report the observed result.

Backend/Kotlin add native contacts, intents, notification access and bounded
Accessibility operations behind `ToolSpec`. Policy binds approval to recipient,
message, app, session and screen observation. An uncertain send never retries.

Mobile presents contact ambiguity, draft provenance, action progress, Stop, exact
approval and uncertain-outcome recovery.

Evaluation keeps drafting quality separate from tool/action correctness. Host fixtures
cover stale targets and policy; only a physical device can pass Android execution,
latency, RAM and compatibility gates.

Gate: deterministic safety cases are 100%, real Needle output selects valid tools, and
the user records one controlled device test with logs.

### VS07 — voice and document inputs

**Demo:** push to talk, interrupt/correct the transcript, attach a document, ask a
sourced question and optionally continue into an approved action.

Backend normalizes speech and document extraction into existing input/source records.
Android owns microphone permissions and its user-started foreground service. Document
parsers are bounded and sandboxed; extracted text is untrusted evidence.

Mobile supports keyboard-safe push-to-talk states, transcript correction, attachment
progress, source inspection and text fallback. No microphone control appears when the
speech adapter is unavailable.

Evaluation measures transcription separately, checks document grounding/injection,
and verifies cancellation, permission denial, corrupt files and memory limits.

Gate: one real voice turn and one supported document work on device; offline support
is reported from measurement rather than assumed.

### VS08 — durable personal workflows

**Demo:** turn a vague request into a reviewable scheduled workflow, run read-only
checks in the background and notify the user; every write remains separately approved.

Backend stores a versioned workflow graph, trigger, timezone, budget, capability
leases and last-run cursor. Android uses WorkManager for appropriate device work; the
server scheduler owns always-on tasks, webhooks, stdio and service credentials.

Mobile provides natural-language setup plus a deterministic review screen for trigger,
data shared, tools, notification rule, cost limits, pause and delete.

Evaluation uses a virtual clock for schedules, duplicate events, timezone/DST,
network loss, revoked auth, quiet hours and restart recovery. Live provider schedules
run only in isolated accounts.

Gate: a read-only scheduled workflow survives restart and deduplicates notification;
all external writes still stop at exact approval.

### VS09 — security, compatibility and signed release

**Demo:** install a signed APK on Android 12+ ARM64 and complete conversation, cited
research, authenticated connection and one supervised phone action.

Run dependency/license inventory, secret and prompt-injection tests, MCP redirect/token
binding tests, Android permission review, memory/thermal benchmarks and recovery under
process death. Test 6 GB first and report 4 GB separately.

Gate: format, lint, tests, deterministic evals, configured live evals, rendered mobile
QA and physical-device evidence all pass. Deliver APK checksum, build inputs, install
steps, limitations and redacted reports.

## Backend-first setup order

The first implementation session completes VS00 and locks VS01 contracts before any
feature UI changes:

1. Run and record the existing Rust checks and JSON-lines fixture.
2. Restore Node 24 and run frontend checks; environment failure remains a blocker.
3. Create a pinned evaluation virtual environment/lock file and install DeepEval only
   there.
4. Add an eval adapter that launches the compiled Rust runner; never recreate the
   assistant in Python.
5. Add deterministic and live eval commands with separate output directories.
6. Add CI only after both commands pass locally; pull requests run deterministic jobs,
   while manual/scheduled jobs use protected secrets for judge/provider access.
7. Freeze VS01 event/output JSON fixtures, then release the mobile and eval lanes.

## Medium-model work packet

The coordinator gives each worker one self-contained packet:

```text
Slice and checkpoint:
Observable behavior:
Current evidence and exact gap:
Files owned (exclusive):
Files read-only:
Locked request/result/event schemas:
Ordered implementation steps:
Exact tests to add:
Commands to run:
Acceptance evidence required:
Security invariants:
Non-goals:
Handoff destination:
```

Worker rules:

- Read `AGENTS.md` and the applicable skill before editing.
- Verify the stated gap in code; report a mismatch before changing the contract.
- Edit only owned files. Ask the coordinator for contract changes.
- Implement one checkpoint; do not scaffold future slices.
- Add the smallest meaningful regression test through the real boundary.
- Run the listed checks, inspect the diff and remove debug output.
- Return changed files, commands/results, evidence, remaining risks and the next
  consumer. Never report mocked, live-provider or physical-device evidence as each
  other.

Reusable worker and review prompts live in `codex-prompts/vertical-slice-worker.md`
and `codex-prompts/vertical-slice-reviewer.md`.

## Merge and rollback policy

Each slice lands through small checkpoint commits. Contract/migration, backend,
fixture, mobile and eval changes remain separable until rendezvous. A slice may be
disabled with one capability/config flag without rolling back persisted user data.
Migrations are additive and tested against an existing database. Provider failures
disable that provider connection; they do not replace the Rust task engine.

The coordinator advances a slice to `DONE` only after its ledger row contains:

- commit or patch identity and changed interfaces;
- deterministic command output;
- UI fixture/render evidence when applicable;
- DeepEval report identity when a quality metric applies;
- live-provider evidence when required;
- physical-device evidence when required;
- measured limitations and unresolved risks.

## Detailed playbooks

- `BACKEND_VERTICAL_SLICES.md` — crate/file ownership and backend acceptance work.
- `MOBILE_UI_VERTICAL_SLICES.md` — mobile states, fixtures and visual QA.
- `AUTOMATED_EVAL_WORKFLOW.md` — real-runner DeepEval integration and automation.
- `MCP_OAUTH_SECURITY.md` — transport, OAuth, credentials and Parallel specifics.
- `WEB_RESEARCH_CAPABILITY.md` — normalized search/source/output behavior.
