# Parallel Codex Execution

> Historical horizontal worktree plan. New work follows
> `VERTICAL_SLICE_PLAN.md`; use its bounded worker/reviewer prompts and exclusive file
> ownership. Keep this file only as background for the original crate ownership.

Create isolated worktrees/agents with strict ownership.

## A — foundation
Owns:
- root workspace
- AGENTS.md
- contracts
- CI skeleton
- ADR/docs scaffolding.

Acceptance:
workspace compiles, shared contracts documented, CI commands defined.

## B — mobile-ui
Owns:
`apps/mobile/src/**`

Use mocks only initially.

Build:
Assistant, Activity, Connections, Models, Tools, Skills, Settings, approval/auth states, developer routing debugger.

Acceptance:
mobile visual states work independently from backend integration.

## C — agent-core
Owns:
assistant-core, model-router, tool-registry, capability-registry, policy-engine.

Use mock providers/MCP.

Acceptance:
multi-step loop, model escalation, policy and loop protection tested.

## D — needle-runtime
Owns:
provider-needle, vendor/needle, Android native integration.

Acceptance:
real ARM64 physical-device local inference returns correct tool JSON and latency.

## E — cloud-models
Owns:
provider-openai, provider-openai-compatible.

Acceptance:
same ModelProvider contract, normalized errors/usage, OpenAI + configurable compatible endpoint.

## F — mcp-auth
Owns:
mcp-manager, auth-manager, Composio adapter, deep-link plumbing.

Acceptance:
mock MCP tests plus real Gmail/Calendar OAuth proof; task pause/resume contract.

## G — context-storage
Owns:
context-manager, storage-sqlite, migrations.

Acceptance:
task/message/tool persistence, model-specific context bundles, crash-safe transaction behavior.

## H — evals
Owns:
evals, fixtures, Maestro.

Acceptance:
deterministic smoke suite + DeepEval starter datasets + comparison report generator.

## Integration order

1. foundation
2. core + context + UI shell
3. providers
4. MCP/auth
5. Needle
6. real UI events
7. eval/test pass
8. physical Android demo
9. security pass.

Never block UI on Needle.
Never block core on OAuth.
Use shared contracts + mocks.
