# Athera personal-assistant build plan

Status: **authoritative product and implementation plan**

The dependency-aware implementation ledger is
[`ATHERA_DEVELOPMENT_TASK_GRAPH.md`](ATHERA_DEVELOPMENT_TASK_GRAPH.md).
The latest evidence-based status reconciliation is
[`ATHERA_STATUS_AUDIT_2026-09-23.md`](ATHERA_STATUS_AUDIT_2026-09-23.md).

This plan supersedes the large-local-chat-model-first product plan. Athera remains
local-first through Needle and deterministic on-device software. Existing code is
retained only where it supports this architecture and passes its acceptance gates.

## 1. Product contract

Athera is a persistent personal assistant, not a chat client with a separate action
mode. A user gives it an outcome in one composer. Athera interprets the request,
uses relevant personal context, creates a durable commitment when necessary, acts
within explicit authority, verifies the outcome, follows up, and learns only
reviewable preferences.

The user never chooses a model, mode, agent, tool, or workflow for an ordinary
request.

### Non-negotiable decisions

1. The default product is **Needle-local-first and cloud-provider-neutral**. Local
   task state, policy, memory, capability retrieval and supported intent routing do
   not depend on a cloud provider.
2. Local-first means Needle, not a second large local chat model. A large offline
   model is optional. It is never required, auto-downloaded, or the
   primary recovery action.
3. Every input enters deterministic local intake. Needle handles work within its
   measured and declared capabilities and requests a cloud handoff when stronger
   language or planning is required.
4. Rust owns tasks, routing, policy, retries, verification, persistence, and
   preference rules.
5. Models propose meaning and actions. They cannot authorize or directly execute
   side effects.
6. React presents authoritative Rust state. It does not call model, MCP, or Android
   capability SDKs.
7. Kotlin owns Android lifecycle integration, permissions, WorkManager, foreground
   services, notifications, and Keystore operations.
8. Secrets never enter React persistence, normal SQLite columns, model context,
   traces, or fixtures.
9. Background operation is durable and event-driven. Athera does not depend on an
   immortal process.
10. No new capability work begins until a real cloud conversation works end to end
    in the installed APK.

## 2. One assistant workflow

Every input enters the same runtime:

```text
text / share / notification / schedule / future voice
                         |
                   normalize input
                         |
             load bounded relevant context
                         |
               derive task requirements
                         |
             choose an execution strategy
       direct Rust | Needle | cloud | optional offline
                         |
               create or update a task
                         |
                propose next action
                         |
                    policy check
                         |
        execute / approve / clarify / wait / deny
                         |
                  observe and verify
                         |
                report and follow up
```

There is no Conversation mode and no Action mode. Short answers may complete in one
step. Ongoing requests become durable jobs using the same state machine.

## 3. Target dependency structure

Keep the existing crate boundaries where they are sound. Add only one production
crate for Rig; do not create a framework of tiny crates.

```text
apps/mobile React
      |
      | typed Tauri IPC
      v
apps/mobile/src-tauri       Kotlin Android integration
      |                         |
      +-----------+-------------+
                  v
            app-runtime
        composition + use cases
                  |
                  v
           assistant-core
  orchestration, routing, policy, context,
       verification, preference decisions
                  |
                  v
       assistant-contracts
          typed stable boundaries

Adapters depend inward on contracts:

storage-sqlite   provider-rig   provider-needle   adapter-mcp
```

Dependency rules:

- `assistant-contracts` depends on no concrete provider, database, UI, Tauri, or
  Android library.
- `assistant-core` depends only on contracts and general-purpose libraries.
- Concrete adapters implement contracts and never leak vendor objects inward.
- `app-runtime` is the composition root. It constructs adapters and exposes use
  cases to thin Tauri commands and evaluation tooling.
- The mobile shell never constructs a second assistant engine.

## 4. Module restructuring

### `crates/contracts`

Split the current broad `lib.rs` by domain while retaining compatible re-exports
during migration:

```text
src/
  input.rs          AssistantInput, InputSource, AttachmentRef
  task.rs           Task, TaskState, TaskRequirements, Commitment
  blocker.rs        TaskBlocker and typed recovery payloads
  action.rs         ActionProposal, PendingAction, ExecutionReceipt
  provider.rs       ModelRequest, ModelResponse, ModelEvent, ModelProvider
  capability.rs     ToolSpec, SkillSpec, capability references
  policy.rs         Risk, AuthorityGrant, ApprovalDecision
  trigger.rs        schedule/event/follow-up triggers
  events.rs         versioned public task events
  platform.rs       native request/outcome contracts
  error.rs          normalized boundary errors
```

