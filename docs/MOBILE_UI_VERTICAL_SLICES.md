# Mobile UI work packages for vertical slices

This is the executable UI track for the Android-first assistant. It deliberately
follows the backend contract work: a slice starts by agreeing a typed Rust payload,
then builds a fixture-backed React state, then connects the real dispatch command
and verifies the rendered state. React is presentation only; it never owns policy,
OAuth, retries, provider selection, tool execution, or secrets.

The primary review viewport is **390 x 844 CSS pixels**. Every screen must use
`env(safe-area-inset-*)`, keep a keyboard-visible composer above the visual viewport,
provide 44 x 44 px minimum interactive targets, preserve readable wrapped text, and
work in light theme, dark theme, and `prefers-reduced-motion: reduce`. Use existing
React, Tauri, Vitest, React Testing Library, and CSS; do not add a UI framework or a
second client state store.

`VERTICAL_SLICE_PLAN.md` owns the global VS order. The sections here are UI work
packages mapped as follows: Slice 0 -> VS00, Slice 1 -> VS01, Slice 3 -> VS03,
Slice 4 -> VS02/VS05, Slice 2 -> VS04/VS06, and Slice 5 -> VS09. A coordinator
assigns the relevant portion after its backend fixture is locked.

## Delivery rhythm and ownership

Work one slice at a time in this order. The backend owner lands the contract and
Rust tests first; the UI owner can build from the JSON fixture while that is in
review. At the rendezvous, replace only the fixture adapter with the real command or
event subscription, run the slice tests, and inspect the 390 x 844 rendering. A
slice is not complete when a mocked screen looks correct.

| Phase | Backend owner | UI owner | Rendezvous evidence |
| --- | --- | --- | --- |
| Contract | versioned Rust types and `Runtime::dispatch` command/event behavior | TypeScript mirror and fixture | serialized contract example passes Rust and TS tests |
| Presentation | deterministic test data only | component, CSS, accessibility tests | fixture screenshot at 390 x 844 in both themes |
| Integration | real store/event behavior | replace fixture transport only | command/event integration test |
| Device gate | Android build supports the state | rendered Android state | Maestro run and physical-device screenshot, clearly labelled |

Do not edit `crates/contracts` concurrently with another slice. Contract changes are
append-only where possible, include `schema_version`, and must tolerate an unknown
event/block by showing a safe “This update is unavailable in this version” row rather
than crashing. Tauri handlers in `apps/mobile/src-tauri/src/lib.rs` stay thin and
delegate to `app-runtime`.

## Common presentation contract

Retain existing `Snapshot`, `Task`, `Conversation`, `Message`, `ToolSpec`, and
`ResearchSession` as the baseline in `apps/mobile/src/types.ts`. Add these common
read models before slices that need them; Rust owns ordering and persistence.

| Contract | Minimum fields the UI may use | Notes |
| --- | --- | --- |
| `aethra.run-event.v1` | `schema_version`, `sequence`, `run_id`, `at`, `kind`, `status`, `summary`, `data` | Monotonic `sequence`; reconnect asks after its last acknowledged sequence. `summary` is user-safe; raw tool/MCP content is excluded. |
| `aethra.connection-state.v1` | `connection_id`, `service_name`, `capability`, `state`, `action_url?`, `resume_run_id?`, `message` | States: `connected`, `required`, `connecting`, `expired`, `denied`, `unavailable`. URLs are opened only by Rust/native authorization flow. |
| `aethra.output.v1` | `blocks`, `citations`, `uncertainty` | Typed blocks only: `markdown`, `source_table`, `comparison_table`, `timeline`, `metrics`, `chart`, `notice`. No model-authored HTML or chart code. |
| `aethra.diagnostics.v1` | `run_id`, `route`, `selected_capability_count`, `model_visible_tool_count`, `latencies`, `redacted_events` | Available only after an explicit developer-mode command. Never return secrets, full prompts, raw pages, headers, or authorization data. |

