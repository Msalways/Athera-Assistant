package dev.local.assistant

import android.app.Activity
import android.content.Intent
import android.provider.Settings
import android.provider.Telephony
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject

@InvokeArg
class SmsArgs {
    lateinit var operation: String
    var epoch: Long = 0
    var arguments: String = "{}"
}

/** Only Rust calls this bridge. No native plugin command is exposed in frontend ACLs. */
@TauriPlugin
class SmsPlugin(private val activity: Activity) : Plugin(activity) {
    private var epoch = 0L
    private var armed = false
    @Command
    fun execute(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SmsArgs::class.java)
            val service = SmsAccessibilityService.instance
            if (args.operation !in listOf("status", "settings")) {
                check(args.epoch >= epoch) { "Cancelled operation" }
                if (args.operation == "stop" || args.operation == "arm") {
                    epoch = args.epoch
                    armed = args.operation == "arm"
                } else check(armed && args.epoch == epoch) { "Session was not authorized" }
            }
            val result = when (args.operation) {
                "arm" -> { check(service != null && service.stopped) { "Accessibility unavailable or session active" }; JSONObject() }
                "status" -> JSONObject().put("enabled", service != null)
                    .put("build", "needle-sms-0.1.0")
                    .put("android_api", android.os.Build.VERSION.SDK_INT)
                    .put("model", android.os.Build.MODEL)
                    .put("abi", android.os.Build.SUPPORTED_ABIS.firstOrNull())
                    .put("package", Telephony.Sms.getDefaultSmsPackage(activity))
                    .put("stopped", service?.stopped ?: true)
                    .put("reason", service?.reason ?: "Enable Needle SMS accessibility manually to run a session.")
                "settings" -> {
                    service?.stop("Accessibility settings opened.")
                    activity.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
                    JSONObject()
                }
                "stop" -> { if (service?.stopped == false) service.stop("Session stopped."); JSONObject() }
                else -> (service ?: error("Accessibility is disabled"))
                    .execute(args.operation, JSONObject(args.arguments))
            }
            if (args.operation == "open_composer") {
                val handler = android.os.Handler(android.os.Looper.getMainLooper())
                val expires = android.os.SystemClock.elapsedRealtime() + 5000
                fun waitForComposer() {
                    if (service == null || service.stopped || !armed || args.epoch != epoch) { invoke.reject("Session cancelled"); return }
                    if (service.composerReady()) { invoke.resolve(JSObject(result.toString())); return }
                    if (android.os.SystemClock.elapsedRealtime() >= expires) { service.stop("SMS app did not open."); invoke.reject("SMS app did not open"); return }
                    handler.postDelayed({ waitForComposer() }, 100)
                }
                waitForComposer()
                return
            }
            invoke.resolve(JSObject(result.toString()))
        } catch (error: Exception) {
            SmsAccessibilityService.instance?.stop("Stopped: ${error.message}")
            invoke.reject(error.message ?: "SMS operation denied")
        }
    }
}
