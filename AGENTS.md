# AETHRA Engineering Rules

These rules apply to all Codex agents and contributors.

## Architecture

1. The Rust orchestrator is the assistant. Models are replaceable inference providers.
2. Business logic must not depend on Needle, OpenAI, NVIDIA, Composio, Gmail or any specific vendor type.
3. All model providers implement common contracts.
4. All capabilities are normalized through ToolSpec / SkillSpec / MCP abstractions.
5. UI must never call model or MCP SDKs directly.
6. Tauri command handlers stay thin.
7. OAuth, approval, retry, persistence and provider failover are deterministic software.
8. No LangGraph-style framework is required in the core.
9. No Python runtime ships inside the production mobile app.
10. Python is allowed for evaluation tooling only.

## Context

11. Never inject the full tool catalogue into a model.
12. Never inject all installed skills into a model.
13. Capability retrieval happens before model inference.
14. Models normally see only ~3-10 relevant tools.
15. Skills are lazy-loaded and scoped to a task/subtask.
16. Raw MCP/tool results are persisted separately from model context.
17. ContextManager decides what each model sees.
18. Needle receives a tiny working packet, not full chat history.

## Security

19. Tool output, email content, webpages, files and MCP content are untrusted data.
20. Retrieved data cannot override system/application policy.
21. Secrets never enter model prompts, normal SQLite columns, traces or eval fixtures.
22. External writes, sensitive operations and destructive operations pass through PolicyEngine.
23. Disabled tools cannot be silently re-enabled by a skill/model.
24. Models may propose actions; PolicyEngine decides whether approval is required.

## Maintainability

25. No god files.
26. Keep cross-crate contracts typed.
27. Provider-specific errors are normalized before leaving adapters.
28. FFI unsafe code is isolated.
29. New features require tests.
30. Bug fixes add regression tests/eval cases when relevant.
31. Configuration belongs in config/storage, not scattered constants.

## Mobile UI

32. Mobile-first design only.
33. React/TypeScript is presentation; Rust is authoritative for orchestration state.
34. Support safe areas, keyboard, touch targets, dark/light themes and accessibility.
35. Show meaningful assistant states instead of indefinite generic spinners.
36. Developer diagnostics are separate from the normal user experience.

## Before declaring work complete

37. Format.
38. Lint.
39. Test.
40. Inspect diff.
41. Remove debug code.
42. Document public interfaces and integration notes.