Initial event kinds are `run_started`, `route_selected`, `worker_progress`,
`tool_started`, `tool_completed`, `tool_failed`, `auth_required`,
`approval_required`, `block_delta`, `block_upserted`, `run_completed`,
`run_failed`, `run_cancelled`, and `outcome_unknown`. The conversation provider's
existing `Delta` and `Finished` events should be adapted into this envelope; do not
make a separate frontend streaming protocol.

## Slice 0 — responsive shell and setup truth

**Outcome:** launch always tells the user whether local conversation can be used and
gives one clear setup path. The normal Assistant tab remains free of diagnostics.

- **Backend contract and fixtures:** keep `model_status` and `snapshot`; fixture
  states are `missing_model`, `downloading` (known byte count), `verifying`, `ready`,
  `failed`, `unavailable`, plus offline bridge failure. The model installer remains
  user initiated. Never represent a fixture as a downloaded or working model.
- **Likely files:** `crates/contracts/src/conversation.rs`,
  `crates/app-runtime/src/local_models.rs`, `apps/mobile/src/types.ts`,
  `apps/mobile/src/App.tsx`, `apps/mobile/src/ConversationView.tsx`,
  `apps/mobile/src/Configuration.tsx`, `apps/mobile/src/styles.css`, and their
  adjacent `*.test.tsx` files.
- **UI acceptance:** the 390 x 844 layout has top and bottom safe-area padding; the
  four-item bottom navigation has 44 px targets; “Set up local model” is visible for
  a missing model; download has bytes/progress and a Cancel action; unavailable and
  offline explain recovery without a spinner; no microphone control is rendered.
  Theme colors meet readable contrast and pulsing animation stops under reduced
  motion.
- **Tests:** RTL covers every fixture and button command; CSS/render test confirms
  target classes and no horizontal overflow; Maestro covers launch, setup, cancel,
  unavailable, light/dark; device review captures keyboard closed and open.
- **Rendezvous:** use real `model_status`, `install_model`, and
  `cancel_model_download`; backend verifies model lifecycle transition tests before
  the UI removes its fixture transport.

## Slice 1 — streamed conversation and structured output

**Outcome:** ordinary local chat streams a readable assistant response, offers Stop,
and renders trusted structured result blocks rather than raw JSON.

- **Backend contract and fixtures:** extend `ConversationEvent` and dispatch with
  the common event envelope and `aethra.output.v1`; create a fixture transcript with
  user text, incremental `block_delta`, a completed markdown block, a failed run,
  and an interrupted response after app restart. Keep `get_conversation` as the
  recovery snapshot for missed events.
- **Likely files:** `crates/contracts/src/conversation.rs`,
  `crates/app-runtime/src/conversations.rs`, `crates/app-runtime/src/lib.rs`,
  `apps/mobile/src/service.ts`, `apps/mobile/src/types.ts`,
  `apps/mobile/src/ConversationView.tsx`, new small `OutputBlocks.tsx`,
  `apps/mobile/src/styles.css`, and tests beside them.
- **UI acceptance:** a generating message shows concise text activity and a Stop
  control; streamed text wraps and retains scroll position unless the user has
  scrolled away; completed output renders Markdown from a safe renderer and typed
  blocks; cancelled, failed, and interrupted state are distinct and explain the next
  action. Composer stays above the Android keyboard and has a 44 px Send target.
- **Tests:** event ordering/deduplication and reconnect-from-sequence; RTL checks
  delta-to-final rendering, Stop, failure, reduced motion, and keyboard class;
  Maestro types a long prompt, opens the keyboard, stops generation, and relaunches
  during an unfinished response.
- **Rendezvous:** Rust verifies persisted ordering and restart interruption; UI
  verifies real stream plus `get_conversation` recovery against one run ID.

## Slice 2 — action activity, tool workers, approvals, and uncertain outcomes

