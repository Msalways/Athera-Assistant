# Android acceptance

Status: **not verified**. The user will connect a physical phone after development;
there is currently no emulator. Do not infer Android support from host tests.

Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/android-preflight.ps1`.
The report is written to `artifacts/android-preflight.json`. It inspects SDK/NDK and
connected devices without installing an APK or executing a phone action. Device
serials are not written to the report. An emulator cannot satisfy physical acceptance.

The inspected SDK currently has no NDK installed. Install the supported NDK and
ARM64 Rust target before initializing/building Tauri Android. The local llama CLI
adapter is a host feasibility path; it does not by itself supply mobile inference.
Native in-process integration, Android permissions, Keystore, speech, share intents
and notifications require their own implementation/build/device checks.

For each 6 GB physical-device run record Android API, model/runtime hashes, APK hash,
signing/build provenance, cold and warm first-token latency, actual generated tokens,
tokens/sec, whole-app peak RAM, thermal state, crashes and a 20-minute session log.
The initial goals are <=3-second warm first token, >=8 tokens/sec and zero session
crashes. Treat 4 GB as a separate tier; do not infer support from a 6 GB pass.

Before delivery exercise airplane mode, missing/corrupt model, download cancellation,
stream cancellation, process death, memory pressure, denied permissions, revoked auth,
stale approval and uncertain external-write recovery. Record actual results, including
misses. A signed APK and physical-device demo remain release blockers.
