import json
import tempfile
import unittest
from pathlib import Path

from evals.lint_datasets import lint


class DatasetLintTests(unittest.TestCase):
    def test_accepts_repository_datasets(self) -> None:
        self.assertEqual(lint(), [])

    def test_rejects_duplicate_ids_and_secrets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "schemas").mkdir()
            (root / "datasets").mkdir()
            (root / "fixtures").mkdir()
            (root / "schemas" / "case.json").write_text(
                json.dumps({"$schema": "https://json-schema.org/draft/2020-12/schema"}),
                encoding="utf-8",
            )
            case = {
                "case_id": "duplicate",
                "profile": "fixture",
                "verification": "exact",
                "input": "Bearer abcdefghijk",
            }
            (root / "datasets" / "bad.jsonl").write_text(
                json.dumps(case) + "\n" + json.dumps(case) + "\n",
                encoding="utf-8",
            )
            errors = lint(root)
            self.assertTrue(any("duplicate case_id" in error for error in errors))
            self.assertTrue(any("credential_pattern" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
