# RIG-001 Rig Android ARM64 Compile/Spike Report

Date: 2026-09-22
Rig version: 0.42.0
Target: aarch64-linux-android
Status: **partial -- dependency compile evidence exists; runtime and packaged-APK gates remain**

## 1. Compile results

| Build | Result | Time |
| --- | --- | --- |
| `cargo check` (native) | Pass | ~2m30s |
| `cargo check` (ARM64 Android) | Pass | ~2m04s |
| `cargo build --release` (ARM64 Android) | Pass | ~4m26s |

The recorded commands found no compile or linker failure. This does not prove a
TLS request on Android because the current adapter does not yet instantiate a
Rig client.

## 2. Binary size measurement

These are rlib sizes (uncompressed, pre-link). Android APK .so sizes will be
smaller due to strip/LTO but proportionally similar.

| Crate | rlib Size | rmeta Size |
| --- | --- | --- |
| `rig-core` | 47.6 MB | 33.5 MB |
| `reqwest` | 3.6 MB | 0.9 MB |
| `tokio` | 7.7 MB | 6.0 MB |
| `tokio-util` | 0.7 MB | 0.6 MB |
| `provider-rig` | 4.2 KB | — |

**Estimated APK impact**: rig-core + reqwest + tokio ≈ **60 MB** pre-strip/release.
With LTO (`opt-level = "s"`) and `strip = true`, expect **8-15 MB** final .so
depending on how many providers are compiled in.

## 3. Feature flags used

```toml
rig-core = { version = "0.42", default-features = false }
```

This is the **minimal** set: no agent runtime, no vector stores, no media
features, no derive macros, no websocket. Only the provider-neutral completion
contracts and built-in provider mappings.

## 4. Dependency tree notes

- `rig-core` depends on: `reqwest`, `serde`, `serde_json`, `schemars`, `sha2`,
  `url`, `http`, `tracing`, `futures`, `async-stream`, `ordered-float`, `glob`,
  `mime_guess`, `fastrand`, `futures-timer`, `base64`, `as-any`, `thiserror`
- No native/C dependencies at the provider level
- TLS provided by reqwest (rustls by default)

## 5. Provider count at this feature set

With `default-features = false`, rig-core still compiles all 26 provider modules
(they are always present as module definitions). The actual HTTP clients for each
provider are only instantiated when you call them.

For a leaner build, individual providers could be feature-gated in the future,
but rig-core 0.42.0 does not offer per-provider feature flags in rig-core itself.

## 6. Risks and recommendations

| Risk | Mitigation |
| --- | --- |
| Rig 0.42.0 is pre-1.0; breaking changes likely | Adapter boundary (provider-rig) isolates all Rig types |
| 60 MB pre-strip is large | Pin minimal features; profile with `cargo bloat` later |
| No per-provider feature flags in rig-core | Accept full compile; strip unused monomorphized code via LTO |
| ICU/unicode data is large | Consider `icu4x` feature optimization in future Rig versions |

## 7. Remaining evidence

- Construct and call a real Rig provider through the production factory.
- Build the packaged APK with that production path linked.
- Measure the actual APK delta, startup cost and one Android TLS request.
- Record cancellation and normalized-error behavior.

The present evidence proves dependency compilation only. It does not complete
`PRV-003`, `PRV-004`, or any live provider task.
