# Athera development task graph

Status: **authoritative execution order for the Jarvis restructuring**

Product architecture is defined in
[`ATHERA_JARVIS_BUILD_PLAN.md`](ATHERA_JARVIS_BUILD_PLAN.md). This document defines
the tasks, dependencies, parallel ownership and evidence gates. Older task lists are
retained as implementation history, not as the order for new development.

## 1. Fixed definitions

These are product decisions, not implementation guesses:

- **Local-first** means deterministic Rust plus Needle on the device. It does not
  mean a mandatory large local chat model.
- Every input is accepted through one assistant entry point.
- Needle handles only measured and declared local abilities. Stronger language,
  planning or provider-specific capabilities are handed to a configured provider.
- All cloud providers are replaceable adapters. No provider type enters core task,
  policy, storage or UI business logic.
- “All providers” means every model provider supported by the exact pinned Rig
  revision, plus generic OpenAI-compatible and Anthropic-compatible profiles. The
  inventory is generated and reviewed in task `PRV-001`; it is not inferred from a
  marketing list.
- A provider is not “supported” until its configuration, every advertised auth
  option, request/stream mapping, normalized errors and Android build behavior pass
  the provider conformance suite.
- React renders trusted typed descriptors returned by Rust. It contains no
  provider-name conditionals and never receives stored secret values.
- Hosting paths are distinct providers. Anthropic direct, Claude through Bedrock,
  Claude through Vertex and Claude through Azure do not share authentication just
  because they expose the same model family.

## 2. Provider and authentication model

### Provider catalog contract

`assistant-contracts` owns neutral descriptor types:

```text
ProviderDefinition
  id
  display_name
  transport_family
  capabilities
  endpoint_fields
  model_source
  auth_options[]
  availability
  documentation_url
  schema_version

AuthOptionSpec
  id
  label
  auth_kind
  fields[]
  expiry_behavior
  refresh_behavior
  android_support

ConfigFieldSpec
  id
  label
  kind: text | secret | select | boolean | url | integer
  required
  secret
  validation
  options
  visible_when[]
  help_text
```

`visible_when` is a small typed equality rule. Provider descriptors cannot execute
code, inject HTML, choose policy, or define arbitrary headers outside an adapter's
allowlist.

### Auth families

The catalog supports these neutral auth families:

| Auth family | Stored material | Runtime behavior |
| --- | --- | --- |
| None | Nothing | Self-hosted or explicitly anonymous endpoint |
| API key | One secret handle | Adapter places it in its fixed provider header/query location |
| Bearer token | Token handle and optional expiry | Adapter adds `Authorization: Bearer` |
| Composite static secret | Multiple secret handles | Access/secret/session-token or equivalent typed set |
| OAuth 2.0 PKCE | Refresh/access token handles and expiry | Deterministic authorization, refresh and revoke |
| Workload identity | Provider configuration and short-lived token handles | Exchange/refresh through the provider identity adapter |
| Cloud identity | Typed cloud credential source | AWS SigV4, Google identity or Azure Entra adapter |
| Custom compatible auth | Allowlisted header descriptors and secret handles | Only for user-defined compatible endpoints |

Android's Keystore holds encryption keys. Secret ciphertext and metadata are stored
in the Keystore-backed vault; plaintext is resolved only inside the native/runtime
boundary for the specific provider purpose.

### Required provider families

This is the minimum catalog shape. `PRV-001` expands it to the complete pinned-Rig
inventory without changing core contracts.

