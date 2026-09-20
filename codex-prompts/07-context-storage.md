# Codex Worktree: Context + Storage

Own ContextManager, SQLite repositories and migrations.

Persist conversations, messages, tasks, steps, model calls, tool calls/results, summaries, connection/config metadata.

Implement model-specific context builders:
- Needle tiny working packet
- planner
- reasoner
- responder.

Raw large tool outputs remain stored outside prompts.
Use progressive retrieval.
No secrets in normal DB columns.
