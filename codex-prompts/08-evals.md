# Codex Worktree: Evaluation

Own `evals/**`, fixtures and Maestro flows.

Build `assistant-eval-runner` integration contract against the real Rust core.

Create deterministic golden datasets covering:
routing, capability retrieval, skill routing, tool choice, arguments, multi-step, pronoun context, OAuth resume, approval, prompt injection, provider failure, offline behavior.

Use DeepEval where appropriate for agent/tool/multi-turn quality, paired with exact assertions.

Create Needle-only vs Cloud-only vs Hybrid report:
success, tool accuracy, P50/P95, cloud calls/task, tokens/task, cost/task.

Add 10/100/1000/5000 synthetic-tool context pollution eval.
Do not invent benchmark values.
