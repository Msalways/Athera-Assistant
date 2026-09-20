package dev.local.assistant

import org.junit.Test
import org.junit.Assert.assertThrows

class SmsPolicyTest {
    @Test fun exactSingleRecipientAndMessagePass() {
        SmsPolicy.verify("+15551234567", "Hi", listOf("+1 (555) 123-4567"), listOf("Hi"))
    }
    @Test fun changedMissingMultipleOrNamedRecipientsFail() {
        for (recipients in listOf(emptyList(), listOf("Alice"), listOf("+15557654321"), listOf("+15551234567", "+15557654321"))) {
            assertThrows(IllegalStateException::class.java) { SmsPolicy.verify("+15551234567", "Hi", recipients, listOf("Hi")) }
        }
    }
    @Test fun changedOrAmbiguousMessageFails() {
        for (editors in listOf(emptyList(), listOf("Changed"), listOf("Hi", "Hi"))) {
            assertThrows(IllegalStateException::class.java) { SmsPolicy.verify("+15551234567", "Hi", listOf("+15551234567"), editors) }
        }
    }
}