Contracts contain types and traits, not orchestration decisions.

### `crates/assistant-core`

Restructure around the personal-assistant loop:

```text
src/
  assistant.rs      public Assistant use cases
  intake.rs         normalize input and resolve immediate context
  requirements.rs   deterministic task requirements
  router.rs         choose direct/Needle/cloud/offline strategy
  orchestrator.rs   advance one durable task transition
  policy.rs         authority and approval checks
  context.rs        bounded model/capability/personal context
  executor.rs       validated capability dispatch
  verifier.rs       confirm observed outcome
  recovery.rs       retry, cancellation and outcome-unknown rules
  preferences.rs    propose/apply scoped preference rules
```

The existing graph scheduler is reused for genuine multi-step jobs. A simple answer
does not need a graph. SMS-specific policy and experiment code must move behind the
normal capability boundary or remain an isolated experiment.

### `crates/provider-rig` — new

This is the only new production crate in the first restructuring phase. It provides
model transport; Athera still owns provider profiles, authentication choices,
credentials, connection state and routing.

Responsibilities:

- Implement Athera's `ModelProvider` contract using Rig.
- Support every provider exposed by the pinned Rig release after its transport,
  authentication and Android behavior pass Athera's provider conformance suite.
- Support configurable OpenAI-compatible and Anthropic-compatible endpoints.
- Normalize streaming, tool calls, usage, cancellation, and provider errors.
- Resolve only opaque secret references supplied by the runtime.
- Expose a real connection test using the same client construction as inference.
- Keep every Rig type inside this crate.

Start with the smallest Rig feature set that satisfies the provider spike. Pin the
exact pre-1.0 version. Do not use `rig-agent` as Athera's top-level runtime.

Provider availability is catalog-driven, not assumed from Rig's compile-time
presence. Each provider profile declares supported protocols, model capabilities,
endpoint fields, authentication methods and Android support. React renders those
descriptors and contains no provider-specific branches.

The spike passes only if it:

1. Compiles for `aarch64-linux-android`.
2. Calls an NVIDIA OpenAI-compatible endpoint successfully.
3. Supports a custom endpoint and model ID.
4. Streams and cancels without leaking a task.
5. Maps 401, 403, 404/model, 429, timeout, invalid response, and network failures.
6. Does not obtain credentials from frontend state or environment variables on
   Android.
7. Has an acceptable measured APK and dependency-size impact.

After parity, remove `provider-cloud` from `app-runtime`; delete the crate only after
its tests have equivalents in `provider-rig`.

### `crates/provider-needle`

Needle may return only bounded outcomes:

- a supported structured action proposal;
- a small extraction/classification result;
- a request for stronger reasoning;
- unavailable.

Needle failure never makes the assistant input unusable. It is bypassed for ordinary
conversation and planning when a configured cloud responder is the correct route.

### `crates/provider-local-chat`

Remove it from the default Android build and normal onboarding. Keep it behind an
explicit experimental feature until there is measured demand. No UI should mention
downloading an offline model unless the user opens an optional Offline Pack setting.

### `crates/storage-sqlite`

Organize repositories around durable product objects:

```text
src/
  tasks.rs
  task_events.rs
  actions.rs
  triggers.rs
  conversations.rs
  results.rs
  capabilities.rs
  connections.rs     metadata and opaque secret refs only
  preferences.rs
  migrations.rs
```

Existing data is migrated additively. Do not wipe the development database as part
of the restructuring.

### `crates/app-runtime`

Reduce `lib.rs` to composition and a small public API:

```text
src/
  lib.rs
  composition.rs
  assistant.rs       submit, clarify, cancel, inspect
  connections.rs     configure, test, revoke
  approvals.rs
  jobs.rs            schedule, resume, reconcile
  preferences.rs
```

Provider selection, credential state, and retry rules are runtime/core concerns,
not Tauri-command logic.

### Android/Tauri shell

```text
apps/mobile/src-tauri/src/
  lib.rs             register thin commands
  commands/          serialization and runtime calls only
  android/           Rust side of typed native bridge

Android Kotlin package:
  secrets/           Keystore-backed secret vault
  jobs/              WorkManager worker and foreground-job service
  notifications/     progress, approval and completion notifications
  capabilities/      contacts/intents/other native adapters
```

WorkManager wakes eligible durable jobs. A foreground service is used only for
visible user-started work that must continue immediately. Kotlin reports platform
facts; Rust decides task and policy transitions.

### React mobile app

