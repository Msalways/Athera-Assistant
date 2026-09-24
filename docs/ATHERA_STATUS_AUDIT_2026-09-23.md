# Athera task status audit

Date: 2026-09-23

Scope: reconcile the 84 tasks in `ATHERA_DEVELOPMENT_TASK_GRAPH.md` against the
current source tree, automated tests and device/live evidence. This report does
not treat a type, stub, fixture or unchecked UAT box as a completed feature.

## Status definitions

- **Verified**: the task's applicable automated evidence gate passed.
- **Implemented; verification remains**: substantive production code exists,
  but a required full-suite, installed-device, dependency or live gate is open.
- **Partial**: only contracts, scaffolding, UI, a legacy implementation or part
  of the acceptance criteria exists.
- **Not implemented**: the task's substantive production path is absent.

## Evidence collected

| Check | Result |
| --- | --- |
| Task graph dependency validation | 84 task IDs; no missing dependency IDs |
| `assistant-contracts` on Windows GNU Rust | 135 passed, 0 failed |
| Frontend Vitest | 8 files and 30 tests passed |
| Frontend lint/type check | Passed |
| Frontend production build | Passed |
| Full Rust workspace tests | Not run to completion: MSVC `link.exe` and GNU `gcc.exe` are unavailable |
| Android durable-job search | No WorkManager, foreground service, boot receiver or constraint scheduler implementation found |
| Installed-device/live-provider evidence | No dated passing record in the task documents |

## Reconciled task inventory

### Verified - 7

`BASE-001`, `CON-001`, `CON-002`, `CON-004`, `CON-005`, `SEC-001`, `UI-008`

Key evidence: architecture terminology is aligned; provider/auth, credential,
blocker and routing-requirement contract tests pass; the vault contract tests
pass; developer diagnostics are separated from the normal UI and the frontend
suite/build pass.

### Implemented; verification remains - 12

`BASE-003`, `CON-003`, `DB-001`, `DB-002`, `DB-004`, `DB-005`, `SEC-002`,
`MEM-001`, `MEM-002`, `MEM-003`, `MEM-004`, `MEM-005`

Key evidence:

- The large local-chat model is optional in the default feature set, but a clean
  installed APK has not proved the gate.
- Provider/model records and migrations 007-009 implement profile, credential,
  usage, blocker and personalization persistence. The full storage suite is
  still blocked by the missing native toolchain.
- Android Keystore AES/GCM storage is wired for the legacy cloud key, but its
  restart, overwrite, delete, corruption and lock-state device matrix is open.
- Personalization has distinct observations/rules/revisions, bounded retrieval,
  proposal/decision/rollback/expiry flows and a generated-skill replay sandbox.
  Its frontend tests pass; its complete Rust and device flows remain unverified.

### Partial - 48

`BASE-002`, `BASE-004`, `BASE-005`, `CON-006`, `DB-003`, `SEC-003`,
`AUTH-001`, `AUTH-002`, `AUTH-003`, `AUTH-004`, `AUTH-005`, `AUTH-006`,
`AUTH-007`, `AUTH-008`, `PRV-001`, `RIG-001`, `PRV-002`, `PRV-003`,
`PRV-004`, `PRV-005`, `PRV-006`, `PVD-001`, `PVD-002`, `PVD-003`,
`PVD-004`, `PVD-005`, `PVD-006`, `PVD-007`, `PVD-008`, `PVD-009`,
`CORE-001`, `CORE-003`, `CORE-004`, `CORE-006`, `CORE-007`, `CORE-008`,
`UI-001`, `UI-002`, `UI-003`, `UI-004`, `UI-005`, `UI-006`, `UI-007`,
`AND-001`, `CAP-002`, `QA-001`, `QA-002`, `QA-004`

Important boundaries inside this group:

- `provider-rig` pins Rig but does not instantiate a Rig provider, perform an
  HTTP request or stream inference. The runtime still uses `provider-cloud`.
- The catalog has 21 of Rig's 26 core providers. ChatGPT, Cohere, Copilot,
  Llamafile and Voyage AI are absent. Several bulk-seeded providers use the
  same `x-api-key` convention even though their production APIs differ.
- `CompositeSecretResolver` currently repeats one stored secret for multiple
  requested keys; it is not a working AWS-style composite resolver.
- Requirements, routing provenance and typed blockers exist, but the single
  submit path does not yet use the new router. Needle still hands normal chat
  to another model instead of returning a normal local response.
- The React shell has one composer, but settings still render a hard-coded
  legacy Action-model form rather than catalog-driven conditional auth fields.
- The SMS experiment has Rust policy and Android accessibility pieces, but it
  is not yet a general typed capability bridge and has no dated device proof.

### Not implemented - 17

`PVD-010`, `PVD-011`, `CORE-002`, `CORE-005`, `AND-002`, `AND-003`,
`AND-004`, `AND-005`, `AND-006`, `CAP-001`, `CAP-003`, `CAP-004`,
`CAP-005`, `QA-003`, `QA-005`, `QA-006`, `REL-001`

These are the remaining companion-provider/failover work, a real local Needle
response, Android durable execution, contacts/reminders/document capabilities,
live/device/security matrices and the signed release gate.

## Corrections to earlier task notes

1. Provider and auth scaffolds are not `DONE` merely because their contracts
   compile. No provider is complete through the new Rig production factory.
2. `RIG-001` has useful dependency compile evidence, but no real Rig client,
   packaged APK size delta, startup measurement or Android TLS request.
3. The previous claim that all Rig providers were cataloged was incorrect.
   Current coverage is 21/26 for `rig-core 0.42.0`.
4. Provider UAT should not start until the factory, auth mapping and generic
   settings UI are integrated; otherwise it tests the old provider path.
5. Personalization is much further along than the prior pending-work note said.
   It should be finished by restoring the Rust test toolchain and running the
   device acceptance flow, not rebuilt from scratch.

## Next dependency-safe development wave

1. Finish `PRV-001` through `PRV-004`: complete/correct the catalog, build real
   Rig clients, normalize errors and run connection tests through that factory.
2. Finish `AUTH-001` and `AUTH-002`, then integrate NVIDIA as the first real
   OpenAI-compatible acceptance path.
3. Wire `CORE-001` through `CORE-004` into one submit path and make Needle a
   valid local responder/handoff layer without requiring a large model.
4. Replace the legacy settings form with `UI-001` through `UI-004`.
5. Only then run NVIDIA/device UAT; in parallel, restore the linker and close
   the database/personalization verification backlog.
