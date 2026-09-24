# Integration and Development

## Local browser development

From the repository root, run `npm run dev`. This starts the real Rust bridge on
`127.0.0.1:8787`, waits until it is ready, and starts Vite (normally
`http://127.0.0.1:1420/`; use the printed URL if that port is occupied). No Android
emulator is required. The first run can take several minutes to compile Rust.
The launcher uses the project-local Rust toolchain when available and inherits
provider secrets from the terminal environment without printing them.

Press Ctrl+C to stop the frontend and the backend started by this command.
A compatible bridge already running on port 8787 is reused and left running.
An incompatible service on that port produces an error instead of starting Vite.
`scripts/start-dev.ps1` delegates to the same launcher. For an intentionally
separate backend, `npm run dev:ui` starts only Vite; Tauri also keeps its existing
frontend-only workspace command.

Set the environment variable referenced by your cloud settings before starting
the launcher. Backend readiness does not verify NVIDIA authentication or model
availability. Never put the API key itself in the secret-reference field.

## Checks

VS00 was verified on 2026-09-13 with Rust/Cargo 1.98.1, Node 24.21.0,
JDK 22.0.2, Python 3.11.15, DeepEval 4.2.2, Android API 36, NDK
27.2.12479018 and ADB 37.0.1. Bootstrap pins Node 24.21.0 and verifies its
published SHA-256 before extraction. Use the project-local toolchain after
`scripts/bootstrap.ps1`:

```powershell
. .\scripts\env.ps1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm ci
npm run format:check
npm run lint
npm test
npm run build
```