```text
src/
  app/               navigation and app shell
  features/assistant one composer and response/task timeline
  features/jobs      active commitments and history
  features/approval  exact-action confirmation
  features/connections provider and service health/setup
  features/preferences inspectable rules and corrections
  lib/ipc.ts         the only Tauri IPC wrapper
  types/             generated or checked Rust payload shapes
```

Remove the mode switch. Diagnostics, event counts, workers, endpoints, and build
details belong in a developer screen.

## 5. Authoritative task model

`TaskState` should describe lifecycle, while `TaskBlocker` explains why progress is
paused.

```text
Draft -> Ready -> Running -> Verifying -> Completed
                   |             |
                   |             +-> OutcomeUnknown
                   |
                   +-> Blocked(TaskBlocker)
                   +-> Scheduled
                   +-> Failed
                   +-> Cancelled
```

Required blocker types:

- `ProviderCredentialRequired { provider_id }`
- `ConnectorAuthorizationRequired { connection_id, scopes }`
- `AndroidPermissionRequired { permission }`
- `ApprovalRequired { proposal_id, exact_action }`
- `ClarificationRequired { question, candidates }`
- `DeviceConstraint { network, battery, foreground, storage }`
- `CapabilityUnavailable { capability_id }`

Do not add another generic `WaitingForAuth` path.

## 6. Model routing

Routing is deterministic software informed by typed task requirements:

- Direct Rust: known commands, status, cancellation, settings, scheduled triggers.
- Needle: one supported familiar action or small structured extraction.
- Cloud responder: conversation, drafting, summarization, normal ambiguity.
- Cloud planner: multi-step goals and plan repair.
- Optional offline pack: user-enabled privacy/resilience path only.

The router records why a strategy was selected, the provider used, data-egress
class, latency, and normalized failure. Provider failover never widens permissions
or replays an uncertain external write.

If no cloud provider is configured, Athera may still execute deterministic and
Needle-supported actions. It must clearly state when richer language reasoning needs
a provider. It must not redirect the user to a one-gigabyte download.

## 7. Connection and secret lifecycle

Provider configuration and provider credentials are separate:

- SQLite stores provider ID, endpoint, model, capabilities, health, and opaque
  credential reference.
- Android Keystore stores the API key encrypted until it expires, is revoked, or the
  user deletes it.
- `Test connection` performs a real minimal inference using production client code.
- Saving a provider does not claim it is healthy.
- UI shows `untested`, `testing`, `ready`, `invalid credential`, `model unavailable`,
  `rate limited`, `network unavailable`, or `provider error` with a repair action.
- Provider credentials and service OAuth use different state and recovery paths.

Provider and auth setup is descriptor-driven. Rust publishes trusted provider,
configuration-field and auth-option descriptors; React renders them generically.
Anthropic direct, Bedrock, Vertex and Azure are separate hosting profiles even when
they expose the same model family. Exact tasks, auth patterns and parallel provider
batches are defined in `ATHERA_DEVELOPMENT_TASK_GRAPH.md`.

## 8. Delivery phases and gates

No phase is complete based only on source code, mocks, or a successful build.

### Phase 0 — reset the baseline

Goal: make the installed artifact trustworthy.

- Declare this document authoritative and archive conflicting large-local-chat-first
  plans without changing Athera's Needle-local-first principle.
- Add a visible build/version fingerprint to developer diagnostics.
- Package exactly one current frontend bundle.
- Remove the offline-model prompt and mode switch from the product path.
- Freeze new MCP, research, adaptive-skill, and broad automation work.

Gate: the APK on the connected device shows the expected fingerprint and current UI.

### Phase 1 — prove real inference first

Goal: fresh install to real response without a local model.

- Run the Rig Android compatibility spike.
- Add `provider-rig` and NVIDIA OpenAI-compatible support.
- Implement Keystore-backed persistent provider credentials.
- Implement configure, save, test, revoke, and normalized connection errors.
- Route ordinary conversation directly to the configured cloud responder.

Gate on the physical device:

1. Install a clean APK.
2. Configure endpoint, model, and temporary NVIDIA key.
3. Test connection and see the actual success/model response.
4. Restart the app and confirm the credential remains usable.
5. Send `hi` and receive a real model response.
6. Revoke/delete the key and confirm the next request shows
   `ProviderCredentialRequired`, not connector authentication.
7. Confirm that no offline-model download is requested anywhere in the flow.

Nothing else has priority until this gate passes.

### Phase 2 — one assistant and typed recovery

Goal: remove the product's internal modes.

- Introduce `TaskRequirements` and typed `TaskBlocker` contracts.
- Replace Conversation/Action selection with one input path.
- Allow direct, Needle, and cloud strategies behind the same task.
- Separate normal user recovery from developer diagnostics.
- Migrate current task/auth state without deleting stored tasks.

