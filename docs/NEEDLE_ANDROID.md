# Needle Android Integration

## V0 strategy

Support Android ARM64 first.

Do not run Python on Android.

Use the current Needle Android native distribution and call the C ABI from Rust.

Before implementation:
1. Download current Android ARM64 artifacts.
2. Inspect exact files and header.
3. Inspect current API signatures.
4. Do not assume file names from old docs.

## Runtime responsibilities

Separate:

- `NeedleModelManager`: installed model/version/checksum/path.
- `NeedleRuntime`: native engine loading/session.
- `NeedleProvider`: implements AETHRA ModelProvider.

## V0 model delivery

Bundle the `.cact` model with the development APK so first-run downloading cannot block the prototype.

Post-V0:
- manifest
- first-run download
- checksum verification
- atomic installation
- version updates
- remove/update UI.

## Runtime lifecycle

Conceptually:

model bytes/file -> native load -> initialize system facts/tools -> complete -> complete -> reset.

Keep the runtime warm after loading.

Serialize calls initially unless upstream explicitly guarantees safe concurrent independent sessions.

## Needle context

Needle should receive:
- current goal
- current step
- minimal state
- latest result summary
- relevant system facts
- small selected tool set.

Never send:
- full conversation history
- complete MCP catalogue
- full Gmail result sets
- long cloud reasoning output.

## First physical-device test

Tool schema:
`set_flashlight(on: bool)`

Input:
`turn the flashlight on`

Success:
- inference occurs locally
- valid JSON/structured envelope
- correct tool name
- correct boolean argument
- latency captured.

The actual flashlight can be mocked during this spike.

## Fallback

If local Needle cannot initialize:
- mark provider unavailable
- keep the app usable
- ModelRouter uses configured fallback
- show sanitized UI state.
