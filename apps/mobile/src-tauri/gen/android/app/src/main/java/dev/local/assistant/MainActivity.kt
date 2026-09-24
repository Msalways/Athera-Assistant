package dev.local.assistant

import android.content.Intent
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onWebViewCreate(webView: WebView) {
    val appVersion = packageManager.getPackageInfo(packageName, 0).longVersionCode
    val preferences = getSharedPreferences("athera.webview", MODE_PRIVATE)

    if (preferences.getLong("assetVersion", -1) != appVersion) {
      // Preserve Athera data and Android Keystore secrets while dropping only stale WebView assets.
      webView.clearCache(true)
      preferences.edit().putLong("assetVersion", appVersion).apply()
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    AuthPlugin.captureCallback(intent?.dataString)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    AuthPlugin.captureCallback(intent.dataString)
  }
}