The combined deterministic command is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check.ps1
```

The JSON-lines evaluator is the smallest end-to-end harness:

```powershell
Get-Content .\evals\fixtures\handoff.jsonl | cargo run -p assistant-cli
powershell -ExecutionPolicy Bypass -File .\evals\run_deterministic.ps1
python -m venv .\evals\.venv
.\evals\.venv\Scripts\python -m pip install -r .\evals\requirements-eval.txt
.\evals\.venv\Scripts\python -m evals.lint_datasets
powershell -ExecutionPolicy Bypass -File .\evals\run_deepeval_smoke.ps1
```

DeepEval is a host-only evaluator and is never linked into Rust, Tauri or the APK.
The VS00 smoke uses an exact offline metric but still invokes the real compiled
Rust runner with a scripted provider. Live quality evaluation stays blocked until
the runner supports named live profiles and an actual profile is configured.

The local UI bridge can also be started with `cargo run -p assistant-cli --bin assistant-dev` and accepts `POST /api/command` only from a client sending `X-Assistant-Client: local-ui`. `GET /api/health` returns only the bridge identity and readiness, never settings or credentials. Tauri uses the same `Runtime::dispatch` interface.

## Run events and structured output

Action tasks now persist versioned `aethra.run-event.v1` envelopes. Each event has a
stable `event_id`, monotonic `sequence`, `run_id`, optional `worker_id`, typed kind,
task status, step, optional `text_delta` and optional validated output. Provider calls
emit worker start/terminal events. OpenAI Responses and compatible Chat Completions
SSE text fragments are normalized into bounded `text_delta` events as bytes arrive;
streamed tool arguments remain inside the adapter until they form one validated action.
Call `run_events {after}` to receive
a bounded page with a reconnect cursor. `has_more` tells the client to continue paging;
`reset_required` tells it that its cursor is ahead of, or has fallen behind, the durable
stream and that it must replace its cached events with the returned page. Each envelope
keeps `task_id` as a compatibility alias of `run_id` while older clients migrate.
The page uses the `aethra.run-events.v1` schema; pass its `next_after` value on the next
poll. The existing `events` command returns the same envelopes as a compatibility
surface.

## Governed adaptive rules

Adaptive behavior is persisted as versioned `aethra.adaptive-rule.v1` records. A
model can submit an `assistant_control` `propose_rule` action, but Rust validates
and stores it as `proposed`; it cannot enable the rule or change tools,
permissions, policy, or secrets. The Settings → Adaptive Rules screen is the
explicit review gate. Only enabled, non-expired rules matching the current
conversation/workflow are retrieved (maximum eight) and sent to providers as
user-approved preferences. Rules can be rejected, disabled, superseded, and
audited through their evidence IDs and version history.

Completed text is normalized to an `aethra.output.v1` Markdown block. Scripted or
future provider adapters may return `respond_structured` with bounded Markdown, list
and table blocks. Web runs can also add a validated sources block with source IDs,
HTTPS locators, bounded excerpts, retrieval time and partial-result state. Rust rejects unknown schemas, duplicate/empty block IDs, oversized
content and ragged tables before completion. React renders these typed blocks without
injecting HTML and shows an expandable ordered execution list.

`evals/fixtures/streaming-output.jsonl` exercises worker and text-delta ordering through
the real Rust runner. `verify_bridge_parity.py` sends that fixture through both the
JSON-lines executable and the loopback browser bridge, normalizes generated IDs, and
requires identical events. The fixture endpoint is denied unless the development
process explicitly sets `ASSISTANT_DEV_FIXTURE_MODE=1`.

## Cloud provider

Settings now accepts an actual API key through a masked, session-only field.
`save_cloud_provider` accepts `{ cloud: CloudConfig, api_key: string | null }`.
Provider settings alone are persisted; the key is held in Rust memory and supplied
through `SecretStore`. A null key preserves an existing session key. Empty,
oversized, or whitespace-containing keys are rejected. Keys are never returned
through snapshots, events, or command responses. `clear_cloud_key` removes the
session key; backend restart also clears it. Session keys are bound to the exact
configured endpoint and credential reference, so editing the endpoint cannot
forward an existing session key to a different server.

The advanced credential-reference field still accepts an environment variable
name (for example `ASSISTANT_CLOUD_KEY`), never a key value. The environment is
the fallback when no matching session key exists. Environment changes require
restarting the backend. Native host credential persistence remains intentionally
session-only. Android connects OAuth token records to an AES-GCM Keystore plugin
boundary using opaque handles and binding metadata as associated data; tokens are
never stored in SQLite or returned to React.

The snapshot's `cloud_session_key` is a boolean and `cloud_credential` reports only
`not_configured`, `missing`, or `configured`, not successful remote authentication.
Authentication failures pause a task in `waiting_for_auth`; `resume_auth` retries
the same task after credentials are fixed. It does not bypass tool approvals.

Legacy invalid credential references are replaced with `ASSISTANT_CLOUD_KEY`
at startup, and new invalid references are rejected before persistence. Rotate
any real key previously pasted into that field: replacement does not guarantee
erasure from SQLite free pages, journals, backups, or earlier logs.

After capability retrieval, a tool-only local provider is bypassed when no tools
are available. The cloud model can answer directly or refine capability search.
Repeated identical searches trigger bounded recovery rather than silently
consuming the entire execution budget.

Configure a provider with `save_cloud_provider` for UI key entry, or `save_settings` for environment-based configuration. Both require an HTTPS endpoint, model ID, and a valid secret reference. Responses API and OpenAI-compatible Chat Completions responses are normalized to `AgentAction`.

The loopback bridge checks Host and Origin in addition to its custom client
header, rejects cross-site/rebound hosts, and offers no CORS permissions. These
checks do not authenticate other processes running under the same local user;
the bridge must remain development-only and bound to loopback.

## Needle

Run `scripts/fetch-needle.ps1` to fetch the pinned Needle 2 distribution and verify checksums. Set `ASSISTANT_NEEDLE_LIBRARY` to the verified host DLL for the real local smoke test:

```powershell
$env:ASSISTANT_NEEDLE_LIBRARY = '.\.tools\needle2\windows\needle\libneedle.dll'
cargo run -p provider-needle --example smoke -- $env:ASSISTANT_NEEDLE_LIBRARY
```

The smoke test asks Needle to select `native.set_flashlight`; it does not execute a device action. Android uses the pinned static archive. The Tauri Android project is initialized and the repository-local SDK/NDK are present. `scripts/android-preflight.ps1 -Sdk .tools/android-sdk` currently reports no authorized physical Android 12+ ARM64 device, so current device and release acceptance remain `not_run`.

## Dynamic capabilities

Use `connect_mcp` with an HTTPS MCP URL. Discovered tools are disabled by default, indexed in the same FTS5 catalogue as skills, and must be explicitly enabled. Use `save_capability` for a validated skill manifest containing instructions and tool IDs. A changed tool schema increments its version and disables the tool until reviewed.

MCP connections use the versioned `aethra.mcp-connection.v1` manifest. VS02 supports
anonymous Streamable HTTP. The VS03 foundation also accepts tagged `bearer_token`,
`api_key_header`, and `oauth_authorization_code` profiles. Static credential values
are resolved from endpoint-bound session storage and never serialized. The host OAuth
flow is implemented, and Android launches authorization in a Custom Tab, captures
only an exact HTTPS App Link callback, and restores bound token records through the
Keystore boundary. Production callback-domain verification and additional transports
remain pending.
`connect_parallel_search {}` installs the reviewed anonymous preset for
`https://search.parallel.ai/mcp`, discovers only `web_search` and `web_fetch`, and
normalizes them to `web.search` and `web.open`. The tools remain disabled until the
user enables them in Settings.