| Provider/hosting path | Auth options to represent | Conditional non-secret fields |
| --- | --- | --- |
| OpenAI direct | API key | organization, project, endpoint override |
| OpenAI-compatible | none, API key, bearer, allowlisted custom header | endpoint, route, model, optional headers |
| NVIDIA NIM | API key/bearer | endpoint, model |
| Anthropic direct | API key, bearer, workload identity | API version, optional workspace, endpoint override |
| Anthropic-compatible | API key, bearer, allowlisted custom header | endpoint, API version, model |
| AWS Bedrock | access/secret/session token, web identity/Cognito, supported AWS credential source | region, optional endpoint, model ID/profile |
| Gemini Developer API | authorization/API key, OAuth | endpoint/version, model, optional project |
| Google Vertex AI | Google OAuth/access token, workload identity, supported service identity | project, location, publisher, model |
| Azure OpenAI/Foundry | API key, Entra identity/access token | resource endpoint, deployment/model, API version/project |
| Self-hosted Rig provider | none, API key, bearer where the transport supports it | endpoint and model |

Long-lived cloud service-account or AWS credentials are not silently enabled for a
distributed build. Their catalog entry must declare whether it is available for a
personal sideload, requires short-lived federation, or requires an Athera gateway.
Normal UI shows only methods implemented and allowed for the current build. Developer
diagnostics shows the complete coverage matrix and the reason for unavailable methods.

### Provider source-of-truth rule

Provider descriptors and tests must be derived from the pinned Rig source and the
provider's current official authentication documentation. The initial sources are:

