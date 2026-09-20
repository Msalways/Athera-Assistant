import tempfile
import unittest
from pathlib import Path

from evals.verify_results import validate_result, verify


class ReportTests(unittest.TestCase):
    def test_blocked_requires_reason(self):
        self.assertIn("blocked result needs blocked_reason", validate_result({"schema_version": "v1", "outcome": "blocked", "profile": "x", "redaction_version": "v1"}))

    def test_verify_rejects_secret_pattern(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.jsonl"
            path.write_text('{"schema_version":"v1","outcome":"passed","profile":"x","redaction_version":"v1","trace":"sk-abcdefghijk"}\n', encoding="utf-8")
            self.assertTrue(verify(Path(directory)))

    def test_verify_accepts_a_redacted_blocked_report(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.jsonl"
            path.write_text('{"schema_version":"v1","outcome":"blocked","blocked_reason":"missing","profile":"x","redaction_version":"v1"}\n', encoding="utf-8")
            self.assertFalse(verify(Path(directory)))


if __name__ == "__main__":
    unittest.main()
