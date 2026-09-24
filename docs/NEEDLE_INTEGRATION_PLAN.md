# Needle integration gap closure — implementation plan

Date: 2026-09-24
Status: planned, not started
Scope: `crates/provider-needle`, contract types only where required. No engine,
UI, fine-tuning, retrieval-index, or Needle 3 work.

## Problem

`provider-needle` drives a calibrated, grammar-constrained tool-call model but
discards four of its safety-relevant outputs:

1. The `confidence` score is never read — the documented act/escalate contract
   is unenforced.
2. The empty-call refusal (`function_calls: []`) is mapped to `InvalidResponse`
   (an error) instead of a recoverable outcome.
3. The system prompt carries no `date:` fact, so relative temporal language
   passes through unresolved by construction.
4. The `validation.ungrounded` list is never checked, so fabricated argument
   values can reach execution.

Additionally, all envelope interpretation lives inline in `infer()` behind the
FFI call, making it untestable without the native library.

## Design principles

- No new `AgentAction` variants. Refusal, low confidence, and ungrounded
  values all map to the existing `Handoff { role: Reasoner, ... }` path —
  they are recoverable situations for cloud/user repair, not task failures
  and not provider errors.
- Pure decision logic separated from FFI. `infer()` becomes: native call →
  parse envelope → `decide_action()`. Everything below is unit-testable
  without `libneedle`.
- Fail closed. Missing confidence, malformed envelopes, and fabricated values
  never produce a `CallTool`.
- No magic without a label. The initial confidence threshold is explicit,
  documented as uncalibrated, and owned by the follow-up measurement task.

## Changes

### 1. `NeedleProvider` gains a confidence threshold

- New field `confidence_threshold: f64`, default `0.70`.
- New builder-style setter `with_confidence_threshold(f64) -> Self`.
- The default is a starting value only, pending device calibration
  (follow-up R&D item 1). It is recorded here so nobody mistakes it
  for a measured number.

### 2. `decide_action` — pure envelope interpreter (new, fully tested)

```rust
fn decide_action(
    envelope: &serde_json::Value,
    confidence_threshold: f64,
    context: &ContextBundle,
) -> Result<AgentAction, Error>
```

Evaluation order (first match wins):

1. Envelope is not an object, or `function_calls` is missing/not an array
   → `Err(Error::InvalidResponse)` (genuine engine bug, unchanged).
2. `validation.ungrounded` is a non-empty list
   → `Handoff { role: Reasoner, objective: goal, reason: "ungrounded arguments: <paths>" }`.
   Fabricated values must never execute; the cloud/user re-derives them.
3. `function_calls` is empty (`[]`)
   → `Handoff { role: Reasoner, objective: goal, reason: "no applicable tool" }`.
   The documented refusal contract; recoverable, not an error.
4. More than one call → `Err(Error::InvalidResponse)` (unchanged; single-shot contract).
5. `confidence` below threshold, missing, null, or non-numeric
   → `Handoff { ..., reason: "low confidence <value>" }`.
   Missing confidence fails closed (treated as 0.0).
6. `name == "request_assistance"` → existing `Handoff` path, extended so the
   reason includes the confidence value when present.
7. Otherwise → existing `protocol::decode_call` path (unchanged).

### 3. System facts with date (new helper, tested)

- New `fn system_prompt() -> String`: base instruction string plus a
  `date: YYYY-MM-DD DDD` fact computed with `std::time` only (no new deps;
  weekday derived from Unix epoch, 1970-01-01 = Thursday).
- The fact is UTC and the helper documents two limits: day-boundary
  ambiguity for timezones far from UTC, and no wall-clock time (date
  granularity only, avoiding false precision).
- `infer()` uses `system_prompt()` in place of the inline literal.

### 4. `infer()` refactor (no behavior change except gaps 1–3)

- Keep: reset discipline, process-wide lock, 256-token budget, 64 KiB output
  buffer, single-call enforcement, `CString` handling.
- Replace the inline envelope interpretation (lines ~150–166) with a call to
  `decide_action`. Delete no safety checks; add no new FFI surface.

## Test plan (`crates/provider-needle`, no native library required)

Envelope fixtures through `decide_action` only:

- high-confidence single call → `CallTool` via `decode_call`
- low-confidence call → `Handoff`, reason contains the value
- missing/null/non-numeric confidence → `Handoff` (fail-closed)
- threshold boundary (equal → act; epsilon below → escalate)
- empty `function_calls` → `Handoff` with refusal reason (not `InvalidResponse`)
- multi-call → `InvalidResponse` (unchanged)
- malformed envelope (missing array) → `InvalidResponse` (unchanged)
- `validation.ungrounded` non-empty → `Handoff`, no execution path reachable
- `request_assistance` → `Handoff`, reason carries confidence when present
- `system_prompt()` contains a `date: \d{4}-\d{2}-\d{2} \w{3}` fact and the
  base instruction; documents UTC limitation in a code comment on the helper

## Verification gates (in order)

1. `cargo fmt --all` clean.
2. `cargo clippy -p provider-needle -p assistant-contracts --all-targets` clean.
3. `cargo test -p provider-needle -p assistant-contracts` green on the GNU
   toolchain (include the MSVC-linker-missing caveat only if it resurfaces;
   these crates have no C dependencies).
4. Full workspace check if time permits; no engine/UI changes means no
   orchestration or snapshot suites are affected.

## Explicit non-goals (not this build)

- Threshold calibration — needs on-device measurements against our tool
  catalogue (follow-up R&D 1).
- Persisted `tool_index_path` retrieval experiment (follow-up R&D 2).
- Multi-turn chaining inside Needle; reset-per-infer discipline stays.
- Fine-tune evaluation; Needle 3 migration.
- Engine, UI, or task-graph behavior changes.
- Any change to `vendor/needle/README.md` (upstream record, not ours).

## Follow-up R&D (tracked, not started)

1. Calibrate the threshold on our tools; replace 0.70 with the measured value.
2. Retrieval experiment: 15–20 declared tools ± persisted index; top-5
   inclusion accuracy + per-turn latency.
3. Fine-tune evaluation (only if calibration shows domain weakness).
4. Needle 3 migration watch (different ABI; queue behind 1–2).

## Acceptance

All four gaps closed behind tested code, gates green, and this file plus the
task-graph pending line updated to reflect completion. Live-device proof
(threshold behavior, date resolution, refusal flow on real hardware) remains
a UAT item, not a merge blocker — the unit contracts it depends on are proven
here.
