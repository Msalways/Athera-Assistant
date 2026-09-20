# Codex Worktree: Mobile UI

Install/read the project Codex skills first, especially mobile-product-ui and assistant-product-ux.

Own `apps/mobile/src/**`.

Build polished mobile-first mock-backed screens:
- Assistant
- Activity
- Connections
- Models
- Tools
- Skills
- Settings
- approval sheet
- OAuth-required/resuming state
- local-model state
- developer capability/model routing debugger.

Do not implement orchestration in React.
Use typed service interfaces that can later bind to Tauri IPC.
Test important UI states with Vitest/RTL.
