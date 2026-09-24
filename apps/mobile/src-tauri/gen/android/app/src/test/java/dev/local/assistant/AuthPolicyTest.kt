package dev.local.assistant

import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class AuthPolicyTest {
    @Test fun acceptsOnlyExactHttpsCallbackBase() {
        val redirect = "https://assistant.example/oauth/callback"
        assertTrue(AuthPolicy.isCallback("$redirect?code=code&state=state", redirect))
        assertFalse(AuthPolicy.isCallback("https://evil.example/oauth/callback?code=code", redirect))
        assertFalse(AuthPolicy.isCallback("https://assistant.example/other?code=code", redirect))
        assertFalse(AuthPolicy.isCallback("$redirect?code=code#fragment", redirect))
    }

    @Test fun rejectsNonHttpsAuthorizationUrls() {
        assertThrows(IllegalArgumentException::class.java) {
            AuthPolicy.requireHttps("http://assistant.example/authorize")
        }
    }
}
