# Codex Skills Required Before Mobile UI

Create/install these project-specific skills before implementing polished UI.

Store version-controlled source under `dev/codex-skills/` and install into the Codex skill directory.

## mobile-product-ui

Rules:
- design mobile-first around a reference ~390x844 viewport
- safe areas
- keyboard avoidance
- touch-sized controls
- semantic color/type/spacing tokens
- light/dark mode
- accessible labels/contrast
- dynamic text wrapping
- loading/empty/offline/error/auth/retry/task-resume states
- avoid desktop dashboards and excessive nested cards
- thumb-reachable primary actions
- reduced-motion support
- visual review before completion.

## assistant-product-ux

Require explicit UI states:
- listening
- transcribing
- local routing
- cloud reasoning
- tool execution
- auth required
- approval required
- paused/resuming
- completed
- failed
- offline
- local model unavailable
- MCP disconnected.

## tauri-mobile-architecture

Enforce:
- React/TS presentation only
- Rust orchestration
- thin Tauri handlers
- Kotlin/Swift native plugins
- typed IPC
- no Python runtime in mobile
- no secrets in localStorage
- no orchestration buried inside React hooks.

## assistant-eval-engineering

Enforce:
- mocks/fixtures
- deterministic regression tests
- DeepEval harness
- routing/tool/skill evals
- latency/cost collection
- Needle-only vs Cloud-only vs Hybrid comparisons
- every discovered bug becomes regression coverage.

## security-review

Run a dedicated security review after first full integration:
- secret handling
- OAuth
- prompt injection
- tool approval
- MCP trust boundaries
- deep links
- unsafe FFI.
