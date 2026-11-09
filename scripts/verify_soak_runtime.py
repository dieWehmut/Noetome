#!/usr/bin/env python3
"""Capture three actual CLI soak runs and independently audit their files.

The Windows workflow uses this focused check after changes to the soak auditor.
It records its platform and scope; it never represents a full regression run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from datetime import datetime, timezone


PAYLOAD_NAMES = ("manifest.json", "report.md", "structure.json", "timings.json")
MEMORY_FIELDS = (
    "first_rss_bytes", "last_rss_bytes", "peak_rss_bytes", "rss_growth_bytes"
)
CASES = (("10-rounds", 10, None), ("300-rounds", 300, None), ("paused", 10, 0))


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def write_json(path: Path, value: object) -> None:
    path.write_bytes((json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def fingerprint(path: Path) -> dict:
    raw = path.read_bytes()
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def capture(command: list[str], directory: Path, prefix: str) -> dict:
    started = utc_now()
    error = None
    try:
        result = subprocess.run(
            command, stdin=subprocess.DEVNULL, capture_output=True, timeout=60,
            check=False,
        )
        code, stdout, stderr = result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired as failure:
        code, stdout, stderr = None, failure.stdout or b"", failure.stderr or b""
        error = "Command exceeded its 60 second limit."
    except OSError as failure:
        code, stdout, stderr = None, b"", b""
        error = str(failure)
    stdout_path = directory / f"{prefix}.stdout.log"
    stderr_path = directory / f"{prefix}.stderr.log"
    stdout_path.write_bytes(stdout)
    stderr_path.write_bytes(stderr)
    receipt = {
        "command": command, "started_at_utc": started,
        "completed_at_utc": utc_now(), "exit_code": code, "error": error,
        "stdout": stdout_path.name, "stderr": stderr_path.name,
    }
    write_json(directory / f"{prefix}.command.json", receipt)
    return receipt


def run_case(binary: Path, checker: Path, root: Path, name: str,
             rounds: int, pause: int | None) -> dict:
    directory = root / name
    directory.mkdir()
    output = directory / "output"
    command = [
        str(binary), "soak", "run", "--rounds", str(rounds),
        "--interval-ms", "0", "--max-rss-growth-bytes", "1073741824",
        "--output", str(output),
    ]
    if pause is not None:
        command.extend(("--pause-after-ms", str(pause)))
    cli = capture(command, directory, "cli")
    audit = capture(
        [sys.executable, "-B", "-X", "utf8", str(checker), str(output), "--json"],
        directory, "audit",
    )
    issues = []
    audit_result = None
    try:
        audit_result = json.loads((directory / "audit.stdout.log").read_bytes())
        (directory / "audit.json").write_bytes((directory / "audit.stdout.log").read_bytes())
    except (UnicodeError, ValueError) as failure:
        issues.append(f"Independent audit did not return JSON: {failure}")
    checks = audit_result.get("checks") if isinstance(audit_result, dict) else None
    total = audit_result.get("total") if isinstance(audit_result, dict) else None
    audit_failures = audit_result.get("failures") if isinstance(audit_result, dict) else None
    audit_passed = (
        type(total) is int and total > 0 and type(audit_failures) is int
        and audit_failures == 0 and isinstance(checks, list) and len(checks) == total
        and all(isinstance(check, dict) and check.get("ok") is True for check in checks)
    )
    payloads = {}
    for filename in PAYLOAD_NAMES:
        path = output / filename
        if path.is_file():
            payloads[filename] = fingerprint(path)
        else:
            issues.append(f"Actual CLI payload is missing: {filename}")
    memory = None
    try:
        timings = json.loads((output / "timings.json").read_bytes())
        memory = {field: timings["summary"][field] for field in MEMORY_FIELDS}
    except (OSError, ValueError, KeyError, TypeError) as failure:
        issues.append(f"Cannot read actual memory result: {failure}")
    passed = cli["exit_code"] == 0 and audit["exit_code"] == 0 and audit_passed and not issues
    receipt = {
        "case": name, "rounds": rounds, "pause_after_ms": pause,
        "result": "passed" if passed else "failed",
        "cli_exit_code": cli["exit_code"], "audit_exit_code": audit["exit_code"],
        "audit_checks": total, "audit_failures": audit_failures,
        "memory_readings": memory, "payloads": payloads, "issues": issues,
    }
    write_json(directory / "receipt.json", receipt)
    return receipt


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="New evidence directory; existing roots are refused.")
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args(argv)
    binary = args.binary.resolve()
    checker = Path(__file__).resolve().with_name("verify_soak_harness.py")
    if not binary.is_file():
        parser.error("--binary must identify an existing actual CLI executable")
    root = args.root.resolve()
    if root.exists():
        parser.error("The evidence directory already exists; choose a new root")
    root.mkdir(parents=True)
    started = utc_now()
    cases = [run_case(binary, checker, root, *case) for case in CASES]
    passed = all(case["result"] == "passed" for case in cases)
    summary = {
        "schema_version": 1, "scope": "targeted_soak",
        "started_at_utc": started, "completed_at_utc": utc_now(),
        "source_commit": os.environ.get("GITHUB_SHA"),
        "platform": platform.platform(), "python": platform.python_version(),
        "binary": {"path": str(binary), **fingerprint(binary)},
        "result": "passed" if passed else "failed", "actual_cli_runs": len(cases),
        "independent_audits": len(cases), "cases": cases,
    }
    write_json(root / "summary.json", summary)
    print(f"Targeted soak: {len(cases)} actual CLI runs and independent audits; {summary['result']}.")
    print(root / "summary.json")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
