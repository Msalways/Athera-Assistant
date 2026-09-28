# Athera proof-assistant boundary and vertical-slice plan

Date: 2026-09-25
Status: active implementation boundary
Scope owner: agent + user-owned physical-device checks

## Purpose

Prove that the normal Athera app is a trustworthy assistant, not merely a set of
provider adapters and contracts. The first proof is deliberately narrow: one
configured cloud provider, one durable conversation, one real response, and
truthful recovery when the provider or bridge fails.

This boundary supersedes any attempt to prove the product through the abandoned
SMS POC. SMS code, SMS build modes, SMS scripts, and SMS visual artifacts are not
part of this proof and must not affect the normal app acceptance decision.

## User-visible proof

A user must be able to:

1. Open the normal app and understand whether Athera is connecting, ready, or
   blocked.
2. Follow one direct setup action to configure a real cloud provider.
3. Save a provider profile without exposing its secret in React, SQLite, logs,
   or diagnostics.
4. Restart the app and see the same active profile and usable credential state.
5. Send `hi` in the single composer and receive a real cloud response.
6. See accepted, working, streaming, completed, failed, blocked, cancelled, and
   timeout states without an indefinite generic spinner.
7. Retry a retry-safe failure without duplicating an accepted request.
8. Reopen the same conversation after restart and see its history and result.
9. Understand which provider is active and whether its credential is available.
10. Use the flow one-handed on a phone with keyboard, safe-area, dark/light,
    screen-reader, and touch-target support.

The proof does not require an external write. It must prove the assistant loop,
trust boundary, and recovery behavior before capabilities are added.

## In-scope architecture

The proof keeps the authoritative boundaries already chosen for Athera:

```text
React/Tauri shell
  -> typed Runtime command
  -> one profile/secret resolver
  -> one provider transport construction site
  -> bounded provider request
  -> durable task + terminal run event
  -> React renders authoritative task state
```

Rules for this boundary:

- Rust owns routing, provider selection, retries, task state, persistence, and
  normalized errors.
- React never calls a model, MCP server, Android SDK, or Keystore directly.
- The connection test and normal inference use the same auth, endpoint, model,
  transport, and error-classification path. The test may be smaller, but it may
  not silently use a different production request contract.
- Missing or unusable credentials fail closed with a typed recovery state.
- Every asynchronous operation reaches a terminal or explicitly retrying state.
- A task accepted by the runtime is not reported as failed merely because a
  subsequent UI refresh failed.
- No credential is accepted as a substitute for a different auth option or
  endpoint.
- The normal app build is the only APK considered by this boundary.

## Vertical slices

### Slice 0 — trustworthy app shell and artifact

- Normal `App` is the only mounted application surface.
- Build fingerprint identifies source, dirty state, build time, and frontend mode.
- Initial loading, bridge unavailable, provider setup required, ready, and
  working are distinct states.
- The composer has a truthful disabled reason and an explicit setup action.
- A stale snapshot cannot keep the app looking ready after bridge failure.

### Slice 1 — configure, verify, and restore one provider

- Curated provider choices are rendered from the Rust catalog.
- The form edits endpoint, model, auth option, and secret without provider-specific
  React branches.
- The secret is sent once to the native boundary and is never rendered back.
- Save and native secret persistence have an observable success/failure result.
- The same profile can be updated without re-entering a secret when the backend
  has a usable stored secret.
- An inactive profile can be loaded for repair; Active means selected, not
  silently assumed healthy.
- A profile without a usable credential cannot silently fall back to an
  unselected provider.
- Restart restores the profile, active selection, and credential state.

### Slice 2 — one real cloud conversation

- `hi` enters the same submit path as every later request.
- The provider request has a bounded deadline and cancellation path.
- A simple response is normalized to a terminal assistant result.
- Text deltas are visible while the task is running.
- The result is persisted in the conversation/task store.
- The normal assistant view restores the conversation after relaunch.

### Slice 3 — failure and recovery

The following scripted outcomes must each produce a distinct visible result:

- provider timeout;
- invalid/non-stream response;
- HTTP 401/403;
- HTTP 404/model unavailable;
- HTTP 429/quota;
- network unavailable;
- bridge/event polling failure;
- app restart during a running task.

Every path ends in `completed`, `failed`, `blocked`, `cancelled`, or an explicit
`retrying` state with a next action. No path may remain an unexplained spinner.

