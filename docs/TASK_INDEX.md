# Task Index

This is the single implementation ledger. No mock or source-only implementation counts as a live-device acceptance pass.

## Web credential entry verification

- M05/M09/M12: web Settings accepts a masked API key through `save_cloud_provider`; Rust supplies it through the existing SecretStore interface. Session keys are replaceable, clearable, bound to an endpoint, and never persisted in settings or returned in snapshots. Backend restart removes them; OS-backed persistent storage remains open.
- Security checks: malformed input, unchanged storage after rejection, key absence from snapshots/settings/events, restart behavior, and endpoint isolation pass. The loopback bridge rejects non-local Host/Origin values in addition to requiring its custom header. It remains a development-only service, not an authenticated multi-user API.
- Evidence: four runtime tests, one bridge security test, eight UI tests, launcher regression, clippy, frontend lint, formatting, and UI build passed. Live bridge checks verified the credential command and rejected malformed keys/cross-origin requests without modifying the user's credential. Browser visual QA was unavailable because no browser connection was present. Live NVIDIA inference still requires a user-supplied key.

## Greeting recovery verification

- M08/M09: fixed empty-tool routing to the cloud, repeated identical search recovery, and explicit authentication pauses. Real browser-bridge smoke: `hi` reached `waiting_for_auth` after one inference step with zero failures; resume rechecked authentication without exhausting the failure budget.
- M05/M12: invalid credential references are rejected before saving; legacy invalid values are replaced without exposing them to UI. Header distinguishes missing credentials from configuration, and never claims verified model readiness.
- Evidence: 21 targeted Rust tests, 5 UI tests, launcher regression, clippy, frontend lint, formatting, and UI production build passed. Live NVIDIA response remains unverified because the backend environment has no configured API key. Previously pasted keys require rotation; SQLite replacement is not secure erasure.

Statuses: TODO, READY, IN_PROGRESS, BLOCKED, DONE, DEFERRED. Update evidence and blocker notes when changing status. Work remains open until all required acceptance checks pass.

