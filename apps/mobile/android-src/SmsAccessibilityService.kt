package dev.local.assistant

import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.graphics.PixelFormat
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Telephony
import android.view.Gravity
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Button
import org.json.JSONArray
import org.json.JSONObject

/** Fail-closed adapter: scoped to a user-started session in the default SMS app. */
class SmsAccessibilityService : AccessibilityService() {
    companion object { var instance: SmsAccessibilityService? = null; private set }
    var stopped = true; private set
    var reason = "No session is active."; private set
    private var selectedPackage = ""
    private var recipient = ""
    private var message = ""
    private var deadline = 0L
    private var enteredApp = false
    private var sendAttempted = false
    private var revision = 0
    private var fingerprint = ""
    private var steps = 0
    private var overlay: Button? = null
    private val handler = Handler(Looper.getMainLooper())
    private val expiry = Runnable { stop("Session time budget exceeded.") }

    override fun onServiceConnected() { instance = this }
    fun composerReady(): Boolean = !stopped && rootInActiveWindow?.packageName?.toString() == selectedPackage
    override fun onInterrupt() { stop("Accessibility interrupted.") }
    override fun onUnbind(intent: Intent?): Boolean { stop("Accessibility disabled."); instance = null; return super.onUnbind(intent) }
    override fun onDestroy() { stop("Accessibility disabled."); instance = null; super.onDestroy() }
    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        if (stopped) return
        if (SystemClock.elapsedRealtime() >= deadline) { stop("Session time budget exceeded."); return }
        if (event?.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) {
            val root = rootInActiveWindow ?: return
            val pkg = root.packageName?.toString()
            if (pkg == selectedPackage) enteredApp = true
            else if (enteredApp) stop("The foreground app changed.")
        }
    }
    fun stop(detail: String) {
        stopped = true
        reason = if (sendAttempted) "Send may have occurred. Check the SMS app; no automatic retry." else detail
        handler.removeCallbacks(expiry)
        overlay?.let { (getSystemService(WINDOW_SERVICE) as WindowManager).removeView(it) }
        overlay = null
        fingerprint = ""
    }
    private fun begin(args: JSONObject): JSONObject {
        check(stopped) { "A session is already active" }
        recipient = args.getString("recipient")
        message = args.getString("message")
        require(Regex("\\+?[0-9]{7,15}").matches(recipient) && message.isNotBlank() && message.length <= 1600)
        selectedPackage = Telephony.Sms.getDefaultSmsPackage(this) ?: error("No default SMS app")
        check(selectedPackage != packageName)
        stopped = false; sendAttempted = false; enteredApp = false; steps = 0
        reason = "SMS session active. Use Stop to cancel."
        deadline = SystemClock.elapsedRealtime() + 90000
        handler.postDelayed(expiry, 90000)
        val button = Button(this).apply {
            text = "Stop Needle SMS"
            contentDescription = "Stop Needle SMS session"
            setOnClickListener { stop("Stopped by user.") }
        }
        val params = WindowManager.LayoutParams(WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT, WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE, PixelFormat.TRANSLUCENT)
        params.gravity = Gravity.TOP or Gravity.END
        (getSystemService(WINDOW_SERVICE) as WindowManager).addView(button, params)
        overlay = button
        startActivity(Intent(Intent.ACTION_SENDTO, Uri.parse("smsto:${Uri.encode(recipient)}"))
            .setPackage(selectedPackage).putExtra("sms_body", message).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return JSONObject().put("opened", true)
    }
    private fun guard() {
        check(!stopped && !sendAttempted && SystemClock.elapsedRealtime() < deadline) { "Session is stopped or expired" }
        check(Telephony.Sms.getDefaultSmsPackage(this) == selectedPackage) { "Default SMS app changed" }
    }
    private data class Screen(val json: JSONObject, val nodes: List<AccessibilityNodeInfo>)
    private fun screen(): Screen {
        guard()
        val root = rootInActiveWindow ?: error("No accessible SMS window")
        check(root.packageName?.toString() == selectedPackage) { "SMS app is not in the foreground" }
        enteredApp = true
        val nodes = mutableListOf<AccessibilityNodeInfo>()
        // Traverse a bounded complete tree; refusing truncation prevents hidden extra recipients.
        var visited = 0
        fun walk(node: AccessibilityNodeInfo, depth: Int) {
            check(depth <= 24 && ++visited <= 256) { "SMS accessibility tree exceeds budget" }
            if (node.isVisibleToUser) nodes.add(node)
            for (i in 0 until node.childCount) node.getChild(i)?.let { walk(it, depth + 1) }
        }
        walk(root, 0)
        val useful = nodes.filter { kind(it) != "other" || !it.text.isNullOrBlank() || !it.contentDescription.isNullOrBlank() }
        check(useful.size <= 48) { "Accessible screen exceeds context budget" }
        val elements = JSONArray()
        useful.forEachIndexed { index, node ->
            elements.put(JSONObject().put("id", index).put("kind", kind(node))
                .put("text", node.text?.toString()?.take(240) ?: "")
                .put("description", node.contentDescription?.toString()?.take(160) ?: "")
                .put("resource", node.viewIdResourceName ?: "").put("enabled", node.isEnabled))
        }
        // Full text contributes to freshness even when its context excerpt is clipped.
        val current = useful.joinToString("\u0000") { "${it.viewIdResourceName}|${it.text}|${it.contentDescription}|${it.isEnabled}|${it.isEditable}|${it.isClickable}" }
        if (current != fingerprint) { fingerprint = current; revision++ }
        return Screen(JSONObject().put("revision", revision).put("package", selectedPackage).put("elements", elements), useful)
    }
    /** Resource IDs are an explicit compatibility allowlist; visible labels alone never grant authority. */
    private fun kind(node: AccessibilityNodeInfo): String {
        val resource = node.viewIdResourceName ?: return "other"
        if (!resource.startsWith("$selectedPackage:id/")) return "other"
        return when (resource.substringAfter(":id/")) {
            "compose_message_text", "embedded_text_editor", "message_compose_text" -> if (node.isEditable) "message" else "other"
            "send_message_button", "send_message", "send_button_sms" -> if (node.isClickable) "send" else "other"
            "recipient_text_view", "recipient_text", "recipients_editor" -> "recipient"
            else -> "other"
        }
    }
    private fun verify(screen: Screen) {
        val recipients = screen.nodes.filter { kind(it) == "recipient" }
        check(screen.nodes.none { (it.viewIdResourceName ?: "").contains("recipient", ignoreCase = true) && kind(it) == "other" && (!it.text.isNullOrBlank() || !it.contentDescription.isNullOrBlank()) }) { "Unrecognized recipient information" }
        // A lone recognized recipient field must expose the entire number, not a contact name.
        val editors = screen.nodes.filter { kind(it) == "message" }
        SmsPolicy.verify(recipient, message, recipients.map { it.text?.toString() ?: "" }, editors.map { it.text?.toString() ?: "" })
    }
    fun execute(operation: String, args: JSONObject): JSONObject {
        check(Looper.myLooper() == Looper.getMainLooper())
        if (operation == "open_composer") return begin(args)
        guard()
        check(++steps <= 24) { "Native step budget exceeded" }
        val screen = screen()
        if (operation == "inspect_screen") return screen.json
        check(args.getInt("revision") == revision) { "Stale screen revision" }
        val node = screen.nodes.getOrNull(args.getInt("target")) ?: error("Stale target")
        check(node.refresh() && node.isVisibleToUser && node.isEnabled) { "Target unavailable" }
        when (operation) {
            "enter_text" -> {
                check(kind(node) == "message" && args.getString("text") == message) { "Only approved message entry is allowed" }
                val bundle = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, message) }
                check(node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, bundle)) { "Text entry denied" }
            }
            "select_element" -> {
                when (kind(node)) {
                    "message" -> check(node.performAction(AccessibilityNodeInfo.ACTION_FOCUS)) { "Focus denied" }
                    "send" -> {
                        // Re-read immediately before ACTION_CLICK, independently of the model's observation.
                        val fresh = screen()
                        check(args.getInt("revision") == revision) { "Screen changed before sending" }
                        verify(fresh)
                        guard()
                        sendAttempted = true
                        getSharedPreferences("sms-experiment", MODE_PRIVATE).edit().putBoolean("uncertain_send", true).commit().also { check(it) { "Cannot persist send boundary" } }
                        // Never retry even if Android reports false or delivery cannot be observed.
                        node.performAction(AccessibilityNodeInfo.ACTION_CLICK)
                        stop("Send attempted.")
                        return JSONObject().put("outcome", "send_attempted")
                    }
                    else -> error("Selecting this element is not allowed")
                }
            }
            else -> error("Unknown operation")
        }
        return JSONObject().put("acted", true)
    }
}