**Outcome:** the user can see what the assistant is doing without seeing tool JSON,
review the exact proposed write, and recover safely when an external result is
unknown.

- **Backend contract and fixtures:** adapt `Task`, `PendingAction`, and
  `AssistantEvent` to `aethra.run-event.v1`. Fixtures cover route selection, local
  worker progress, tool start/completion/failure, `waiting_for_user`, an email-like
  external write with recipient/subject/body, approval denial, cancellation, and
  `waiting_for_resolution` with `OutcomeUnknown`. Approval payload contains a
  display-safe action summary plus exact typed arguments; it never contains secrets.
- **Likely files:** `crates/contracts/src/lib.rs`, `crates/assistant-core/src/engine.rs`,
  `crates/app-runtime/src/lib.rs`, `apps/mobile/src/types.ts`,
  `apps/mobile/src/App.tsx`, `apps/mobile/src/TaskView.tsx`, new small
  `RunActivity.tsx` and `ApprovalSheet.tsx`, `apps/mobile/src/styles.css`, tests.
- **UI acceptance:** compact activity rows announce current worker/tool and elapsed
  progress; details disclose a sanitized timeline. Approval is a focus-trapped bottom
  sheet with action-specific label such as “Send email”, exact recipient/subject/body,
  Cancel, and Send; neither button is generic “Continue”. Unknown outcome says the
  action may have happened, disables replay, offers safe verification/cancel choices,
  and identifies no result as success. Cancel remains reachable while running.
- **Tests:** exact approval arguments and policy-approved command are asserted; a
  duplicate event does not duplicate a row; denied approval and outcome-unknown have
  separate accessible copy; Maestro completes a mock action and inspects the sheet.
- **Rendezvous:** backend supplies stable event IDs, action summary, and deterministic
  approval/outcome tests; UI connects `resolve_approval`, `cancel_task`,
  `answer_question`, and the event bridge.

## Slice 3 — connections and credential recovery

**Outcome:** a paused task explains exactly which capability needs authorization and
resumes only after the backend confirms a connection state change.

- **Backend contract and fixtures:** implement `aethra.connection-state.v1` through
  `snapshot`/events and an `authorize_connection` command that launches the native
  browser flow. Fixtures: disconnected Gmail, connecting, connected Calendar,
  callback success with `resume_run_id`, cancelled browser, denied scope, expired
  token, and provider unavailable. Credentials are represented only by state and
  secret reference status.
- **Likely files:** `crates/app-runtime/src/credentials.rs`,
  `crates/app-runtime/src/lib.rs`, `crates/adapter-mcp/src/`,
  `apps/mobile/src/types.ts`, `apps/mobile/src/Configuration.tsx`,
  `apps/mobile/src/TaskView.tsx`, `apps/mobile/src/App.tsx`, CSS/tests, and Android
  authorization wiring under `apps/mobile/src-tauri/src/android.rs` when needed.
- **UI acceptance:** Connections groups service capabilities and shows Connected,
  Connect, Expired, Denied, or Unavailable. Auth pause says what service is needed,
  why, and which task will resume. Connect opens the system browser through Rust;
  returning displays “Connected — resuming your task” only after the event arrives.
  API-key settings remain masked/session-only and cannot render the actual key.
- **Tests:** Rust exercises callback/expiry/denial and no secret serialization; RTL
  uses only state fixtures and verifies the browser command and resume state; Maestro
  uses a controllable callback fake, not a production OAuth account.
- **Rendezvous:** use a real callback only after the backend has a tested OAuth
  connection. A cancelled or expired callback must leave the task honestly paused.

## Slice 4 — research results, sources, tables, charts, and uncertainty

**Outcome:** research is legible on a phone, grounded in retrieved source records,
and makes partial, stale, or conflicting evidence visible.

