package dev.local.assistant

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.browser.customtabs.CustomTabsIntent
import androidx.core.net.toUri
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.net.URI
import java.security.KeyStore
import java.security.MessageDigest
import java.security.GeneralSecurityException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONObject

@InvokeArg
class AuthArgs {
    lateinit var operation: String
    var arguments: String = "{}"
}

/** Native-only OAuth and secret boundary. React can receive status, never callback URLs or values. */
@TauriPlugin
class AuthPlugin(private val activity: Activity) : Plugin(activity) {
    companion object {
        @Volatile private var callbackCandidate: String? = null

        fun captureCallback(url: String?) {
            if (url != null) callbackCandidate = url
        }
    }

    private var expectedRedirect: String? = null
    private val preferences by lazy {
        activity.getSharedPreferences("aethra.native.secrets", Context.MODE_PRIVATE)
    }

    @Command
    fun execute(invoke: Invoke) {
        try {
            val request = invoke.parseArgs(AuthArgs::class.java)
            val arguments = JSONObject(request.arguments)
            val result = when (request.operation) {
                "open_external" -> {
                    val url = arguments.getString("url")
                    AuthPolicy.requireExternalUrl(url)
                    val intent = Intent(Intent.ACTION_VIEW, url.toUri())
                    check(intent.resolveActivity(activity.packageManager) != null) { "No application can open this action" }
                    activity.startActivity(intent)
                    JSONObject().put("opened", true)
                }
                "open" -> {
                    val authorizationUrl = arguments.getString("authorization_url")
                    val redirectUri = arguments.getString("redirect_uri")
                    AuthPolicy.requireHttps(authorizationUrl)
                    AuthPolicy.requireHttps(redirectUri)
                    expectedRedirect = redirectUri
                    callbackCandidate = null
                    CustomTabsIntent.Builder().build().launchUrl(activity, authorizationUrl.toUri())
                    JSONObject()
                }
                "take_callback" -> {
                    val callback = callbackCandidate
                    if (callback == null) {
                        JSONObject().put("callback_url", JSONObject.NULL)
                    } else {
                        check(AuthPolicy.isCallback(callback, expectedRedirect)) { "OAuth callback did not match the registered redirect" }
                        callbackCandidate = null
                        JSONObject().put("callback_url", callback)
                    }
                }
                "clear_callback" -> {
                    callbackCandidate = null
                    expectedRedirect = null
                    JSONObject()
                }
                "store_secret" -> {
                    storeSecret(arguments.getString("handle"), arguments.getString("binding"), arguments.getString("value"))
                    JSONObject()
                }
                "load_secret" -> {
                    val value = loadSecret(arguments.getString("handle"), arguments.getString("binding"))
                    JSONObject().put("value", value ?: JSONObject.NULL)
                }
                "delete_secret" -> {
                    deleteSecret(arguments.getString("handle"))
                    JSONObject()
                }
                else -> error("Unsupported native authorization operation")
            }
            invoke.resolve(JSObject(result.toString()))
        } catch (error: Exception) {
            invoke.reject(error.message ?: "Native authorization failed")
        }
    }

    private fun storeSecret(handle: String, binding: String, value: String) {
        require(handle.isNotBlank() && handle.length <= 256)
        require(binding.isNotBlank() && binding.length <= 16_384)
        require(value.isNotBlank() && value.length <= 65_536)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(handle))
        cipher.updateAAD(binding.toByteArray(Charsets.UTF_8))
        val encrypted = cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        check(preferences.edit().putString(storageKey(handle), Base64.encodeToString(encrypted, Base64.NO_WRAP)).commit())
    }

    private fun loadSecret(handle: String, binding: String): String? {
        require(handle.isNotBlank() && handle.length <= 256)
        require(binding.isNotBlank() && binding.length <= 16_384)
        val encoded = preferences.getString(storageKey(handle), null) ?: return null
        return try {
            val encrypted = Base64.decode(encoded, Base64.NO_WRAP)
            if (encrypted.size <= 12) {
                deleteSecret(handle)
                return null
            }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(handle), GCMParameterSpec(128, encrypted.copyOfRange(0, 12)))
            cipher.updateAAD(binding.toByteArray(Charsets.UTF_8))
            cipher.doFinal(encrypted.copyOfRange(12, encrypted.size)).toString(Charsets.UTF_8)
        } catch (_: GeneralSecurityException) {
            deleteSecret(handle)
            null
        } catch (_: IllegalArgumentException) {
            deleteSecret(handle)
            null
        }
    }

    private fun deleteSecret(handle: String) {
        require(handle.isNotBlank() && handle.length <= 256)
        preferences.edit().remove(storageKey(handle)).apply()
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (store.containsAlias(keyAlias(handle))) store.deleteEntry(keyAlias(handle))
    }

    private fun key(handle: String): SecretKey {
        val alias = keyAlias(handle)
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .build(),
            )
            generateKey()
        }
    }

    private fun storageKey(handle: String) = "secret.${keyAlias(handle)}"
    private fun keyAlias(handle: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(handle.toByteArray(Charsets.UTF_8))
        return "aethra.${Base64.encodeToString(digest, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)}"
    }
}

object AuthPolicy {
    fun requireExternalUrl(value: String) {
        val uri = URI(value)
        require(
            uri.userInfo == null && uri.fragment == null &&
                uri.scheme in setOf("https", "http", "mailto", "tel", "geo", "whatsapp")
        ) { "Unsupported external action URL" }
        if (uri.scheme == "http" || uri.scheme == "https") require(uri.host != null) { "External URL must have a host" }
    }

    fun requireHttps(value: String) {
        val uri = URI(value)
        require(uri.scheme == "https" && uri.host != null && uri.userInfo == null && uri.fragment == null) { "OAuth URLs must use HTTPS" }
    }

    fun isCallback(callback: String, expected: String?): Boolean {
        if (expected == null) return false
        val actualUri = runCatching { URI(callback) }.getOrNull() ?: return false
        val expectedUri = runCatching { URI(expected) }.getOrNull() ?: return false
        return actualUri.scheme == "https" &&
            actualUri.userInfo == null && actualUri.fragment == null &&
            actualUri.scheme == expectedUri.scheme &&
            actualUri.rawAuthority == expectedUri.rawAuthority &&
            actualUri.rawPath == expectedUri.rawPath
    }
}
