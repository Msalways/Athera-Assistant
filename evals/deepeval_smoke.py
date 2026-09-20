"""Offline DeepEval smoke that evaluates output from the real Rust runner."""
from __future__ import annotations

import asyncio
import json
import os
import subprocess
from pathlib import Path

from deepeval import assert_test
from deepeval.metrics import BaseMetric
from deepeval.test_case import LLMTestCase

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "evals" / "fixtures" / "deepeval-smoke.jsonl"
RUNNER = REPO / "target" / "debug" / (
    "assistant-cli.exe" if os.name == "nt" else "assistant-cli"
)


class RustRunnerContractMetric(BaseMetric):
    """Exact, offline metric for the VS00 runner boundary."""

    threshold = 1.0
    async_mode = False
    strict_mode = True
    include_reason = True

    @property
    def __name__(self) -> str:
        return "Rust runner contract"

    def measure(self, test_case: LLMTestCase, *args, **kwargs) -> float:
        self.score = 1.0 if test_case.actual_output == test_case.expected_output else 0.0
        self.reason = (
            "The real Rust runner completed the scripted task and selected fixture.echo exactly once."
            if self.score == 1.0
            else "The Rust runner result or selected tool differed from the locked fixture."
        )
        self.success = self.score >= self.threshold
        return self.score

    async def a_measure(self, test_case: LLMTestCase, *args, **kwargs) -> float:
        return await asyncio.to_thread(self.measure, test_case, *args, **kwargs)


def test_real_rust_runner_with_deepeval() -> None:
    assert RUNNER.is_file(), "Build assistant-cli before running the DeepEval smoke."
    fixture = FIXTURE.read_text(encoding="utf-8").strip() + "\n"
    completed = subprocess.run(
        [str(RUNNER)],
        input=fixture,
        text=True,
        capture_output=True,
        check=False,
        cwd=REPO,
    )
    assert completed.returncode == 0, completed.stderr
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    assert len(lines) == 1
    result = json.loads(lines[0])
    calls = [json.loads(value) for value in result["task"]["call_counts"]]
    contract = {
        "result": result.get("result"),
        "status": result.get("task", {}).get("status"),
        "tools": [
            {"name": call["tool_id"], "arguments": call["arguments"]}
            for call in calls
        ],
    }
    expected = {
        "result": "pass",
        "status": "completed",
        "tools": [{"name": "fixture.echo", "arguments": {"text": "setup"}}],
    }
    assert_test(
        LLMTestCase(
            input="Echo the deterministic setup value.",
            actual_output=json.dumps(contract, sort_keys=True),
            expected_output=json.dumps(expected, sort_keys=True),
        ),
        [RustRunnerContractMetric()],
        run_async=False,
    )