- **Backend contract and fixtures:** implement `aethra.output.v1` and research event
  payloads specified in `WEB_RESEARCH_CAPABILITY.md`. Fixtures include search
  progress, selected/rejected sources, cited narrative, source table, wide comparison
  table, timeline, metrics/chart data, no connected provider, partial worker failure,
  disagreement, and stale source. Chart data is validated structured values plus a
  title/units; model strings never become chart code.
- **Likely files:** `crates/contracts/src/conversation.rs` or a new focused
  `crates/contracts/src/research.rs`, `crates/app-runtime/src/conversations.rs`,
  `apps/mobile/src/types.ts`, `apps/mobile/src/ResearchPanel.tsx`, new small
  `ResearchOutput.tsx`, `SourceList.tsx`, `OutputBlocks.tsx`, styles and tests.
- **UI acceptance:** final prose has inline source locators; source cards display
  title, canonical host, retrieval time, excerpt, and safe external-open action.
  Tables fit a 390 px screen by wrapping cells or using a labelled horizontal scroll
  region; charts have a text/table equivalent; uncertainty/disagreement is a visible
  notice. “Unavailable” is shown when no provider is configured. Raw pages,
  provider IDs, rejection reasons, and request data remain out of normal chat.
- **Tests:** schema-invalid blocks are rejected by Rust and rendered as a safe error
  state; RTL tests citations, table overflow affordance, chart alternative, and
  partial failure; Maestro captures narrow light/dark research results.
- **Rendezvous:** backend validates source/passage references and persists evidence;
  UI uses only validated block data and does not fetch URLs itself.

## Slice 5 — developer diagnostics, accessibility, and release evidence

**Outcome:** developers can inspect a redacted run separately from normal
conversation, while every user-visible state has repeatable browser and device proof.

- **Backend contract and fixtures:** add explicit developer-mode enablement and
  `aethra.diagnostics.v1`; fixtures include capability counts, selected-tool count,
  model route, redacted timings, and unavailable diagnostics. Keep normal snapshot
  unchanged unless diagnostics are requested.
- **Likely files:** `crates/app-runtime/src/lib.rs`, `crates/assistant-core/src/context.rs`,
  `apps/mobile/src/types.ts`, `apps/mobile/src/Configuration.tsx`, a new small
  `DeveloperDiagnostics.tsx`, styles/tests, `evals/` fixtures, and Maestro flows.
- **UI acceptance:** diagnostics live under Settings → Developer and are absent from
  Assistant/Activity. It displays route, bounded capability counts, selected tools,
  latency and sanitized event timeline; it never displays prompts, secrets, raw MCP
  results, headers, or full retrieved pages. All interactive elements are labelled,
  focus-visible, 44 px minimum, theme-correct, and motion-free when requested.
- **Tests:** redaction is exact in Rust tests; RTL checks developer opt-in and normal
  screen absence; accessibility checks cover names, focus, alerts, and contrast;
  Maestro records setup, conversation, action/approval, auth pause, outcome unknown,
  offline, research, and diagnostics at 390 x 844.
- **Rendezvous:** attach rendered browser evidence and a physical-device capture with
  device/Android version/date to `docs/INTEGRATION.md`; simulation must be labelled
  as simulation.

## Slice 6 — voice and document input states

**Outcome:** VS07 can accept speech and documents without hiding permission,
transcription, extraction or grounding failures.

- **Backend contract and fixtures:** consume typed availability, permission,
  recording, transcription, attachment extraction, source and cancellation events.
  Fixtures cover unavailable speech, denied microphone, recording, correcting a
  transcript, corrupt/oversized document, extracted source and injected document
  text. The UI does not parse documents.
- **Likely files:** `apps/mobile/src/types.ts`, `ConversationView.tsx`, a small
  `InputAttachments.tsx`, `ResearchPanel.tsx`, styles and adjacent tests. Kotlin owns
  the actual microphone and document picker.
- **UI acceptance:** the microphone appears only when available, recording always has
  an obvious Stop action, transcript text is editable before submission, attachment
  progress and failure are visible, and every sourced answer links to the extracted
  source record. The composer remains usable with the keyboard open.
