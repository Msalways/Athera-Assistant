"""Compare one fixture's normalized events through JSONL and the real HTTP bridge."""
from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any


def normalized_events(report: dict[str, Any]) -> list[dict[str, Any]]:
    workers: dict[str, str] = {}
    normalized: list[dict[str, Any]] = []
    for event in report.get("events", []):
        item = dict(event)
        item.pop("event_id", None)
        item["run_id"] = "run"
        item["task_id"] = "run"
        worker = item.get("worker_id")
        if worker:
            item["worker_id"] = workers.setdefault(
                worker, f"worker-{len(workers) + 1}"
            )
        normalized.append(item)
    return normalized


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def post_fixture(port: int, fixture: dict[str, Any]) -> dict[str, Any]:
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/api/evaluate-fixture",
        data=json.dumps(fixture).encode(),
        headers={"Content-Type": "application/json", "X-Assistant-Client": "local-ui"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.load(response)


def wait_ready(port: int) -> None:
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(
                f"http://127.0.0.1:{port}/api/health", timeout=1
            ) as response:
                if json.load(response).get("status") == "ready":
                    return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("assistant-dev did not become ready")


def verify(runner: Path, bridge: Path, fixture_path: Path) -> int:
    line = next(line for line in fixture_path.read_text(encoding="utf-8").splitlines() if line.strip())
    fixture = json.loads(line)
    cli = subprocess.run(
        [str(runner)],
        input=line + "\n",
        text=True,
        capture_output=True,
        check=True,
        timeout=30,
    )
    cli_report = json.loads(cli.stdout.strip())
    port = free_port()
    database = Path(tempfile.gettempdir()) / f"aethra-parity-{os.getpid()}.db"
    environment = os.environ.copy()
    environment.update(
        {
            "ASSISTANT_DEV_FIXTURE_MODE": "1",
            "ASSISTANT_DEV_PORT": str(port),
            "ASSISTANT_DEV_DATABASE": str(database),
        }
    )
    process = subprocess.Popen(
        [str(bridge)],
        env=environment,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        wait_ready(port)
        bridge_report = post_fixture(port, fixture)
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        database.unlink(missing_ok=True)
        Path(f"{database}-shm").unlink(missing_ok=True)
        Path(f"{database}-wal").unlink(missing_ok=True)
    if cli_report.get("result") != "pass" or bridge_report.get("result") != "pass":
        raise AssertionError("fixture did not pass through both transports")
    cli_events = normalized_events(cli_report)
    bridge_events = normalized_events(bridge_report)
    if cli_events != bridge_events:
        raise AssertionError(
            "normalized event mismatch:\n"
            + json.dumps({"cli": cli_events, "bridge": bridge_events}, indent=2)
        )
    print(f"browser bridge parity: {len(cli_events)} normalized events match")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("runner", type=Path)
    parser.add_argument("bridge", type=Path)
    parser.add_argument("fixture", type=Path)
    arguments = parser.parse_args()
    return verify(arguments.runner, arguments.bridge, arguments.fixture)


if __name__ == "__main__":
    raise SystemExit(main())
