# AETHRA

## Adaptive Edge Task & Hybrid Reasoning Assistant

AETHRA is an Android-first, Needle-local-first and provider-neutral personal
assistant. It turns a user's request into a safe, durable and verifiable job without
exposing model or tool selection as product modes.

Core idea:

- **Needle** = the local-first reflex for intent, supported routing and extraction.
- **Cloud LLMs** = replaceable providers for work beyond Needle's declared ability.
- **Offline chat model** = optional user-installed pack, never an onboarding requirement.
- **Rust orchestrator** = owns tasks, context, routing, policy, retries and execution.
- **MCP** = capability bus.
- **Composio initially** = OAuth-backed Gmail / Calendar and other SaaS integrations.
- **SQLite** = operational state.
- **Capability Search + Skills** = scale to thousands of tools without polluting model context.
- **Tauri 2 + React/TypeScript** = Android-first mobile shell.
- **DeepEval + deterministic tests + Maestro** = quality/evaluation stack.

The architecture must remain replaceable at every vendor boundary.

## Start here

1. Read `AGENTS.md` for engineering rules.
2. Read `docs/ATHERA_JARVIS_BUILD_PLAN.md` — authoritative product and build plan.
3. Read `docs/ATHERA_DEVELOPMENT_TASK_GRAPH.md` — dependency-aware task graph.
4. Read `docs/CAPABILITY_ROUTING.md`.
5. Read `docs/AUTONOMOUS_AGENT_ARCHITECTURE.md`.
6. Treat `docs/MASTER_BUILD_PLAN.md` as historical input; it is superseded by the
   authoritative plan above.
7. Read `docs/ON_DEVICE_REASONING_RND.md` for optional large-local-model research.
8. Read `docs/WEB_RESEARCH_CAPABILITY.md` for the web research worker.
9. Read `docs/MCP_OAUTH_SECURITY.md` for MCP transports and auth.
10. Read `docs/MOBILE_UI_AND_CONFIGURATION.md`.
11. Read `docs/EVAL_STRATEGY.md` and `docs/AUTOMATED_EVAL_WORKFLOW.md`.

## First product gate

Before adding more capabilities, the installed Android APK must prove:

1. The displayed build fingerprint matches the source being tested.
2. A user can configure and test an OpenAI-compatible cloud provider.
3. The API key is kept through Android Keystore until expiry, revocation or deletion.
4. `hi` receives a real cloud-model response through the Rust runtime.
5. Provider credential failure is distinct from connector OAuth failure.
6. The assistant uses one composer with no Conversation/Action mode selection.
7. No offline model is required or promoted during this flow.
8. Restart, cancellation and normalized provider errors are exercised on the device.

## Primary V0 demo

> "Check tomorrow's first meeting and see if Rahul emailed me anything about it."

Expected flow:

User -> task -> capability retrieval -> Needle/planner -> Calendar MCP -> Gmail MCP ->
context manager -> cloud reasoner only if needed -> final answer.

Second demo:

> "Reply saying I'll review it tonight."

Must resolve previous context, prepare the reply, require approval, then send only after approval.

## Building

### Prerequisites

- Rust stable toolchain with `aarch64-linux-android` target
- Node.js 24+
- Android SDK 36 + NDK r27c
- Tauri CLI 2

### Quick start

```powershell
# Bootstrap development environment
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bootstrap.ps1

# Full check (format, lint, tests, eval)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check.ps1

# Development server
npm run dev
```

### Android build

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-sms-apk.ps1
```

## Repository structure

```text
crates/
  contracts/           Vendor-neutral type boundaries (no provider/vendor leakage)
  assistant-core/      Orchestration, routing, policy, context, verification
  app-runtime/         Composition root; constructs adapters, exposes use cases
  storage-sqlite/      SQLite persistence, migrations, FTS5
  provider-cloud/      OpenAI cloud adapter (to be replaced by provider-rig)
  provider-needle/     Needle on-device provider
  provider-local-chat/ llama.cpp offline provider (experimental only)
  adapter-mcp/         MCP client, OAuth discovery, capability bus
  assistant-cli/       CLI evaluation runner

apps/mobile/
  src/                 React/TypeScript mobile UI
  src-tauri/           Tauri shell + Android native bridge

evals/                 Python evaluation tooling, fixtures, datasets
docs/                  Authoritative architecture and plan documents
scripts/               Build, dev, and CI scripts
```

## Coding conventions

- **Rust**: `unsafe_code = "forbid"`, `thiserror` for errors, `serde` with `rename_all`,
  `async_trait` for async traits, `uuid` for identifiers. No vendor-specific types in
  contracts. Format with `cargo fmt`, lint with `cargo clippy`.
- **TypeScript**: React 19, Vite 7, Vitest, strict TypeScript. Component files are
  PascalCase, utility files are camelCase. Use `lucide-react` for icons.
- **CSS**: Mobile-first, CSS custom properties, `prefers-color-scheme` dark mode,
  `env(safe-area-inset-*)` for notched phones, 44px minimum touch targets.
- **Docs**: Authoritative plans live in `docs/`. `AGENTS.md` is the engineering rule
  set. Historical plans are marked superseded.