- **Tests/rendezvous:** RTL covers all fixtures and cancellation; Maestro covers
  permission denial, correction and document selection. Physical speech/document
  evidence is required before VS07 is done.

## Slice 7 — durable workflow review and notification states

**Outcome:** VS08 turns a vague automation request into a reviewable schedule and
shows run history without pretending the phone is an always-on server.

- **Backend contract and fixtures:** consume typed trigger, timezone, capability
  leases, data-sharing summary, cost/budget, run status, notification rule and
  pause/delete outcomes. Fixtures cover draft, approval, next run, quiet hours,
  missed network, expired auth, duplicate event and server-owned schedule.
- **Likely files:** `apps/mobile/src/types.ts`, a focused `WorkflowReview.tsx`,
  `RunActivity.tsx`, `Configuration.tsx`, styles and tests.
- **UI acceptance:** review shows when it runs, where it runs, which data/tools leave
  the device, cost limits and notification behavior. Pause and Delete use exact
  labels; an external write inside a workflow still gets a separate approval.
- **Tests/rendezvous:** RTL checks timezone/DST copy, blocked auth and deduplicated run
  rows; Maestro covers create, pause and delete. Integrate only after the scheduler's
  virtual-clock tests pass.

## Verification workflow for a medium-model implementation agent

1. Read this file, `AGENTS.md`, `docs/MOBILE_UI_AND_CONFIGURATION.md`,
   `docs/WEB_RESEARCH_CAPABILITY.md`, and `docs/EVAL_STRATEGY.md`.
2. Choose the earliest incomplete slice. Inspect every likely file before editing.
   Do not refactor unrelated screens or add dependencies.
3. Make the Rust contract and serialization tests pass. Add a fixture matching its
   actual serialized shape. Do not claim fixture output is inference or OAuth.
4. Implement the smallest component(s) that render that fixture. Preserve existing
   `command()` as the only UI-to-Rust transport. Add RTL tests for success, empty,
   offline/error, auth-required, approval, and the slice-specific recovery state.
5. Replace the fixture adapter with the dispatch/event path. Verify sequence recovery,
   malformed data handling, and no rendered secrets. Run the appropriate Rust and
   frontend tests, format, lint, typecheck/build, inspect the diff, and remove debug
   output.
6. Render at 390 x 844 with keyboard closed/open, light/dark, and reduced motion.
   Capture a screenshot only as evidence after checking the live rendered UI. Run the
   matching Maestro flow when Android/emulator access exists; record blocked device
   work rather than calling it complete.

## Evaluation hooks

Keep model and system evaluation outside the production mobile application.
`evals/` owns DeepEval and the real Rust runner; fixtures call the real
`assistant-eval-runner`/`AssistantCore` interface and return route, selected
capabilities, events, task transitions, final typed output, and redacted metrics.
Each UI slice adds deterministic event/state fixtures that also become regression
inputs for this evaluator. DeepEval may judge goal quality and presentation
grounding, but exact checks gate event order, policy approval, no replay after
`OutcomeUnknown`, source/reference validity, disabled-tool non-use, and secret
redaction. UI tests validate rendering of the resulting states; they do not pretend
to measure model quality.

## Slice completion checklist

- Backend and TypeScript contract examples agree.
- Rust tests, frontend tests, formatting, lint, typecheck, and build pass for changed
  scope.
- At least one real dispatch/event integration path is exercised.
- 390 x 844 rendered states cover keyboard, safe areas, both themes, and reduced
  motion.
- Approval, auth, unavailable/offline, failure, cancellation, and uncertain outcome
  have distinct copy and controls where applicable.
- Normal conversation contains no diagnostics, raw tool output, secrets, or claims
  that a fixture or uncertain action succeeded.
- The rendezvous evidence and any live/device blocker are recorded before moving on.
