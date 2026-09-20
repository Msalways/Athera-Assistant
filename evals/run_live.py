"""Run configured live profiles through the real Rust JSONL runner.

This module deliberately does not plan, route, or execute tools in Python.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from evals.redact import REDACTION_VERSION, configured_secrets, redact
from evals.verify_results import verify

DEFAULT_RUNNER = ("cargo", "run", "--quiet", "-p", "assistant-cli", "--bin", "assistant-cli")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def preflight(dataset: Path, profile: str, runner: tuple[str, ...]) -> list[str]:
    blockers: list[str] = []
    if not dataset.is_dir():
        blockers.append("dataset_missing")
    elif not any(dataset.rglob("*.jsonl")):
        blockers.append("dataset_empty")
    if not shutil.which(runner[0]):
        blockers.append("rust_runner_unavailable")
    if profile != "fixture" and not os.environ.get(f"ASSISTANT_EVAL_PROFILE_{profile.upper()}"):
        blockers.append(f"profile_not_configured:{profile}")
    if importlib.util.find_spec("deepeval") is None:
        blockers.append("deepeval_not_installed")
    # The present Rust runner is deterministic-only. Do not pretend a fixture is a live model run.
    blockers.append("rust_runner_live_profile_not_implemented")
    return blockers


def blocked_report(profile: str, blockers: list[str]) -> dict[str, Any]:
    return {
        "schema_version": "v1",
        "case_id": None,
        "run_id": str(uuid.uuid4()),
        "outcome": "blocked",
        "blocked_reason": ",".join(blockers),
        "profile": profile,
        "redaction_version": REDACTION_VERSION,
        "trace": None,
        "runtime": {"runner": "assistant-cli", "started_at": utc_now()},
    }


def write_report(report: dict[str, Any], output: Path | None) -> None:
    line = json.dumps(redact(report, configured_secrets()), sort_keys=True)
    if output is None:
        print(line)
        return
    output.mkdir(parents=True, exist_ok=True)
    (output / "report.jsonl").write_text(line + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Run a live AETHRA profile through the Rust JSONL runner.")
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--runner", nargs="+", default=list(DEFAULT_RUNNER))
    args = parser.parse_args()
    runner = tuple(args.runner)
    blockers = preflight(args.dataset, args.profile, runner)
    if blockers:
        write_report(blocked_report(args.profile, blockers), args.output)
        return 0 if args.dry_run else 2
    # Kept after preflight so a dry run cannot send a prompt or contact a provider.
    cases = "\n".join(path.read_text(encoding="utf-8").strip() for path in sorted(args.dataset.rglob("*.jsonl")) if path.read_text(encoding="utf-8").strip()) + "\n"
    completed = subprocess.run(runner, input=cases, text=True, capture_output=True, check=False)
    if completed.returncode:
        print(completed.stderr, file=sys.stderr)
        return completed.returncode
    if args.output is None:
        print(completed.stdout, end="")
        return 0
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "report.jsonl").write_text(completed.stdout, encoding="utf-8")
    errors = verify(args.output, configured_secrets())
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
