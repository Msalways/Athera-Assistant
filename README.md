# AETHRA
## Adaptive Edge Task & Hybrid Reasoning Assistant

AETHRA is an Android-first, local-first personal assistant architecture.

Core idea:

- **Needle** = fast local reflex / tool router.
- **Cloud LLMs** = planning, reasoning, summarization, composition.
- **Rust orchestrator** = owns tasks, context, routing, policy, retries and execution.
- **MCP** = capability bus.
- **Composio initially** = OAuth-backed Gmail / Calendar and other SaaS integrations.
- **SQLite** = operational state.
- **Capability Search + Skills** = scale to thousands of tools without polluting model context.
- **Tauri 2 + React/TypeScript** = Android-first mobile shell.
- **DeepEval + deterministic tests + Maestro** = quality/evaluation stack.

The architecture must remain replaceable at every vendor boundary.

## Start here

1. Read `AGENTS.md`.
2. Read `docs/MASTER_BUILD_PLAN.md`.
3. Read `docs/CAPABILITY_ROUTING.md`.
4. Read `docs/AUTONOMOUS_AGENT_ARCHITECTURE.md`.
5. Read `docs/VERTICAL_SLICE_PLAN.md` for implementation order and worker handoffs.
6. Read `docs/ON_DEVICE_REASONING_RND.md` for the local-model research program.
7. Read `docs/WEB_RESEARCH_CAPABILITY.md` for the web research worker and source-provenance contract.
8. Read `docs/MCP_OAUTH_SECURITY.md` for MCP transports, authentication and secret handling.
9. Read `docs/MOBILE_UI_AND_CONFIGURATION.md`.
10. Read `docs/EVAL_STRATEGY.md` and `docs/AUTOMATED_EVAL_WORKFLOW.md`.
11. Use `codex-prompts/vertical-slice-worker.md` and `vertical-slice-reviewer.md` for bounded implementation.

## Tomorrow V0

The minimum convincing Android V0 should prove:

1. Tauri Android app launches on a physical ARM64 phone.
2. Rust AssistantCore receives a chat request.
3. Needle runs locally and can return a structured tool call.
4. OpenAI and one OpenAI-compatible provider adapter work behind the same interface.
5. MCP manager can connect to Composio.
6. Gmail + Calendar OAuth can pause and resume a task.
7. Context manager persists task/tool state in SQLite.
8. External writes require confirmation.
9. UI exposes Assistant, Activity, Connections, Models, Tools and Skills.
10. Eval harness compares Needle-only, Cloud-only and Hybrid behavior.

## Primary V0 demo

> "Check tomorrow's first meeting and see if Rahul emailed me anything about it."

Expected flow:

User -> task -> capability retrieval -> Needle/planner -> Calendar MCP -> Gmail MCP ->
context manager -> cloud reasoner only if needed -> final answer.

Second demo:

> "Reply saying I'll review it tonight."

Must resolve previous context, prepare the reply, require approval, then send only after approval.
