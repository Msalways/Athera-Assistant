# Needle-only Android SMS experiment

This personal Android 12+ ARM64 build uses the pinned Needle 2 static archive and embedded model. The Android dependency graph excludes `app-runtime`, `provider-local-chat`, `provider-cloud`, and `adapter-mcp`; the existing desktop runtime is preserved. There is no Internet permission, cloud fallback, Python runtime, or direct SMS API in the APK.

## Install and test

1. Transfer `needle-sms-test.apk` to the phone and open it. Allow installation from the file manager when Android asks. This APK uses a local development signing key.
2. Open **Needle SMS Experiment**. Set a default SMS app first if one is missing.
3. Enter a recipient number you control, including country code, and an instruction. Tap **Ask Needle to draft**. Repeat in airplane mode to check offline inference.
4. Review the actual output and measured inference time. An invalid result is a failed experiment; unchanged wording is not successful rewriting.
5. For app interaction, manually enable **Needle SMS experiment** in Android Accessibility settings. Android may first require **App info → menu → Allow restricted settings** for a sideloaded accessibility service.
6. Approve the displayed exact number and wording. The SMS app opens with a **Stop Needle SMS** overlay. A session lasts at most 90 seconds and eight model steps.
7. Stop cancels pending actions. Returning to another app, disabling accessibility, stale screen targets, unknown controls, or unverifiable recipients/messages stops execution.
8. After any possible send, inspect the SMS app yourself before starting another session. The experiment cannot confirm delivery and never automatically retries sending.

The service recognizes a small set of SMS composer resource IDs. Contact names, multiple recipients, inaccessible fields, unfamiliar app versions, long conversations, or large accessibility trees may cause a safe refusal. Existing conversation text is visible only when exposed by the SMS app and within the bounded screen context. This is not general app automation.

## Share results without ADB

Expand **Test report to share**, tap its text box, then copy the selected text into your reply. It includes device/build information, accessibility status, execution state, inference latency, and errors. Recipient, instruction, draft, and accessible screen content are excluded. Separately report whether wording improved, whether airplane-mode drafting worked, and what you observed in the SMS app.

If the app crashes before showing a report, Android **Developer options → Take bug report** can capture crash details. Share the relevant crash excerpt, not an entire device report. If you later use ADB, `adb logcat -b crash -d` collects the crash buffer. Detailed memory and native startup diagnostics require device tools.

## Implementation and build

- `assistant_core::sms::Experiment` uses vendor-neutral `ModelProvider`, `ToolExecutor`, `ToolSpec`, and `Store` contracts. `DraftResult` is a strictly validated `{message: string}` proposal.
- `PolicyEngine` binds approval to the proposal ID and exact recipient/message. Only a recognized message editor or send control can be selected. Kotlin independently rechecks the foreground SMS package, session budget, screen revision, single numeric recipient, and exact message immediately before clicking Send.
- Rust stores bounded tool results separately from inference context. The model receives three scoped screen tools and one recent observation. Drafting receives only its draft-result capability plus Needle's control capabilities.
- Stop invalidates Rust proposals and the Android bridge session counter. Late native inference cannot execute tools. Native inference itself cannot be forcibly interrupted.
- Send uncertainty is persisted before attempting the external write and survives restart. There is no replay or automatic retry.
- Kotlin source: `apps/mobile/android-src`; JVM safety tests: `apps/mobile/android-tests`. Generated Android project customizations are committed as source and must be preserved if reinitializing Tauri.
- Run `powershell -ExecutionPolicy Bypass -File scripts/build-sms-apk.ps1` after installing the workspace prerequisites. `scripts/android-env.ps1` defines SDK, NDK, Needle, Gradle cache, and development key paths; set `JAVA_HOME` if Java is elsewhere.
- Needle 2 revision: `32e9e3a93b205f786929697446ae669cf0a84579`. Android static archive SHA-256: `93738ae3a9488cbc3104eb65bf49093683c26eb01b0d257e980e499d1d06a9f4`. The Rust build verifies this hash before linking. Model weights are embedded in the archive.
- Platform: API 36 compile/target, minimum API 31, ARM64 only. NDK r27c (`27.2.12479018`). Native C++ runtime is bundled. Runtime/model licenses are included in frontend assets.

## Host evidence and remaining limits

The real Windows Needle run is recorded in `artifacts/needle-sms-host.jsonl`: exact tool/argument matching passed **1/4** cases. Draft rewriting returned the original wording; ambiguity produced an invalid response. Measured host latency was 1.409–3.178 seconds. These are synthetic host instructions; no SMS was executed. Drafting quality is separate from tool-selection accuracy.

Rust regression coverage includes malformed outputs, unchanged wording, exact approvals, stale targets, permission denial, Stop, and uncertain-send recovery. Kotlin tests cover exact versus changed, missing, named, or multiple recipients/messages. Frontend tests cover approval payloads and Stop.

Physical-device startup, airplane-mode inference, SMS-app compatibility, latency, memory use, actual sending, and delivery remain pending user testing. This build does not claim reliable message rewriting or multistep control.

Implementation references: [Tauri mobile plugins](https://v2.tauri.app/develop/plugins/develop-mobile/), [Android AccessibilityService](https://developer.android.com/reference/android/accessibilityservice/AccessibilityService), [pinned Needle distribution](https://huggingface.co/Cactus-Compute/needle2/tree/32e9e3a93b205f786929697446ae669cf0a84579).
