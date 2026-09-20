# Codex Worktree: Needle Android Runtime

Own provider-needle and vendor/needle.

Goal: prove real local Needle inference on a physical Android ARM64 device.

Steps:
1. fetch current upstream Android ARM64 artifact
2. inspect actual archive/header/API
3. create minimal Rust FFI
4. link it into Tauri Android
5. bundle current `.cact` model for V0
6. isolate all unsafe code
7. keep model warm
8. serialize calls initially
9. implement AETHRA ModelProvider adapter
10. capture latency.

Golden test:
prompt "turn the flashlight on"
tool schema set_flashlight(on: bool)
must return correct structured call.

Do not redesign AssistantCore if integration is difficult; stay behind ModelProvider.