| Task | Module | Deliverable | Dependencies | Status | Evidence / Remaining Work |
|---|---|---|---|---|---|
| M00.1 | Planning | Reconcile architecture documents | None | DONE | Existing docs reconciled into MASTER_BUILD_PLAN.md |
| M00.2 | Planning | Create task ledger | None | DONE | This file is the ledger |
| M00.3 | Planning | Map implementation prompts | None | DONE | Prompt scope mapped to M01-M15/V01 |
| M01.1 | Foundation | Inspect toolchain and initialize Git | M00 | DONE | Git repository initialized; local Rust/Node toolchain bootstrapped |
| M01.2 | Foundation | Scaffold Rust/Tauri/React | M00 | DONE | Cargo workspace, Tauri shell, Vite React app compile |
| M01.3 | Foundation | Establish CI and formatting | M00 | DONE | .github/workflows/check.yml and check scripts added |
| M02.1 | Contracts | Provider/action/handoff types | M01 | DONE | Shared typed contracts compile and serialize |
| M02.2 | Contracts | Capability and persistence contracts | M01 | DONE | Tool/skill/store boundaries compile |
| M02.3 | Contracts | IPC/events and mocks | M01 | DONE | Runtime command surface and scripted providers added |
| M03.1 | Needle | Verify upstream artifacts and ABI | M01 M02 | DONE | Needle 2 revision, license, header and checksums recorded |
| M03.2 | Needle | Native lifecycle and linking | M01 M02 | DONE | Verified Windows DLL smoke succeeds; repository-local Android NDK 27.2.12479018 is present; Android static-link proof remains pending |
| M03.3 | Needle | Provider and physical ARM64 proof | M01 M02 | BLOCKED | Android toolchain and ARM64 Rust target are present; requires an authorized ARM64 device and Android build/run evidence |
| M04.1 | Storage and Context | Migrations and repositories | M02 | DONE | SQLite migration and persistence tests pass |
| M04.2 | Storage and Context | Task/action/result persistence | M02 | DONE | Restart and ordered event tests pass |
| M04.3 | Storage and Context | Bounded context and history | M02 | DONE | Context budget and catalogue scale tests pass |
| M05.1 | Policy and Secrets | Risk and permission checks | M02 M04 | DONE | Runtime validates enabled state, schema and risk |
| M05.2 | Policy and Secrets | Exact-action approvals | M02 M04 | DONE | Stale/duplicate approval tests pass |
| M05.3 | Policy and Secrets | Platform secret store and redaction | M02 M04 | IN_PROGRESS | Host env reference exists; Android Keystore adapter required |
| M06.1 | Capabilities | Registry and FTS search | M02 M04 M05 | DONE | Unified tool/skill search tests pass |
| M06.2 | Capabilities | Progressive schema loading | M02 M04 M05 | DONE | Model context is bounded to eight tools |
| M06.3 | Capabilities | Catalogue refresh and versioning | M02 M04 M05 | DONE | MCP refresh disables removed/changed tools |
| M07.1 | Skills | Validate and import skills | M06 | DONE | Manifest validation and import UI added |
| M07.2 | Skills | Resolve tool requirements | M06 | DONE | Activation rejects missing/disabled tools |
| M07.3 | Skills | Activate and scope versions | M06 | DONE | Scoped skill revalidation is enforced |
| M08.1 | Task Engine | Persistent action loop | M04 M05 M06 | DONE | Multi-step task tests pass |
| M08.2 | Task Engine | Limits cancellation and recovery | M04 M05 M06 | DONE | Cancellation, limits and unknown-write tests pass |
| M08.3 | Task Engine | Plans and model handoffs | M04 M05 M06 | DONE | Hybrid handoff tests pass |
| M09.1 | Cloud | OpenAI Responses adapter | M02 M05 | DONE | Request/response normalization tests pass |
| M09.2 | Cloud | Compatible endpoint adapter | M02 M05 | DONE | Compatible response normalization tests pass |
| M09.3 | Cloud | Streaming capabilities fallback and live tests | M02 M05 | IN_PROGRESS | Bounded response reader exists; live credentials pending |
| M10.1 | MCP and Authentication | Streamable HTTP discovery and execution | M05 M06 M08 | IN_PROGRESS | Anonymous adapter tests pass; live server and authenticated profiles pending |
| M10.2 | MCP and Authentication | OAuth callback and task resume | M05 M06 M08 | BLOCKED | Browser/deep-link and provider credentials not configured |
| M10.3 | MCP and Authentication | Real Composio Gmail Calendar proof | M05 M06 M08 | BLOCKED | External account and OAuth setup required |
| M10.4 | MCP and Authentication | Credential profiles and secret binding | M05 M06 M08 | IN_PROGRESS | Static Bearer and reviewed API-key profiles use exact connection/origin/resource/purpose binding, session-only values, redacted status and redirect denial; OAuth extensions, Android Keystore, mTLS and adapter-specific signing remain |
| M10.5 | MCP and Authentication | stdio and legacy SSE compatibility | M05 M06 M08 | TODO | stdio belongs on desktop/server; legacy SSE uses explicit spec fallback |
| M11.1 | Native Tools | Android capability registration | M05 M06 | IN_PROGRESS | Shared path is ready; Android plugin pending |
| M11.2 | Native Tools | Platform permissions and errors | M05 M06 | BLOCKED | Requires Android integration |
| M12.1 | Coordination | Connect provider roles | M03 M07 M08 M09 M10 | DONE | Needle/cloud role handoff tests and host Needle smoke pass |
| M12.2 | Coordination | Handoffs and search recovery | M03 M07 M08 M09 M10 | DONE | Search, handoff and replanning actions implemented |
| M12.3 | Coordination | Fallback and replanning integration | M03 M07 M08 M09 M10 | DONE | Unavailable Needle delegates to cloud fixture |
| M13.1 | Mobile Experience | Assistant and Activity | M02 M08 M12 | DONE | React screens, task states and UI tests added |
| M13.2 | Mobile Experience | Connections and configuration | M02 M08 M12 | DONE | Models/tools/skills/MCP controls added |
| M13.3 | Mobile Experience | Approval auth offline and reconnect | M02 M08 M12 | IN_PROGRESS | Host UI states exist; Android E2E pending |
| M14.1 | Evaluation | Real-core runner and fixtures | M02 M12 | DONE | JSON-lines runner and fixture added |
| M14.2 | Evaluation | Retrieval context and security regressions | M02 M12 | DONE | 11 core and 3 provider tests pass |
| M14.3 | Evaluation | Diagnostics and measured comparisons | M02 M12 | IN_PROGRESS | Host smoke metrics recorded; benchmark report pending |
| M15.1 | Delivery | Integrated Android regressions | M01-M14 | BLOCKED | SDK/NDK are present; no authorized physical device is currently visible to ADB |
| M15.2 | Delivery | Security and recovery review | M01-M14 | IN_PROGRESS | Core checks present; final integration review pending |
| M15.3 | Delivery | Signed build and setup notes | M01-M14 | BLOCKED | Personal signing/build environment not configured |
| V01.1 | Voice Boundary | Transcript input and response events | M02 M08 M13 | DONE | `InputSource::Voice` and shared dispatch path are implemented |
| V01.2 | Voice Boundary | Future speech adapter contract | M02 M08 M13 | DONE | Voice is explicitly deferred behind shared input/events |