For a static credential, call `connect_mcp_with_credential` with a `connection`
manifest and separate `secret`. The mobile connection form does this for Bearer and
the reviewed `x-api-key` header. `save_mcp_credential` and `clear_mcp_credential`
change session-only values. `mcp_connection_status` returns a redacted
`aethra.connection-state.v1` value with connection state, requested/granted scope
names, expiry and resumable-task metadata, but no credential values. The adapter
resolves the exact connection/origin/resource binding before network I/O and does
not follow HTTP redirects.

Web tool calls persist an `aethra.tool-result.v1` record. Its `raw` field retains the
bounded MCP response for audit, while only the separately bounded `model_context`
field can enter provider context. Source excerpts are explicitly named
`untrusted_excerpt`; instructions inside them cannot change application policy. The
core asks models to cite source IDs as `[source:ID]` and deterministically appends the
matching source cards to completed output.

Run the live anonymous protocol smoke without a model credential:

```powershell
. .\scripts\env.ps1
cargo run -p adapter-mcp --example parallel_search_smoke -- "What did Parallel announce for Search MCP in April 2026?"
```

The dated 2026-09-14 result is in
`artifacts/evals/vs02-parallel-live-2026-09-14.json`. It proves live MCP discovery,
execution and source normalization, not cloud-model answer quality.

Android release builds require the verified App Link host and callback path to be
provided as Gradle properties; the placeholder host is rejected for release:

```powershell
.\gradlew.bat assembleRelease -PoauthRedirectHost=assistant.example.com -PoauthRedirectPath=/oauth/callback
```

The corresponding `https://assistant.example.com/oauth/callback` association must be
published in `assetlinks.json` before device acceptance.

Run the OAuth metadata-only smoke without starting authorization:

```powershell
. .\scripts\env.ps1
cargo run -p adapter-mcp --example parallel_oauth_discovery
```

The 2026-09-14 result is in
`artifacts/evals/vs03-parallel-oauth-discovery-2026-09-14.json` (SHA-256
`C4BAD2EF0EC36754322DB9805BB60051009D65BD9CCFE13A2953B97C1FC27A82`).
It proves live RFC metadata discovery and validation only. It does not prove user
consent, token exchange, authenticated MCP calls, refresh, or Android storage.

## IPC commands

`submit_input`, `run_task`, `cancel_task`, `resume_auth`, `resolve_approval`, `answer_question`, `save_settings`, `save_cloud_provider`, `clear_cloud_key`, `save_capability`, `save_mcp_connection`, `connect_mcp`, `connect_mcp_with_credential`, `save_mcp_credential`, `clear_mcp_credential`, `mcp_connection_status`, `start_mcp_oauth`, `complete_mcp_oauth`, `cancel_mcp_oauth`, `connect_parallel_search`, `disconnect_mcp`, `snapshot`, `events`, and `run_events` are the runtime command names. Input can be marked `source: "voice"` today; speech capture and wake-word services are deferred behind this boundary.

`WorkGraph`, `WorkNode`, `WorkEdge`, `WorkerRequest`, and `WorkerOutcome` are the
VS04 persistence boundary. Models may emit only a bounded `WorkGraphProposal`; Rust
validates it and assigns task/node/worker IDs, deadlines, retry limits and
idempotency keys. Worker packets are capped at 64 KiB and eight selected tools. The
scheduler runs at most two ready workers concurrently, revalidates enabled read-only
tools immediately before execution, persists every transition and joins results in
stable graph order. Mutations remain in the serialized `Assistant`/`PolicyEngine`
path. Cancellation aborts active work and cancels descendants; safe unfinished
nodes are recovered at runtime startup, including graphs beyond the UI task-page
limit. Unsafe, expired, cancelled, approval-paused and write-capable work is left
for explicit resolution rather than replayed.

## Acceptance evidence

Host evidence is recorded by `cargo test --workspace` and `npm test`. Anonymous
Parallel Search MCP was verified live on 2026-09-14. Deterministic graph evidence is
in `evals/fixtures/parallel-workers.jsonl`. Physical Android, OAuth, authenticated
MCP, cloud-model synthesis, and signed APK evidence must be appended here with
device/version and timestamp before those tasks are marked done.

## Offline companion commands

