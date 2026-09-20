# Codex Worktree: MCP + OAuth

Own MCP manager, auth manager and Composio adapter.

Implement generic server/tool discovery and normalized calls.

Initial real integrations:
- Gmail
- Google Calendar via Composio.

OAuth must:
- set task WAITING_FOR_AUTH
- open system browser
- handle deep-link callback
- resume same task/step automatically.

Never expose credentials to models/logs.

Include mock MCP/auth tests before live integration.
