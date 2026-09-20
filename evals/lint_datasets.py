"""Deterministic JSONL and schema checks for evaluation inputs."""
from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Iterable

from evals.redact import secret_violations

ROOT = Path(__file__).resolve().parent
CASE_REQUIRED = {"case_id", "profile", "verification", "input"}
VERIFICATIONS = {"exact", "judge", "both", "physical"}


def jsonl(path: Path) -> Iterable[tuple[int, Any]]:
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.strip():
            yield number, json.loads(line)


def lint(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    for schema in sorted((root / "schemas").glob("*.json")):
        try:
            value = json.loads(schema.read_text(encoding="utf-8"))
            if not isinstance(value, dict) or "$schema" not in value:
                errors.append(f"{schema}: schema must be an object with $schema")
        except json.JSONDecodeError as error:
            errors.append(f"{schema}:{error.lineno}: invalid JSON")

    seen: set[str] = set()
    for path in sorted((root / "datasets").rglob("*.jsonl")):
        try:
            records = list(jsonl(path))
        except json.JSONDecodeError as error:
            errors.append(f"{path}:{error.lineno}: invalid JSON")
            continue
        if not records:
            errors.append(f"{path}: dataset is empty")
        for number, case in records:
            where = f"{path}:{number}"
            if not isinstance(case, dict):
                errors.append(f"{where}: case must be an object")
                continue
            missing = CASE_REQUIRED - case.keys()
            if missing:
                errors.append(f"{where}: missing {','.join(sorted(missing))}")
            case_id = case.get("case_id")
            if not isinstance(case_id, str) or not case_id.strip():
                errors.append(f"{where}: case_id must be non-empty")
            elif case_id in seen:
                errors.append(f"{where}: duplicate case_id {case_id}")
            else:
                seen.add(case_id)
            if case.get("verification") not in VERIFICATIONS:
                errors.append(f"{where}: invalid verification")
            if not isinstance(case.get("input"), str):
                errors.append(f"{where}: input must be a string")
            for finding in secret_violations(case):
                errors.append(f"{where}: {finding}")

    for path in sorted((root / "fixtures").glob("*.jsonl")):
        try:
            records = list(jsonl(path))
        except json.JSONDecodeError as error:
            errors.append(f"{path}:{error.lineno}: invalid JSON")
            continue
        for number, fixture in records:
            where = f"{path}:{number}"
            if not isinstance(fixture, dict) or not isinstance(fixture.get("input"), str):
                errors.append(f"{where}: fixture needs a string input")
            for finding in secret_violations(fixture):
                errors.append(f"{where}: {finding}")
    return errors


def main() -> int:
    errors = lint()
    if errors:
        print("\n".join(errors))
        return 1
    print("evaluation datasets verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
