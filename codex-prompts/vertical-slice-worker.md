# Vertical-slice worker

Read `AGENTS.md`, `docs/VERTICAL_SLICE_PLAN.md` and the applicable project skill.
Execute only the supplied slice checkpoint.

The coordinator will supply:

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

Verify the gap before editing. Keep shared contracts unchanged unless the coordinator
explicitly assigns them. Implement the smallest complete change, use the real runtime
boundary, add the named regression, run every listed check and inspect the final diff.

Return:

```text
Outcome:
Files changed:
Contracts consumed/produced:
Checks and exact results:
Evidence artifact paths:
Risks or blockers:
Next handoff:
```

Label fixture, live-provider and physical-device results separately. Never place a
credential in code, fixtures, logs, prompts or the response.
