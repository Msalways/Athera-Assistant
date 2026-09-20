package dev.local.assistant

/** Pure verification shared by the accessibility send boundary and JVM regression tests. */
object SmsPolicy {
    fun verify(recipient: String, message: String, recipients: List<String>, editors: List<String>) {
        check(recipients.size == 1) { "Cannot verify exactly one recipient" }
        check(recipients.single().replace(Regex("[ ()-]"), "") == recipient) { "Recipient changed or cannot be verified" }
        check(editors.size == 1 && editors.single() == message) { "Message changed or cannot be verified" }
    }
}