## Deferred

Wake word, executable skill plugins, broad screen automation and public publishing remain deferred. Model lifecycle and foreground voice are now in E02/E10; they are no longer blanket deferrals. The app name is undecided; internal assistant identifiers are development-only.

## Verification

See `INTEGRATION.md` for reproducible commands and `MASTER_BUILD_PLAN.md` for acceptance gates. Physical-device tests, cloud credentials, OAuth configuration and app signing must be recorded separately from deterministic checks.


## Vertical-slice execution ledger (2026-09-13)

This ledger is the active execution order. It supersedes the historical M/V and
companion E ordering without discarding their evidence. `VERTICAL_SLICE_PLAN.md`
defines each gate; its backend, mobile and evaluation playbooks provide work packets.

| ID | Outcome | Dependencies | Status | Required evidence before DONE |
| --- | --- | --- | --- | --- |
| VS00 | Reproducible backend, frontend and external-eval setup | None | DONE | Rust/Cargo 1.98.1, Node 24.21.0, JDK 22.0.2, Python 3.11.15, DeepEval 4.2.2, Android API 36/NDK 27.2.12479018/ADB 37.0.1; 57 Rust tests, 16 mobile tests plus launcher, production build, all JSONL fixtures, nine eval harness tests, dataset lint and real-runner DeepEval smoke pass; 390x844 dark render has no overflow or JS errors; physical device absent and live profiles remain explicitly blocked |
| VS01 | Common execution stream and structured output | VS00 | DONE | `aethra.output.v1` validates Markdown/list/table blocks; migrations 003/004 persist `aethra.run-event.v1` with stable task/run IDs, worker lifecycle, bounded text deltas, block upsert, one terminal event, paging and stale-cursor reset. Responses and Chat Completions SSE parsers have fragmented-stream/tool-argument tests; late deltas are discarded. Five real-runner fixtures include streaming output; JSON-lines/browser bridge parity matches eight normalized events. Integrated evidence: 69 Rust tests, ten eval-harness tests, 18 mobile tests plus launcher, production build, real-runner DeepEval smoke, and 390x844 structured and streaming renders pass. Live cloud inference remains credential-dependent evidence and is not claimed by this deterministic gate |
| VS02 | Public Parallel search with cited evidence | VS01 | BLOCKED | Versioned anonymous Streamable HTTP manifest, Parallel preset, canonical `web.search`/`web.open`, bounded raw/context separation, source blocks, exact recorded fixture, injection/size failures, and dated live anonymous discovery/query pass. Integrated evidence: 85 Rust tests, ten eval-harness tests, 21 mobile tests, DeepEval real-runner smoke, production build, and 390x844 source render. No cloud credential is configured in the build environment, so the required live cloud-model selection and cited synthesis run remains externally blocked |
| VS03 | Authenticated MCP connections | VS02 | IN_PROGRESS | Tagged anonymous/Bearer/API-key/OAuth manifests, strict HTTPS/header/resource validation, exact credential binding, session-only resolver, pre-I/O auth pause, redirect denial, MCP 401 mapping, redacted typed status, endpoint-change/restart/secret-leak regressions, and mobile static/OAuth setup pass. Host OAuth covers Bearer challenges, RFC discovery order, extensible metadata, preregistration/CIMD/DCR selection and public DCR, 256-bit S256 PKCE, resource/redirect/state binding, expiry/replay denial, bounded code exchange, exact token ownership, refresh rotation, transport recreation, and one-time task resume. Dated live Parallel metadata discovery passed (`C4BAD…7A82`). Android Custom Tabs/App Links/Keystore, step-up challenge propagation, and live token/authenticated-call proof remain pending |
| VS04 | Bounded graph and parallel read workers | VS03 | IN_PROGRESS | Vendor-neutral persisted graph contracts now validate DAG shape, exact task/node/worker binding, unique idempotency keys, deadlines, retry/attempt budgets, 64 KiB worker context, 1-4 concurrency and read-only tool execution. Deterministic readiness, lease/running/outcome transitions, bounded retry, restart recovery, cancellation, partial joins and duplicate-outcome denial have contract tests. Planner proposal expansion, Assistant scheduling/persistence events, concurrent real-runtime/restart traces and plan/tool DeepEval report remain pending |
| VS05 | Rich Parallel research and monitoring | VS04 | TODO | Typed output/source tests; signed/deduplicated webhook fixture; dated live Task result |
| VS06 | Supervised on-device Needle action worker | VS03 VS04 | TODO | Exact policy fixtures; real Needle output; controlled physical action logs and device metrics |
| VS07 | Voice and document inputs | VS06 | TODO | Permission/cancel/grounding tests; physical voice and supported-document evidence |
| VS08 | Durable personal workflows | VS04 VS05 | TODO | Virtual-clock/restart/dedup tests; review UI; isolated live read-only schedule |
| VS09 | Security, compatibility and signed release | VS01-VS08 | TODO | Full checks/evals; security review; rendered/device matrix; signed APK and checksum |

