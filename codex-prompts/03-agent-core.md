# Codex Worktree: Agent Core

Own assistant-core, model-router, capability/tool registries and policy engine.

Implement:
- ModelProvider interface
- AgentAction protocol
- deterministic TaskEngine
- ModelRouter
- loop protection
- policy risk classes/approval transitions
- CapabilityRegistry interfaces
- mocks.

Do not import vendor SDK types in AssistantCore.

Tests must cover fast path, escalation, repeated failure, approval and provider fallback.
