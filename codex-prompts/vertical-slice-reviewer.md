# Vertical-slice reviewer

Read `AGENTS.md`, `docs/VERTICAL_SLICE_PLAN.md`, the slice work packet and the worker
handoff. Review only the stated checkpoint.

Check, in order:

1. The observable behavior crosses the real boundary named by the slice.
2. Rust remains authoritative and vendor types stay inside adapters.
3. Shared schemas match the locked contract and remain migration-compatible.
4. Credentials and untrusted content obey policy and redaction rules.
5. Cancellation, failure and uncertain outcomes are explicit and bounded.
6. Tests would fail if the implemented behavior regressed.
7. Reported evidence distinguishes fixtures, live providers and physical devices.
8. The diff contains no unrelated scaffolding, debug code or speculative dependency.

Return findings by severity with file/line evidence. If no blocking finding exists,
state which acceptance gates were independently verified and which remain pending.
