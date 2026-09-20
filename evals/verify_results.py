"""Validate redacted JSONL evaluation reports without importing product code."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Iterable

if __package__:
    from evals.redact import REDACTION_VERSION, configured_secrets, secret_violations
else:  # Supports the documented `python .\\evals\\verify_results.py` command.
    from redact import REDACTION_VERSION, configured_secrets, secret_violations

REQUIRED = {"schema_version", "outcome", "profile", "redaction_version"}
OUTCOMES = {"passed", "failed", "blocked", "needs_review"}


def validate_result(result: Any) -> list[str]:
    if not isinstance(result, dict):
        return ["result is not an object"]
    errors = [f"missing {name}" for name in sorted(REQUIRED - result.keys())]
    if result.get("schema_version") != "v1":
        errors.append("schema_version must be v1")
    if result.get("redaction_version") != REDACTION_VERSION:
        errors.append("redaction_version must be v1")
    if result.get("outcome") not in OUTCOMES:
        errors.append("invalid outcome")
    if result.get("outcome") == "blocked" and not result.get("blocked_reason"):
        errors.append("blocked result needs blocked_reason")
    return errors


def report_paths(path: Path) -> Iterable[Path]:
    if path.is_file():
        yield path
    elif path.exists():
        yield from path.rglob("*.jsonl")
        yield from path.rglob("*.json")


def verify(path: Path, secrets: Iterable[str] = ()) -> list[str]:
    errors: list[str] = []
    files = list(report_paths(path))
    if not files:
        return [f"no report files found under {path}"]
    for report in files:
        for line_number, line in enumerate(report.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                continue
            try:
                result = json.loads(line)
            except json.JSONDecodeError as error:
                errors.append(f"{report}:{line_number}: invalid JSON ({error.msg})")
                continue
            errors.extend(f"{report}:{line_number}: {error}" for error in validate_result(result))
            errors.extend(f"{report}:{line_number}: {finding}" for finding in secret_violations(result, secrets))
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate redacted AETHRA evaluation reports.")
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    errors = verify(args.path, configured_secrets())
    if errors:
        print("\n".join(errors))
        return 1
    print("evaluation reports verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