### Slice 4 — assistant-grade UI/UX

- First run has one obvious setup route, not a hidden Settings labyrinth.
- The assistant is conversation-first and the composer remains thumb reachable.
- Approval/auth copy explains consequence and recovery in plain language.
- Raw arguments, worker events, and provider diagnostics are secondary details.
- Settings are reachable at narrow widths without clipped tabs or hidden controls.
- Keyboard, safe areas, dynamic text, light/dark themes, focus, live status, and
  44px minimum touch targets are covered by tests and a physical-device pass.
- No optional offline model download is promoted in the first-run proof.

## Required automated evidence before device testing

The mobile device is not the only test boundary. The following host-side tests
are mandatory:

1. A real `Runtime::submit_input` path with a scripted provider returning text.
2. The same path with delayed, timeout, 401, 403, invalid-stream, and non-closing
   providers.
3. Real task/event SQLite persistence and startup reconciliation.
4. Provider profile save/update/activate/restore with a fake native vault.
5. Tauri command serialization and error propagation.
6. React rendering of every terminal and recovery state.
7. A package/asset smoke test proving the normal frontend bundle is embedded.
8. A regression test proving a pending promise cannot leave the UI busy forever.

Unit tests that only exercise a mocked component or an isolated fake provider
are necessary but are not sufficient evidence for this boundary.

## Device acceptance

The user-owned device check is limited to irreducible platform behavior:

- install/update identity and package/signature;
- Android Keystore persistence across restart/update;
- real WebView keyboard/safe-area behavior;
- actual provider network/TLS response;
- process death and relaunch;
- screen-reader/touch interaction;
- light/dark appearance.

Do not use `pm clear`, uninstall, or factory reset during the first acceptance
run. Those operations are separate destructive tests.

## Explicit non-goals for this proof

- SMS or the abandoned SMS build.
- Sending messages, email, or any other external write.
- Broad MCP connection management.
- Multi-provider failover beyond selecting one valid active profile.
- Optional large local-model download or offline conversation mode.
- Fine-tuning or replacing Needle.
- Adding new capabilities merely because the core assistant loop is not proven.
- Porting Google ARTEMIS into production. ARTEMIS may later be used as an external
  Android QA harness, but it is not a dependency of the Athera runtime.

## Definition of done

The proof is complete only when:

- the normal APK is built from an identified source state;
- the setup → restart → `hi` → response path passes on a real device;
- failure and recovery states are visible and retry-safe;
- the conversation is restored after relaunch;
- the UI/UX acceptance checks pass;
- automated host-side integration tests are green;
- the user has not needed to debug raw logs or understand provider internals to
  complete the happy path.

## Implementation progress

### 2026-09-25 — Slice 0/1 frontend foundation

Implemented and tested:

- bounded Tauri/fetch command timeouts;
- distinct connecting, online, degraded, offline, setup-required, and working
  states;
- first-run provider CTA with submission disabled until setup is complete;
- persisted active conversation ID and unsent composer draft;
- provider catalog loading, retry, empty, and field-validation states;
- inactive-profile editing and stored-secret-preserving updates;
- auth-option changes clearing incompatible secrets;
- confirmation before provider removal;
- human-first approval copy with technical details collapsed;
- direct connection recovery from an authentication pause;
- horizontally scrollable settings tabs, safe bottom insets, and mobile touch
  targets.

### 2026-09-25 — Rust proof path and normal Android artifact

Implemented and tested:

- one shared profile/transport construction site in `app-runtime`; the active
  profile is authoritative, unusable non-selected profiles are skipped, and an
  unusable explicitly active profile fails closed with its own typed blocker;
- connection testing now uses the same `RigCloudProvider` transport and auth
  construction as inference, via a bounded production `probe()` request;
- secret-valued config fields and known secret strings are rejected before
  persistence, and provider response bodies are never echoed into UI messages;
- `Assistant::recover_unfinished_tasks()` requeues only retry-safe ordinary work
  at startup, so an interrupted task cannot stay `Running` indefinitely.

Host evidence:

- `cargo check --workspace --all-targets` — passed;
- `cargo fmt --all -- --check` — passed;
- `cargo clippy --workspace --all-targets -- -D warnings` — passed;
- `cargo test --workspace` — passed, including the new
  `startup_recovery_requeues_an_ordinary_task_and_provider_failure_is_terminal`,
  `an_unusable_active_profile_does_not_fall_back_to_an_unselected_provider`,
  `a_selected_profile_without_a_key_pauses_with_its_own_blocker`, and
  `connection_test_messages_do_not_echo_provider_bodies` regressions.

Normal Android artifact:

- `scripts/build-android-apk.ps1` now selects the verified GNU host toolchain
  for the cross build and the NDK clang linker for the target, so the release
  build no longer depends on a missing MSVC `link.exe`;
- `artifacts/athera-mobile.apk` built successfully (68,276,420 bytes,
  SHA-256 `ac84b4a91a4cb6c707d84878f0f2dfedc482ca9442c99a1be130dd2ba6acd09e`);
- the APK is signed with the existing local development key so it can update an
  existing install without a reinstall.

Still open and user-owned: physical-device install/update identity, Android
Keystore persistence across restart, real WebView keyboard/safe-area behavior,
live provider `hi`, and screen-reader interaction. SMS and external writes stay
out of this boundary.

### 2026-09-26 — UI compatibility and density pass

The UI was re-audited by rendering the real production bundle in headless
Chrome at 320/360/412 px in both light and dark, plus landscape, short-viewport
(IME), and 1.5x root-font-scale, across every tab. That produced 58 measured
states. Defects found and fixed:

**P0 — primary actions could be unreachable**

The composer and bottom nav were both `position: sticky` inside a
`min-height: 100dvh` flex column, so they overlaid the scroll region. The
result was that the last action on a screen was hidden or half-cut on multiple
screens: "Retry after connecting" (the only auth recovery), "Save/Update
provider", the last activity row, and three preference actions in landscape.
The shell is now a fixed-height flex column with `main` as the single scroll
container; the composer and nav are normal flow items. The large
`padding-bottom` compensations that papered over the old behaviour were removed.
Nav overlap went from 8 states to 0.

**P0 — a render fault produced a permanent white screen**

There was no error boundary, so any throw during render blanked the app with no
recovery. `AppErrorBoundary` now wraps the root and offers a reload path.

**P0 — the Memory tab could render nothing**

`list_memories` was trusted blindly, so a non-array response threw inside the
map and blanked the screen. It is now shape-checked, and the tab has explicit
loading and empty states.

**P1 — the conversation opened at the wrong end and was not compact**

- The thread opened scrolled to the oldest message, leaving the newest — and
  often the only actionable one — behind the composer. It now opens at the
  newest turn and follows it.
- Status was printed twice (chip plus an identical message line). The
  redundant line is suppressed.
- Vertical rhythm was loose; gaps, task padding, and line-height were tightened
  and turns are now separated by a hairline instead of dead space.
- Answers are capped at `68ch` so landscape and wide viewports no longer stretch
  a line to 760px.

**P1 — markdown was printed literally**

Model output was dumped into a `pre-wrap` paragraph, so answers showed raw
`**bold**` and `- bullets`. A dependency-free renderer now handles headings,
bold/italic, inline code, links, and ordered/unordered lists as real elements.
It builds React nodes only — no `dangerouslySetInnerHTML` — and a test asserts
raw HTML in model output is not interpreted.

**P2 — smaller items**

- Provider inputs were 43px tall; they now meet the 44px minimum.
- The settings tab strip silently clipped "Diagnostics" at 360px. It now keeps
  its scroll position with a fade affordance and snap points so the hidden tab
  is discoverable.

Post-fix evidence: 58/58 measured states clean (no overflow, no clipped text, no
nav overlap, no sub-44px targets, no sub-4.5:1 text); 51 frontend tests pass
(6 new markdown tests); lint, Prettier, production build, and the mechanical
detector (`[]`) pass.

### 2026-09-26 — Web build verified against the real Rust runtime

The UI was also driven in a browser against the **real** `assistant-dev` bridge
(`crates/assistant-cli/src/dev.rs`) over the repo's own Vite proxy, rather than a
mock. That exercises the real `Runtime::dispatch`, real SQLite, real provider
catalog, and the real provider HTTP client. It immediately found a defect no
mock could have:

**P0 — first launch always looked broken**

The first proxied `snapshot` took **18.5s** (runtime boot, dependency
optimization); steady-state polls are 50–100ms. The 5s snapshot budget timed
out, so the app opened on "Connection degraded — the snapshot request did not
respond within 5 seconds" with sending disabled, and never recovered on its own.