VS00 completed on 2026-09-13. VS01 completed on 2026-09-14. VS02's local and live
anonymous-search work is complete, but its configured cloud-model gate is blocked by
the absence of a cloud credential in the build environment. VS03 host work has begun
with static credential profiles and binding; its OAuth and Android gates remain open.

## Historical companion implementation ledger (2026-09-11)

This E-series previously superseded conflicting scope in the M/V rows above. It is
retained for evidence mapping; the VS ledger now owns execution order. Historical
rows retain their original evidence and status. DONE still means only the acceptance
gates recorded for that historical row.

| ID | Owner | Dependencies | Implementation | Host verification | Live/device verification | Evidence / blockers |
|---|---|---|---|---|---|---|
| E00 | Primary | None | DONE | PASS | N/A | Unique IDs; plan reconciled; baseline below |
| E01 | Primary + storage | E00 | IN_PROGRESS | Pending integrated rerun | N/A | Typed conversation contracts; additive migration |
| E02 | Local-model agent | E01 | IN_PROGRESS | Pending native smoke | NOT RUN | Pinned model lifecycle and host adapter in development |
| E03 | Primary | E02 | BLOCKED | SDK inventory inspected | BLOCKED | SDK present, no NDK found; user will connect phone after development; no emulator |
| E04 | Storage + primary | E01 E02 | IN_PROGRESS | Pending integrated rerun | NOT RUN | History, explicit memory, temporary chats and bounded packets |
| E05 | Primary | E02 E04 | IN_PROGRESS | Pending integrated rerun | NOT RUN | Conversation/action paths separated; automatic proposals and cloud coordination still open |
| E06 | UI agent | E01 E04 E05 | IN_PROGRESS | Pending UI checks/visual QA | NOT RUN | Conversation setup, history, stop/retry and memory |
| E07 | Primary | E03 E05 | TODO | NOT RUN | BLOCKED | Native plugin, permissions and physical phone required |
| E08 | Storage + primary + UI | E04 E06 | IN_PROGRESS | Pending integrated rerun | NOT RUN | Research sources/revision storage; live retrieval remains open |
| E09 | Primary | E05 E07 | TODO | Existing MCP fixtures only | BLOCKED | Live MCP and calendar account/callback required |
| E10 | Primary | E06 E07 | TODO | Existing voice input contract only | BLOCKED | Kotlin speech/TTS and device required |
| E11 | Primary | E07 | TODO | NOT RUN | BLOCKED | Reminder scheduling/notifications and device required |
| E12 | Primary | E00-E11 | IN_PROGRESS | Baseline recorded; final checks pending | BLOCKED | No signed APK or comparative physical benchmark yet |

### E00 baseline

2026-09-11: `cargo test --workspace`: 26 passed (4 runtime, 1 bridge,
15 orchestration, 4 cloud protocol, 1 Needle, 1 storage). `npm test`:
1 launcher regression and 8 React tests passed; `npm run build` passed.
PowerShell script policy required a process-scoped Bypass invocation using the
repository toolchain. Frontend esbuild needed sandbox escalation to read ancestor
paths. No test failures remained in the baseline after environment correction.
The whole repository was untracked on entry; do not confuse a full new-file Git diff
with work performed in this iteration. No existing configuration was reset.