Gate: greeting, direct settings command, supported Needle action, cloud answer,
clarification, provider failure, connector auth, approval, cancel, and retry each
render the correct distinct state.

### Phase 3 — first complete action

Goal: prove request-to-verified-outcome, not generic phone control.

Implement one narrow vertical slice:

> “Message Arun that I will be late.”

- Contact lookup using native Android data.
- Ambiguity clarification.
- Exact message and recipient approval.
- SMS compose/send through the safest supported native path.
- Normalized receipt: completed, opened-for-confirmation, denied, unavailable, or
  outcome-unknown.
- No automatic retry after an uncertain send.

Gate: device tests cover one match, multiple matches, permission denied/revoked,
approval denied, cancellation, successful outcome, and uncertain outcome.

### Phase 4 — durable commitments

Goal: jobs survive the UI and process.

- Add triggers and commitment records.
- Bridge the Rust job ledger to WorkManager.
- Add progress, blocker, approval, and completion notifications.
- Add safe boot/process-death recovery.
- Replace the global task runner mutex with per-task leases and resource locks.

Gate: a reminder/follow-up job survives app closure and process restart, can be
cancelled, and does not duplicate an external action.

### Phase 5 — three useful jobs

Complete these before general app automation:

1. Capture, remind, and follow up.
2. Resolve a person and prepare/send a message using a supported channel.
3. Parse/summarize a shared document and save/share the result.

Each must pass request, clarification, approval, background recovery, cancellation,
verification, and explanation tests.

### Phase 6 — personal context and preferences

- Separate observations, confirmed personal facts, preferences, rules, and policy.
- Learn only from meaningful repeated signals and explicit corrections.
- Propose inferred rules for acceptance.
- Scope, version, expire, inspect, disable, and roll back rules.
- Never let a learned rule grant permissions or weaken policy.

Gate: accepted and rejected preference proposals change only their intended scope and
can be undone.

### Phase 7 — proactive and voice surfaces

Only after durable jobs are reliable:

- Contextual suggestions from commitments and deadlines.
- User-started foreground voice session.
- Speech-to-text and text-to-speech as adapters to the same input/task runtime.
- Optional wake/assistant-role research with explicit battery and privacy evidence.

Do not create a separate voice agent loop.

## 9. Parallel work structure

After Phase 0 freezes contracts, these streams can proceed in parallel:

| Stream | Scope | Depends on |
| --- | --- | --- |
| Provider | Rig spike, cloud adapter, normalized errors | Phase 0 |
| Secrets | Android Keystore and credential lifecycle | Phase 0 contracts |
| Core state | Task requirements, blockers, routing | Phase 0 contracts |
| Mobile UI | One composer and typed state rendering | Core payloads |
| Evaluation | Live-provider smoke and installed-APK proof | Provider/runtime |

Android background jobs begin after the task state and storage migrations stabilize.
Native action work begins after the real-inference gate so failures are attributable.

## 10. What is deliberately frozen

Until Phase 3 passes:

- No mandatory or promoted large local model.
- No multi-agent runtime.
- No general accessibility-driven app controller.
- No new MCP catalogue expansion.
- No self-writing skills enabled on device.
- No always-listening microphone.
- No UI redesign beyond the single-assistant and recovery states.

The existing code may remain behind developer flags, but it does not define the
product path.

## 11. Development acceptance rules

Every deliverable must include:

1. Typed contract and migration compatibility.
2. Deterministic unit/contract regression.
3. Real runtime integration test.
4. Live provider evidence when provider behavior is claimed.
5. Physical-device evidence when Android behavior is claimed.
6. Installed build fingerprint and screenshot/log reference.
7. Formatting, lint, tests, diff inspection, and debug-code removal.

“Implemented,” “builds,” and “mock passes” are not equivalent to “works on the
phone.”

## 12. Initial execution backlog

The next development iteration is intentionally short:

1. Mark old large-local-chat-first plans as superseded while retaining Needle as the
   local-first reflex.
2. Remove `provider-local-chat` from the default runtime/build path.
3. Remove the model-ready guard from the single input pipeline.
4. Add typed provider-connection states distinct from connector OAuth.
5. Complete the Rig Android/NVIDIA spike.
6. Add the Keystore-backed provider secret path.
7. Implement production `Test connection` through the same provider factory.
8. Route `hi` to the configured cloud responder.
9. Remove the mode switch and offline-download primary action.
10. Build, install, and record the Phase 1 device gate.

Only after item 10 passes should action capabilities resume.