Fixed: the cold-start budget is now separate from the steady-state poll
(25s vs 8s), and a slow first response keeps the app in an honest "Starting
Athera" state for up to three consecutive attempts before it is allowed to call
itself offline. A regression test covers it.

**Verified end-to-end against the real runtime, with a deliberately invalid key**

| Step | Result |
|---|---|
| First launch | "Setup required" with a direct "Connect a provider" route |
| Catalog | all 26 real providers render; unavailable ones are marked |
| Save profile | `openai` / `gpt-4o-mini` / `key_configured: true` |
| Activate | `cloud_credential: configured`, status "Ready" |
| Connection test | real network call; honest typed result — `failure_kind: Credential`, "Authentication was rejected. Check the API key." No response body echoed. |
| Send `hi` | task created, ran, and reached a terminal **"Connection required"** state — not stuck Running |
| Recovery | both "Open connections" and "Retry after connecting" visible and unobscured |
| Activity | "hi · Connection required · 1 step" plus a suggested next step |
| Exceptions | none |

This is the boundary's proof path executing for real: a wrong key produces a
truthful, actionable, terminal failure with working recovery — rather than a
spinner, a stuck task, or a leaked upstream body. The only thing still
unproven is a *successful* model response, which needs a real credential.

Also corrected: "1 workers" / "1 events" pluralisation in the execution summary.

Final gates: 52 frontend tests, lint, Prettier, production build, and the
mechanical detector (`[]`) all pass.

### 2026-09-26 - Live provider test with a real credential

Run against the real `assistant-dev` bridge and the real
`integrate.api.nvidia.com`, using a temporary key that was not written to the
repository and whose test database was destroyed afterwards.

**What worked.** A real profile saved, activated, and the production connection
probe reached NVIDIA and returned `success: true, latency_ms: 25731,
model_id: deepseek-ai/deepseek-v4.1-flash`. The credential path, the vault, the
transport, and the unary probe are real and correct.

**What did not.** The same task could not converse. Three defects:

**P0: the app can report Connected and then fail every real message**

The connection probe is unary (`stream: false`) and succeeded. The chat path
streams, and this model emits no SSE data at all: a direct stream returned
nothing in 100s while the identical unary request returned `Hi!` in 34s. Every
conversation therefore failed with "Provider response is invalid" while Setup
showed a green Connected badge. This is exactly the unary-test versus
streaming-chat contract split this boundary warns about, and it is not cosmetic:
a provider can pass setup and be unusable for its entire purpose.

**P1: a non-retryable failure is retried three times over about two minutes**

`InvalidResponse` is correctly non-retryable in `NormalizedError::is_retryable`,
but the engine still spends three full provider round-trips (each roughly 35s)
before terminating. The user watches a spinner for two minutes for a condition
that cannot improve on retry.

**P1: the terminal message names the wrong cause**

The task did reach `Failed`, with "Task stopped after repeated errors: Operation
timed out", even though the root cause was an unsupported streaming response
rather than a timeout. The last real error is overwritten by retry bookkeeping.

**Fixed during this test**

- HTTP 410 Gone (model retired) now classifies as `ModelNotFound` with an
  actionable message. It previously fell through to a generic provider error.
  Regression test added.
- The catalog NVIDIA help text recommended `meta/llama-3.1-8b-instruct`, which
  NVIDIA retired on 2026-08-26. It now points at the provider own model list.
- The connection probe hardcoded a 30-second budget, ignoring the configurable
  engine timeout, so any provider slower than 30s looked dead. It now uses
  `engine.timeout_seconds` (validated 1-120) and says the provider may still be
  starting up.

**Still unproven.** A successful `hi` end to end. That needs either a provider
that streams, or a chat path that does not hard-require streaming. Closed below.

### 2026-09-26 - Streaming contract unified, and `hi` proven end to end

A scripted OpenAI-compatible provider exercises the real runtime against
deterministic SSE without a vendor credential. Eleven scenarios ran through the
real bridge, the real streaming client, the real engine, and the real UI.

**The success path is proven.** The production bundle rendered `hi`, received a
streamed reply, and reached a terminal `Completed` state with a real output
block, `Execution - 14 events - 1 worker`, an Activity row reading
`hi - Completed - 1 step`, and zero page exceptions.

