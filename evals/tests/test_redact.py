import unittest

from evals.redact import REDACTED, redact, secret_violations


class RedactionTests(unittest.TestCase):
    def test_redacts_nested_credentials_and_identifiers(self):
        result = redact({"authorization": "Bearer abcdefghijkl", "text": "mail me at a@example.com or +1 202 555 0199"})
        self.assertEqual(result["authorization"], REDACTED)
        self.assertEqual(result["text"], f"mail me at {REDACTED} or {REDACTED}")

    def test_redacts_query_secrets_without_dropping_the_url(self):
        self.assertEqual(redact("https://example.test/a?token=abc&safe=yes"), "https://example.test/a?token=%5BREDACTED%5D&safe=yes")

    def test_finds_unredacted_configured_secret(self):
        self.assertIn("configured_secret", secret_violations({"trace": "value"}, ("value",)))

    def test_finds_unredacted_personal_identifier(self):
        self.assertIn("personal_identifier", secret_violations("real.person@example.com"))


if __name__ == "__main__":
    unittest.main()