- [Rig repository and provider architecture](https://github.com/0xPlaygrounds/rig)
- [Anthropic API authentication and cloud-platform paths](https://platform.claude.com/docs/en/api/overview)
- [AWS SDK for Rust credential providers](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html)
- [AWS temporary web-identity credentials for mobile](https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRoleWithWebIdentity.html)
- [Gemini API key authentication](https://ai.google.dev/gemini-api/docs/api-key)
- [Gemini OAuth authentication](https://ai.google.dev/gemini-api/docs/oauth)
- [Google Cloud authentication](https://docs.cloud.google.com/docs/authentication)
- [Azure OpenAI/Foundry key and Entra authentication](https://learn.microsoft.com/en-us/azure/foundry/how-to/develop/sdk-overview)

If official documentation and Rig behavior differ, Athera marks the combination
unsupported until an adapter and conformance test resolve the difference. It does not
guess headers, token exchange, refresh behavior or model capabilities.

## 3. Dependency graph

```text
BASE-001 product definitions
    |
    +--> BASE-002 reproducible installed build
    |
    +--> CON-001 provider/auth contracts
    |       +--> DB-001 provider profile storage
    |       +--> SEC-001 credential-vault contract
    |       +--> UI-001 generic provider renderer fixture
    |       +--> PRV-001 pinned Rig provider inventory
    |
    +--> CON-002 task/blocker contracts
            +--> CORE-001 local intake and Needle outcome
            +--> DB-002 task migration
            +--> UI-005 typed assistant states

PRV-001 + SEC-001
    +--> RIG-001 Android compile/size spike
    +--> PRV-002 provider factory/catalog

SEC-002 Android vault + PRV-002
    +--> AUTH-* auth resolvers

RIG-001 + PRV-002 + AUTH-API
    +--> PVD-OPENAI/NVIDIA
    +--> PVD-ANTHROPIC
    +--> PVD-BEDROCK
    +--> PVD-GEMINI/VERTEX
    +--> PVD-AZURE
    +--> PVD-REMAINING

At least one live provider + CORE-001
    +--> CORE-002 unified routing
            +--> UI-006 one assistant composer
            +--> CORE-006 verified action loop
                    +--> AND-010 durable Android jobs
```

## 4. Task ledger

Statuses used during implementation: `TODO`, `READY`, `IN_PROGRESS`, `BLOCKED`,
`DONE`. A task becomes `DONE` only when its evidence gate passes.

### Baseline and governance

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| BASE-001 | Freeze the corrected product definitions and architecture links | None | README and authoritative docs agree on Needle-local-first and optional large local model |
| BASE-002 | Deterministic frontend packaging and visible build fingerprint | BASE-001 | Installed APK fingerprint matches source; one current JS bundle; screenshot recorded |
| BASE-003 | Remove large-local-chat provider from default feature/build path | BASE-001 | Clean APK runs without model binary/download/setup prompt |
| BASE-004 | Freeze unrelated feature expansion during restructure | BASE-001 | MCP/research/self-writing/general accessibility additions remain behind existing flags |
| BASE-005 | Add module ownership and integration rules to the execution ledger | BASE-001 | Each active task has one owner and allowed files |

### Neutral contracts

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| CON-001 | `ProviderDefinition`, `AuthOptionSpec`, `ConfigFieldSpec`, visibility and availability contracts | BASE-001 | Serialization, invalid descriptor and version tests |
| CON-002 | Neutral `CredentialRef`, `CredentialPurpose`, expiry and secret-state contracts | CON-001 | No secret-bearing type implements unsafe debug/serialization to public DTOs |
| CON-003 | Normalize model request, stream, usage, capability and provider error contracts | BASE-001 | Existing cloud and scripted providers pass compatibility tests |
| CON-004 | Split provider credential, connector OAuth, Android permission, approval, clarification and device blockers | BASE-001 | Every blocker round-trips and has one recovery action |
| CON-005 | Define `TaskRequirements` and routing provenance | CON-003 CON-004 | Route reasons and egress class serialize without vendor types |
| CON-006 | Define provider catalog and connection IPC payloads | CON-001 CON-002 | Rust/TypeScript payload contract snapshot passes |

### Storage and migration

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| DB-001 | Provider profiles: provider ID, auth-option ID, non-secret config, schema version | CON-001 | Upgrade/rollback fixture preserves current NVIDIA profile |
| DB-002 | Credential metadata and opaque references only | CON-002 | Database secret scan finds no plaintext credential |
| DB-003 | Provider health, last test, normalized failure and capability snapshot | CON-003 DB-001 | Restart preserves health without falsely claiming readiness |
| DB-004 | Migrate generic `WaitingForAuth` into typed blocker records | CON-004 | Existing tasks remain readable and resume through correct recovery path |
| DB-005 | Persist routing provenance and model usage | CON-005 | Context excludes credential/provider internals while diagnostics can read redacted facts |

### Secret vault and auth engines

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| SEC-001 | Vendor-neutral vault interface with purpose-bound get/put/delete | CON-002 | Wrong provider/profile/purpose cannot resolve a secret |
| SEC-002 | Android Keystore-backed encryption and ciphertext store | SEC-001 BASE-002 | Restart, overwrite, delete, corruption and device-lock tests on phone |
| SEC-003 | Host test vault for deterministic tests only | SEC-001 | Explicitly unavailable in production Android build |
| AUTH-001 | API key, bearer and typed custom-header resolver | SEC-002 CON-001 | Header redaction and endpoint-binding tests |
| AUTH-002 | Composite static secret resolver | SEC-002 CON-001 | Partial credential sets rejected; session token handled conditionally |
| AUTH-003 | OAuth 2.0 authorization-code PKCE, refresh, expiry and revoke | SEC-002 CON-001 | State/PKCE/redirect/replay and restart tests |
| AUTH-004 | AWS credential source and SigV4 integration | AUTH-002 SEC-002 | Static temporary credentials and approved web-identity/Cognito path pass Bedrock fixture |
| AUTH-005 | Google identity integration | SEC-002 CON-001 | Supported API-key/OAuth/workload-identity paths pass token-expiry tests |
| AUTH-006 | Azure Entra integration | SEC-002 CON-001 | Supported Entra token acquisition/refresh and API-key alternative pass fixtures |
| AUTH-007 | Credential lifecycle service | SEC-001 CON-001 | Each auth resolver reports expiry, refresh and revoke through one neutral lifecycle contract |
| AUTH-008 | Anthropic workload-identity token exchange | SEC-002 CON-001 | Official token exchange, expiry, workspace binding and failure fixtures pass |

### Rig and provider catalog

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| PRV-001 | Pin exact Rig revision and inventory every provider, transport, feature and auth expectation from source | CON-001 | Checked-in generated/reviewed coverage manifest; no marketing-list assumptions |
| RIG-001 | Minimal Rig Android ARM64 compile, dependency and APK-size spike | PRV-001 BASE-002 | Debug and release link; measured size/startup/TLS report |
| PRV-002 | Provider catalog registry and descriptor validation | PRV-001 CON-001 | Duplicate IDs, unsafe fields and unsupported auth combinations rejected |
| PRV-003 | Provider factory mapping profile plus resolved auth to one model client | PRV-002 CON-003 SEC-001 | No Rig/vendor type crosses the adapter boundary |
| PRV-004 | Real connection-test service using the production factory | PRV-003 DB-003 | Test result distinguishes credential, endpoint, model, quota, network and provider errors |
| PRV-005 | Model discovery/static model source and capability cache | PRV-003 | Unsupported discovery is explicit; cached models are profile-scoped and refreshable |
| PRV-006 | Provider conformance harness with scrubbed cassettes | PRV-003 | Streaming/non-streaming/tool/error/usage cases replay without credentials |

### Provider implementation batches

These tasks can run in parallel after their listed auth engine and `PRV-006` exist.

| ID | Provider scope | Depends on | Evidence gate |
| --- | --- | --- | --- |
| PVD-001 | OpenAI direct | AUTH-001 PRV-006 RIG-001 | Cassette suite plus dated live text/stream/tool test |
| PVD-002 | Generic OpenAI-compatible profile | AUTH-001 PRV-006 RIG-001 | Custom endpoint, no-auth/key/bearer/allowed-header fixtures |
| PVD-003 | NVIDIA NIM profile | PVD-002 | Live configured NVIDIA connection and response on Android |
| PVD-004 | Anthropic direct and Anthropic-compatible | AUTH-001 AUTH-008 PRV-006 RIG-001 | API-key/bearer and implemented workload-identity paths; workspace conditional field test |
| PVD-005 | AWS Bedrock | AUTH-004 PRV-006 RIG-001 | Region/model/SigV4, expiry and live authorized call on Android-supported auth path |
| PVD-006 | Gemini Developer API | AUTH-001 AUTH-005 PRV-006 RIG-001 | Implemented key/OAuth methods, streaming and tool fixtures |
| PVD-007 | Google Vertex AI | AUTH-005 PRV-006 RIG-001 | Project/location/model plus implemented Google identity path |
| PVD-008 | Azure OpenAI/Foundry | AUTH-001 AUTH-006 PRV-006 RIG-001 | Key/Entra profiles and deployment/API-version condition tests |
| PVD-009 | Remaining OpenAI-shaped providers in `PRV-001` | PVD-002 PRV-006 | One catalog entry and conformance result per pinned provider |
| PVD-010 | Remaining native/non-compatible Rig providers in `PRV-001` | PRV-001 PRV-006 RIG-001 AUTH-007 | One catalog entry and conformance result per pinned provider; a new auth family adds an explicit prerequisite before implementation |
| PVD-011 | Provider failover compatibility matrix | PVD-001 PVD-004 PVD-005 PVD-006 PVD-008 | Failover only between task-compatible profiles; no permission/privacy widening |

### Local-first assistant core

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| CORE-001 | Deterministic local intake for every input | CON-005 | Input works with cloud absent; no provider decision in UI |
| CORE-002 | Typed Needle outcome: local response, action proposal, cloud handoff or unavailable | CORE-001 | Real Needle fixtures/evals establish declared capability boundaries |
| CORE-003 | Task-requirement router across direct, Needle and provider profiles | CORE-002 PRV-003 | Route tests cover privacy, capability, latency, availability and explicit user choice |
| CORE-004 | Unified submit path replacing conversation/action modes | CORE-003 DB-004 | Greeting, action and ongoing job enter the same state machine |
| CORE-005 | Provider selection and compatible failover | CORE-003 PVD-011 | Failure never changes egress policy or replays uncertain writes |
| CORE-006 | Typed blocker pause/resume | CORE-004 CON-004 | Provider auth, connector auth, permission, approval and clarification resume independently |
| CORE-007 | Outcome verification and `OutcomeUnknown` handling | CORE-004 | External write is never reported successful or retried without evidence |
| CORE-008 | Per-task leases and resource locks | CORE-004 DB-004 | Independent reads progress concurrently; writes remain serialized per resource |

### Mobile presentation

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| UI-001 | Generic catalog and form renderer using static descriptor fixtures | CON-001 | Text/select/secret/URL/boolean and visibility-rule component tests |
| UI-002 | Provider picker and auth-option picker from Rust catalog | UI-001 CON-006 PRV-002 | Adding a catalog fixture requires no TS provider conditional |
| UI-003 | Secret-entry boundary showing only set/unset/expired state | UI-001 SEC-001 | Secret never appears in React state snapshots after submit |
| UI-004 | Save, test, repair, revoke and provider-health states | UI-002 UI-003 PRV-004 | Every normalized provider failure renders the exact recovery action |
| UI-005 | Typed blocker components | CON-004 | Provider, connector, permission, approval, clarification and outcome-unknown screenshots |
| UI-006 | One assistant composer; remove Conversation/Action selector | CORE-004 UI-005 | Same composer completes greeting, Needle route and provider route |
| UI-007 | Jobs/commitments surface | CORE-008 | Active, scheduled, blocked, completed, cancelled and unknown states |
| UI-008 | Developer diagnostics separation | BASE-002 DB-005 | Build/provider/event detail absent from normal recovery UI |

### Android durable execution

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| AND-001 | Typed Kotlin/Rust native bridge | CON-004 BASE-002 | Malformed payload/permission/outcome mapping tests |
| AND-002 | WorkManager bridge to durable Rust job IDs | CORE-008 DB-004 AND-001 | Deferred job wakes and advances once |
| AND-003 | Progress, blocker, approval and completion notifications | AND-002 UI-005 | Notification opens the exact job/recovery action |
| AND-004 | User-visible foreground execution for eligible ongoing work | AND-002 | Start/cancel/process-death test under current Android restrictions |
| AND-005 | Boot and app-update rescheduling | AND-002 | Eligible jobs reschedule; completed/cancelled/writes do not duplicate |
| AND-006 | Network/battery/storage constraints | AND-002 CON-004 | Constraint changes produce typed wait/resume events |

### First product capabilities

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| CAP-001 | Contacts search with stable candidates | AND-001 CORE-006 | One/multiple/no match and permission cases on device |
| CAP-002 | Approved SMS/message compose/send receipt | CAP-001 CORE-007 | Exact recipient/content approval and uncertain-outcome test |
| CAP-003 | Local reminder and follow-up trigger | AND-002 AND-003 | Survives process death and cancellation without duplicate notification |
| CAP-004 | Shared file/document intake and bounded parsing | CORE-004 | Supported type, size, cancellation and untrusted-content tests |
| CAP-005 | Save/share summarized document | CAP-004 CORE-007 | Output provenance and Android handoff outcome shown honestly |

### Personal context and controlled improvement

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| MEM-001 | Separate observation, confirmed fact, preference, rule and policy records | CORE-004 DB-005 | No inference can overwrite policy or a confirmed fact |
| MEM-002 | Bounded personal-context retrieval | MEM-001 | Irrelevant person/task data absent from model packet |
| MEM-003 | Preference proposal from repeated/corrective signals | MEM-001 | One event never silently creates a permanent rule |
| MEM-004 | Accept/reject/edit/expire/rollback preference rule | MEM-003 UI-007 | Scope and rollback device flow |
| MEM-005 | Generated skill proposal sandbox/evaluation pipeline | MEM-004 CAP-001 CAP-004 | Generated skill cannot enable tools, permissions or policy by itself |

### Evaluation and release

| ID | Deliverable | Depends on | Evidence gate |
| --- | --- | --- | --- |
| QA-001 | Contract and migration suite | CON-001 CON-002 CON-003 CON-004 CON-005 CON-006 DB-001 DB-002 DB-003 DB-004 DB-005 | Required CI matrix green |
| QA-002 | Provider cassette and credential-scrub suite | PRV-006 | No secret/account identifier in fixtures |
| QA-003 | Live-provider matrix runner | PVD-001 PVD-002 PVD-003 PVD-004 PVD-005 PVD-006 PVD-007 PVD-008 PVD-009 PVD-010 | Dated result per provider/auth option; unavailable is not reported as pass |
| QA-004 | Local-first routing evaluation | CORE-003 | Needle/direct/cloud routing accuracy and handoff failures measured |
| QA-005 | Installed-APK Maestro/device flows | UI-006 AND-002 | Provider setup, greeting, action, blocker, restart and cancellation flows |
| QA-006 | Security review | SEC-002 AUTH-007 CORE-007 MEM-005 | Credential, prompt-injection, approval and replay regressions |
| REL-001 | Release build and dependency inventory | QA-001 QA-002 QA-005 QA-006 | Reproducible signed artifact and exact source fingerprint |

## 5. Parallel execution lanes

Parallelism begins only after shared contracts are accepted. Two agents must not edit
the same composition or migration file concurrently.

### Wave 0 — short serial foundation

`BASE-001 -> CON-001/002/003/004 -> BASE-002`

This freezes the payloads and installed-build identity that every lane consumes.

### Wave 1 — four independent modules

| Lane | Tasks | File ownership |
| --- | --- | --- |
| Provider/Rig | PRV-001, RIG-001, PRV-002, PRV-006 | `crates/provider-rig/**`, provider coverage fixtures |
| Security/Android | SEC-001, SEC-002, SEC-003 | credential contracts already frozen; Android vault/native secret files |
| Core/storage | CON-005, DB-001 through DB-005, CORE-001 | contracts/core/storage only; migration numbers reserved first |
| Mobile UI | UI-001 and static descriptor fixtures | `apps/mobile/src/features/connections/**`; no runtime wiring yet |

Integration point: `PRV-003` combines the first three lanes after their tests pass.

### Wave 2 — auth and first providers

| Lane | Tasks that can run together |
| --- | --- |
| Static auth/providers | AUTH-001, PVD-001, PVD-002, then PVD-003 |
| Federated auth | AUTH-003, AUTH-005, AUTH-006 and AUTH-008 behind their neutral contracts |
| AWS | AUTH-002, AUTH-004, then PVD-005 |
| UI/runtime | UI-002, UI-003, DB-003, then PRV-004/UI-004 |

Anthropic, Gemini/Vertex and Azure provider tasks begin as soon as their auth
dependency passes. They do not wait for Bedrock.

### Wave 3 — unified assistant

| Lane | Tasks that can run together |
| --- | --- |
| Local/core | CORE-002, CORE-003, CORE-004 |
| Provider expansion | PVD-004, PVD-006, PVD-007, PVD-008, PVD-009, PVD-010 |
| UI | UI-005, then UI-006 and UI-008 |
| Evaluation | QA-002, QA-003 fixtures, QA-004 scenarios |

Integration point: the device must pass NVIDIA plus at least one non-OpenAI protocol
provider before the provider layer is considered general.

### Wave 4 — durable work and useful capabilities

| Lane | Tasks that can run together |
| --- | --- |
| Runtime | CORE-006, CORE-007, CORE-008 |
| Android jobs | AND-001, then AND-002/003/004/006 |
| Capabilities | CAP-001 and CAP-004 after native/input contracts |
| UI/eval | UI-007, QA-005 scenario authoring |

CAP-002 depends on contacts and verification. CAP-003 depends on durable jobs.
CAP-005 depends on document parsing and verification.

### Wave 5 — personalization and hardening

`MEM-001/002 -> MEM-003/004 -> MEM-005`, while provider coverage, device matrices,
security review and release work continue in their own files.

## 6. Critical path to a truthful first build

```text
BASE-001
  -> CON-001/002/003
  -> SEC-001/002
  -> PRV-001/RIG-001/PRV-002/003/004/006
  -> AUTH-001
  -> PVD-002/003
  -> CORE-001/002/003/004
  -> UI-002/003/004/005/006
  -> QA-005
```

The first gate remains a real NVIDIA connection and `hi` response on the installed
phone, but the contracts built for that gate already support multiple provider and
auth patterns. It is not an NVIDIA-specific patch.

## 7. Provider definition of done

For every catalog provider and each advertised auth option:

1. Configuration fields come from the Rust catalog.
2. Conditional fields render without provider-specific React code.
3. Secrets enter the purpose-bound vault and never return to React.
4. Connection test uses the production provider factory.
5. Non-streaming, streaming, cancellation and tool/schema behavior are tested when
   the provider advertises them.
6. 401/403, endpoint, model, quota/rate limit, timeout, network and malformed-response
   errors normalize correctly.
7. Credential expiry, refresh, revoke and restart behavior match that auth option.
8. The Android target compiles and the provider's dependency/size impact is recorded.
9. A dated live test is recorded where credentials and service access are available.
10. Unsupported features are shown as unavailable, never silently emulated or
    reported as complete.

## 8. User acceptance plan (owner: user, 2026-09-23)

These checks are the user's live/device acceptance work. They do not make an
upstream development task complete by themselves: the production factory,
routing and UI path must first pass their automated gates. A box becomes done
only when the user records the result (pass/fail + date) against it.

### UAT-001 First product gate (phone)
- [ ] Install APK, open Diagnostics tab: fingerprint (git hash, timestamp,
      version) matches source
- [ ] Configure NVIDIA NIM provider with API key in Settings; key persists
      across app restart (Keystore)
- [ ] Send `hi` in the single composer; real cloud response returns
- [ ] Wrong key produces a credential error distinct from network errors

### UAT-002 Android vault (SEC-002)
- [ ] Restart preserves credentials
- [ ] Overwrite + delete behave correctly
- [ ] Corrupted entry fails closed (no crash, clear error)
- [ ] Device-lock behavior acceptable

### UAT-003 Live providers (one box per provider tested)

Do not run these boxes until the production Rig factory serves provider requests
and Settings renders endpoint/auth fields from the Rust catalog. Earlier runs
exercise the legacy provider-cloud path and their results must be discarded, not
recorded here.

- [ ] PVD-001 OpenAI: text + streaming + tool call
- [ ] PVD-003 NVIDIA NIM: text response
- [ ] PVD-004 Anthropic: text response (needs key)
- [ ] PVD-006 Gemini: text response (needs key)
- [ ] PVD-008 Azure OpenAI: key path (needs endpoint + deployment)

### Pending dev work (owner: agent, not blocked on user)

- Replace the scaffold-only `provider-rig` path with real Rig clients and make
  the production runtime use that factory.
- Correct provider-specific auth headers and implement genuine composite
  credentials before any provider is called supported.
- Wire deterministic requirements/routing into the single submit path and let
  Needle produce a normal local response or a typed handoff.
- Replace the legacy hard-coded Action-model settings with the Rust catalog and
  conditional auth renderer.
- Add Android durable jobs, typed capability bridges and first-product tools.
- Restore a native linker/C toolchain and run the full Rust workspace suite.

## 9. Verified implementation status (2026-09-23)

The task-by-task reconciliation is recorded in
[`ATHERA_STATUS_AUDIT_2026-09-23.md`](./ATHERA_STATUS_AUDIT_2026-09-23.md).
It is the current status overlay for this graph.

| State | Count | Meaning |
| --- | ---: | --- |
| Verified | 7 | The applicable automated evidence gate passed |
| Implemented; verification remains | 12 | Substantial production code exists, but a required suite/device/dependency gate is still open |
| Partial | 48 | Contracts, UI, a legacy path or a subset exists; the deliverable is not end-to-end |
| Not implemented | 17 | The substantive production path is absent |

`Partial` and `implemented; verification remains` are not synonyms for `DONE`.
Tasks requiring provider credentials, Android lifecycle behavior or an installed
APK also remain open until their dated live evidence is recorded.