Streaming robustness confirmed: multi-frame deltas, one payload split across two
TCP writes, `reasoning_content`-only deltas followed by content, and a
deliberately slow stream all completed correctly.

**P0 fixed: setup could report Connected for a provider that cannot chat.**

The probe used a unary request while the assistant chats over a stream, so a
provider with an empty or absent stream passed setup and then failed every real
message. Reproduced deterministically: with an empty-but-valid stream and with a
zero-frame stream, the old probe returned `success: true` while chat always
failed. The probe now runs the same streaming contract as the assistant and
rejects a stream that yields no content:

> The provider connected but did not return a usable answer. It may not support
> streaming replies.

Connection test and normal inference now share one request contract, which is
what this boundary requires. Setup can no longer be green over a broken
assistant.

| Scenario | Probe | Chat |
|---|---|---|
| normal stream | Connected | completed |
| empty stream | rejected, typed message | failed |
| zero frames | rejected, typed message | failed |
| split frame | Connected | completed |
| reasoning-only | Connected | completed |
| slow stream | Connected | completed |
| error mid-stream | Connected | completed |
| 401 | Credential | waiting for auth |
| 404 and 410 | ModelNotFound | failed |
| 429 | Quota | failed, retry guidance |

**Known remaining defect.** Chat reports `Provider response is invalid` for a
404 or 410 where the probe reports the accurate `ModelNotFound`. The status is
lost in the third-party streaming client's error surface, not in Athera's own
mapping, so a correct fix is provider-side. The failure is still terminal and
honest; only its wording is imprecise.

Also still open: a non-retryable failure is retried three times before
terminating, and a terminal message can name a timeout when the cause was a
malformed response. Both are engine-level and independent of provider access.

Regression test `probe_request_uses_the_configured_model_bounded_output_and_the_chat_stream_contract`
asserts the probe streams. `cargo fmt`, `clippy -D warnings`, and the
provider-rig, app-runtime, and contract suites pass.

### 2026-09-26 - Ordered, bounded, attributed failover

`FailoverPolicy` existed in `contracts` with good guardrails but zero runtime
usage: validated, tested, and never executed. It is now wired, with the missing
rules supplied.

**Trigger rule.** Only transient, provider-independent failures may move
vendor: timeout, network unavailable, rate limit, quota, and 5xx. A rejected
credential, an unknown model, a bad endpoint, and a stream the provider could not
produce are the user's own setup; those are surfaced instead of hidden. The
rule is `failover::may_fail_over` and is covered directly by tests.

**Bounded.** At most the primary plus one fallback per turn
(`FAILOVER_MAX_ATTEMPTS = 2`), regardless of chain length. Never mid-stream.

**Attributed.** The answering provider is recorded and surfaced in the snapshot
as `failover.served_by`, alongside the configured chain. A fallback is never
silent. The handle is held on the Runtime so it survives reconfiguration, and
`AssemblyDeps` bundles it with the other long-lived assembly state.

**Credential-host binding.** Every chain member resolves its own profile and its
own stored credential; the primary's secret is never reused against another
host. A member that cannot bind its own credential does not join the chain.

**Opt-in.** With no policy the previous single-provider resolution is
unchanged. `set_failover_policy` and `clear_failover_policy` manage the chain,
and both refuse providers the user has not configured.

Verified against two real scripted hosts through the real runtime:

| Case | Result |
|---|---|
| primary 503 transient | failed over to secondary, answered |
| primary 401 | did NOT fail over, surfaced the credential fault |
| healthy primary | answered on the primary |
| policy cleared | single provider, unchanged |

Two real bugs were found and fixed while building this: holding the settings
read guard across `configure` deadlocked the command, and clearing the policy
wrote JSON null which was then read back as a malformed policy and broke the
whole snapshot. Both now have regression tests.

Still open: the UI has no failover configuration screen or per-turn provider
badge yet; the data is in the snapshot and the runtime enforces the policy.
Engine-level items from earlier remain: non-retryable failures retried three
times, and a terminal message that can blame a timeout for a malformed
response.

### 2026-09-26 - Fallover made visible and configurable

The previous slice enforced the policy but left it invisible: nothing told the
user a fallback had happened, and there was no way to set the chain from the
app. Both closed.