Conversation is now the default screen. The explicit Action mode retains the existing
Needle/cloud task engine and its approvals. Ordinary `send_message` requests call
only the configured local conversation adapter. Missing models return setup/unavailable
and do not submit an action task or fall back to cloud.

`send_message {conversation_id,text,temporary?}` returns an assistant message in
`generating` state immediately. Poll `get_conversation {conversation_id}` for ordered
messages and partial text; `cancel_message {conversation_id}` stops that generation.
`list_conversations {}` lists history; `delete_conversation {conversation_id}` removes
its messages and research sessions (an active generation must stop first). Persistent
unfinished replies become `interrupted` on startup. Temporary chats live only in the
Rust process, do not retrieve personal memory and are lost when it exits.

`list_memories {}`, `save_memory {id?,text,confirmed:true}` and `delete_memory {id}`
provide explicit user-owned memory. Save without confirmation is denied. Known
configured credentials are rejected from new conversation, memory and research input.
`save_summary {conversation_id,text}` stores a user-edited bounded summary. The prompt
prioritizes the latest user message, then recent completed turns, summary, relevant
confirmed memory and bounded research excerpts. The native adapter owns the exact
4,096-token limit; raw tool-result records are never inserted into this path.

Research commands: `create_research {conversation_id,title}`, `list_research
{conversation_id}`, `get_research {id}`, `delete_research {id}`,
`append_research_note {id,text}`, `add_research_source {id,title,url?,excerpt}` and
`update_research_reasoning {id,hypotheses?,decisions?,experiments?}`. Notes append revisions.
Sources added through these commands are explicit user-shared excerpts. A stored URL
is a locator, not evidence that the app fetched it. Action-mode `web.search` and
`web.open` results are separately persisted evidence and rendered as source cards.
Automatic document ingestion remains open; discussion without excerpts must be
described as unsourced reasoning.

## Local model setup and verification

`model_status {}` returns provider availability, installation and pinned manifest.
`install_model {}` starts the disclosed download; poll status for bytes and verification.
`cancel_model_download {}`, `unload_model {}` and `remove_model {}` control lifecycle.
No download occurs at startup. Models live beside the database in `models/` and are
excluded from source control. Existing settings and session cloud keys are preserved.

The default host adapter runs a separately installed llama.cpp executable named by
`ASSISTANT_LLAMA_CLI`. Android enables the `native-local-chat` feature, which selects
the in-process native shim; that shim must be built and packaged for ARM64. The host
CLI alone does not satisfy Android acceptance. See `vendor/local-chat/` and
`scripts/fetch-local-chat.ps1` for pinned runtime build support.

Real dispatch smoke (requires the verified model and native runtime installed first):

```powershell
cargo run -p app-runtime --example companion_smoke -- assistant.db
cargo run -p app-runtime --example companion_smoke -- assistant.db --cancel
```

The smoke uses temporary synthetic conversation, never phone actions or cloud. It
reports observed first text with a 100 ms polling interval, not exact first-token or
tokens/sec metrics. Missing artifacts produce a nonzero blocked result. To test the
in-process adapter, add `--features native-local-chat` and configure the shim link path.

`evals/companion-v1.jsonl` is the 60-scenario acceptance catalog. `evals/README.md`
distinguishes deterministic regressions from model-quality measurements. Android
preflight and physical-device instructions are in `ANDROID_FEASIBILITY.md`.

## Governed personalization

Personalization is application-owned state, not a model prompt cache. `adaptive_rules`
holds the current rule, while immutable `personal_revisions` records every proposal,
review, disable, replacement and rollback. Rules are retrieved before inference with
scope, expiry, priority and size limits; temporary conversations receive neither
rules nor personal memories. Exact rule revisions used by a response/action are
recorded in `personal_usage`.

The runtime exposes `remember_preference`, `record_observation`,
`list_adaptive_rules`, `list_rule_proposals`, `review_rule_proposal`,
`disable_adaptive_rule`, `personal_rule_details`, `personal_rule_history`,
`rollback_adaptive_rule`, `propose_skill`, and `evaluate_personal_skill`.
Explicit remembered preferences require confirmation and can activate immediately.
Inferred preferences and model suggestions remain proposals until reviewed. A deleted
conversation removes derived observations and preferences.

Generated skills are declarative capability contracts, never executable scripts.
They stay inactive until replay evaluation compares the candidate against the
baseline using recorded/simulated tool data. Activation also rechecks each captured
tool binding; a changed or disabled dependency automatically disables the skill.
Rules and generated skills cannot grant permissions, enable a disabled tool, or
override `PolicyEngine` approval requirements.
