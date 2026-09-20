# Offline-Capable Phone Companion

The app name remains undecided. This is the authoritative product-scope plan.
`VERTICAL_SLICE_PLAN.md` defines implementation order and worker handoffs. Unique task
IDs, owners, status, evidence and blockers live in TASK_INDEX.md; interfaces and
checks live in INTEGRATION.md. Existing task, approval and cloud settings are
preserved.

## Outcome and defaults

English-first conversation, constructive debate, research and approved phone actions.
`AUTONOMOUS_AGENT_ARCHITECTURE.md` defines the runtime, model-routing and protocol
boundaries for general autonomous tasks.
Rust is authoritative, React presents state and models remain replaceable adapters.
Local conversation works without cloud. Needle receives scoped action objectives,
not every conversational turn. Optional cloud assistance must be explicitly enabled.
Target Android 12+ ARM64, initially 6 GB RAM; measure 4 GB independently.
No paid fallback, mandatory cloud, always-listening mic, unrestricted screen
automation or production Python runtime.

## Build sequence

| ID | Deliverable | Dependencies | Acceptance gate |
|---|---|---|---|
| E00 | Documentation and baseline | None | Unique authoritative ledger |
| E01 | Conversation/inference/streaming/routing contracts | E00 | Contract tests and compatible migration |
| E02 | Native local chat and model lifecycle | E01 | Verified model, offline host chat, cancellation |
| E03 | Android build and inference feasibility | E02 | Installed APK and physical-device report |
| E04 | History, summaries and explicit memory | E01 E02 | Restart/resume, inspect/delete, bounded context |
| E05 | Local/Needle/cloud coordination | E02 E04 | Tool-free chat, safe actions, bounded escalation |
| E06 | Phone-first conversation/setup UX | E01 E04 E05 | Complete flow and rendered visual QA |
| E07 | Native capabilities and share input | E03 E05 | Permission-aware execution and honest outcomes |
| E08 | Research, sources and versioned notes | E04 E06 | Resume sourced discussion and revise conclusions |
| E09 | Dynamic tools, scoped skills and authentication | E05 E07 | Live unfamiliar-tool discovery |
| E10 | Foreground voice | E06 E07 | Speak, interrupt, correct and fall back to text |
| E11 | Opt-in reminders/proactive events | E07 | Quiet hours, deduplication, restart recovery |
| E12 | Security, evaluation and signed release | All | Evidence-backed APK and reproducible demo |

Implement E00-E03 first. Continue independent host work if device access is blocked.
Implementation, host, live-provider and physical-device verification are distinct
columns/gates. Mock evidence cannot satisfy a live or physical-device gate.

## Implementation decisions

- Start with Qwen3-1.7B Q4_K_M GGUF, non-thinking conversation and 4,096-token
  context through isolated native llama.cpp. Pin artifact/runtime revisions,
  licenses and checksums. This is an initial benchmark configuration, not a speed
  promise. Sources: https://huggingface.co/Qwen/Qwen3-1.7B and
  https://github.com/ggml-org/llama.cpp/blob/master/docs/android.md.
- User-initiated model download discloses size, progress and cancellation. Verify
  checksums and install/remove atomically. Serialize generation, support interruption
  and unload on memory pressure. Missing models show setup without silent cloud routing.
- Add vendor-neutral conversation/message, memory, research, provider-availability
  and installation contracts. Tauri and the real Rust browser bridge share dispatch.
- Retrieve relevant capabilities before action inference. Local chat can propose an
  action; Rust validates it. Needle sees a bounded objective and relevant schemas.
  PolicyEngine remains responsible for sensitive, destructive and external writes.
- One bounded cloud handoff may follow invalid proposals, repeated lack of progress
  or explicit deeper assistance. Apply request/token limits and rate-limit cooldowns;
  continue locally where possible. Never replay uncertain external writes.
- Preserve NVIDIA settings; never assume free service. Additional explicitly enabled
  clouds require endpoint/model/free-tier verification. Only approved, minimized
  context packets may leave the device.
- Store conversations separately from raw tool results. Retrieve bounded recent
  messages, summaries and relevant confirmed memory. Durable personal facts require
  confirmation and inspection/edit/delete. Temporary conversations do not persist.
- Conversation is the default UI. Keep history and focused research notes accessible.
  Provide streaming, stop, retry, approvals, recovery and model setup; validate one
  complete flow before expanding the redesign.
- First native tools: local notes/reminders, flashlight, share input, supported timer,
  calendar and dial intents. Kotlin owns permissions. Normalize through ToolSpec and
  distinguish opened-for-confirmation from completed execution.
- Research accepts shared documents/URLs and optional configured search. Preserve
  source references, hypotheses, decisions, experiments and note revisions. Without
  retrieval, label discussion as unsourced reasoning; never invent citations.
- Test live MCP discovery, enablement, schema/version changes, scoped skills and auth.
  Calendar OAuth is first; retain blockers until credentials and callbacks are tested.
- Push-to-talk and opt-in spoken replies use replaceable Android speech/TTS adapters
  with explicit availability; offline recognition is not promised. Proactivity v1
  covers opted-in app reminders and task-completion notifications.
- Android credentials require Keystore-protected encryption. Secrets/OTPs stay outside
  prompts, normal SQLite, memory and traces. OTP retrieval is a separate experiment:
  https://developer.android.com/about/versions/15/behavior-changes-all.

## Verification and release

Version a 60-scenario set covering conversation, disagreement, continuity, actions,
research grounding and recovery. Compare local-only, local+Needle and optional cloud.
Require zero unauthorized writes and no known-secret leakage in security fixtures;
model accuracy is independent of software security enforcement.

Measure cold/warm first token, tokens/sec, whole-app peak RAM, crashes and thermal
behavior on physical devices. Initial 6 GB goals: warm first token <=3 seconds,
>=8 tokens/sec and no crash over 20 minutes. Report misses; measure 4 GB separately.
Exercise offline use, quotas, missing models, interrupted downloads, denied permissions,
revoked credentials, stale approvals, malicious results, cancellation and process death.
Run Rust format/Clippy/tests and frontend format/lint/typecheck/tests/build. Inspect
mobile/desktop, keyboard, safe areas, touch targets, accessibility and both themes.
Only physical-device evidence satisfies Android acceptance; label browser simulations.

Deliver a signed personal APK, setup/recovery notes, benchmark report and demo of
offline conversation, approved action, research continuity and cloud-quota recovery.
Release remains open until these artifacts and their evidence exist.

## Coordination

Use bounded gpt-5.6-sol agents at medium reasoning. Settle contracts/migrations before
parallel work, separate runtime/model, UI and verification ownership, and avoid
concurrent shared-contract edits. Each handoff records files, tests, risks and evidence.