**Per-turn attribution.** Attribution moved off a process-wide "last answered"
value and onto the task (`Task.answered_by`). A global value is wrong the moment
two turns run at once - each would report whichever finished last. The chain
provider now emits `ProviderEvent::ProviderSelected` when a provider begins
answering; the engine records it on that turn and persists it. Verified that two
tasks in one session carry different `answered_by` values at the same time.

**Provider names come from Rust.** The snapshot's `failover.chain` carries
`{provider_id, display_name}` rather than bare ids, so the UI shows "NVIDIA NIM"
instead of a slug without a second catalog round trip.

**Never switch after text is on screen.** A genuine defect found while building
this: a provider that streamed partial text and then dropped the connection
produced a transient-looking error, so the chain moved to the second vendor and
spliced one vendor's words into another's answer. The chain now watches for
streamed text and treats any later failure as terminal. Proven with a
`socket.destroy()` mid-stream: the turn fails and stays on the primary.

**Configuration screen.** `FailoverSettings` sits under Settings > Models. It
only offers providers that already have a stored key, defaults the first slot to
the active provider so enabling a fallback does not move ordinary traffic, and
states the rules in the UI rather than hiding them.

**Badge.** A muted pill shows which vendor answered; when it was the fallback the
pill turns amber and reads "Fallback · <name>", with the reason in its tooltip.

Verified in the production bundle against the real runtime, dark theme, zero page
exceptions: settings section renders with stacked 44px controls, fallback turns
show the amber pill, healthy turns show the muted pill.

One layout bug was caught only by looking at the screenshot: the section was
missing the `settings-form` class, so its labels sat side by side and its
selects were under the 44px touch target. The unit tests could not have seen it.

Still open: a turn that fails mid-stream reports the generic "The cloud model is
unavailable. Check the provider settings" message, which blames settings when the
real cause was a dropped connection, and it does not tell the user that the
partial text they read was discarded. Same family as the known wrong-cause
terminal message defect.

### 2026-09-26 - Web verification pass (before device)

Full sweep of the production bundle against the real runtime: every view, every
settings tab, and every conversation state, in dark and light, at 430x932.
Result: zero page exceptions, zero console errors, zero failed requests, and no
layout defects (no horizontal overflow, no sub-44px touch target, no clipped
text) across 11 views and 7 conversation states.

The pass found two real defects that the `hi` proof could never reach.

**Tool calling was completely broken against real streaming providers.** A
streamed tool call was parsed into two tool calls and then rejected as
"Provider response is invalid", so approval could never be reached. Two causes,
both fixed in `provider-rig`:

1. `merge_tool_delta` only merged an id-less delta into a pending call whose own
   id was empty, so the ordinary OpenAI shape (name first, then id-less
   argument fragments) started a second call instead of continuing the first.
2. The root cause: for one tool call the stream yields a complete tool call
   *and* the same call again as deltas, each with a different identifier.
   Identical calls now collapse to one in `finalize_tool_calls`. Confirmed by
   logging the parsed response: `calls=[("tomosFYtwPnRMfCPCCAkS", ...),
   ("call_scripted_1", ...)]` for a single requested call.

Verified against four different tool-call encodings, all now reaching
`waiting_for_approval`, and the full UI path: approve, deny, and cancel.

**A tool that could not run was reported as a cloud-model failure.** A tool with
no available connection produced "The cloud model is unavailable. Check the
provider settings and connection", sending the user to the wrong screen. The
tool step now reports its own cause, naming the tool and the connection it
needs. Regression test `a_tool_that_cannot_run_is_not_blamed_on_the_cloud_model`.

**Markdown tables, fenced code, and blockquotes rendered as raw text.** The
dependency-free renderer handled headings, lists, and inline marks only, so a
table answer showed literal `|---|` and code showed literal backticks. All three
are now real elements, with tables and code scrolling horizontally instead of
widening the page. Nine new tests, including that markup inside a code fence is
never interpreted and an unterminated fence does not swallow the answer.

Both defects were invisible to unit tests that only ever exercised a text reply.
The tool-calling bug in particular would have made every real tool use fail on a
physical device, and nothing in the `hi` proof touched it.

**Device build is stale.** `artifacts/athera-mobile.apk` predates all of the
above, and the committed Android WebView assets carry older bundle hashes than
`dist`. A device test must start with a fresh `scripts/build-android-apk.ps1`
run, or it will exercise code that no longer exists.
