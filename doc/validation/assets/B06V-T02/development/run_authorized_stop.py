"""Run three actual ordinary CLI zero-request stops on an explicit immutable binary."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def pin(path):
    raw = Path(path).read_bytes()
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def write(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("repository", type=Path)
parser.add_argument("binary", type=Path)
parser.add_argument("new_root", type=Path)
args = parser.parse_args()
repository = args.repository.resolve()
binary = args.binary.resolve()
root = args.new_root.resolve()
root.mkdir()
fixture = repository / "fixtures/scan-formula"
config = fixture / "api-profiles/authorized-stop.json"
profile = fixture / "profiles/native.json"
frozen = json.loads((fixture / "frozen-sha256.json").read_bytes())
files = frozen["files"]


def source_pins():
    return {name: pin(fixture / name) for name in files}


before = source_pins()
for name, expected in files.items():
    assert before[name] == expected, ("frozen source changed", name)
selected_before = {"binary": pin(binary), "config": pin(config), "profile": pin(profile)}
assert binary.stat().st_nlink == 1, "selected binary must have a single ordinary link"
sentinel = root / "human-sentinel.txt"
sentinel.write_bytes(b"selected test parent human sentinel; preserve exactly\n")
sentinel_before = pin(sentinel)
environment = dict(os.environ)
environment.pop("NOETOME_TANGMAO_API_KEY", None)
environment.pop("NOETOME_TEST_SCAN_CREDENTIAL", None)
records = []
for state in ("clear", "unreadable_sign", "unreadable_subscript"):
    output = root / state
    case = fixture / "cases" / state
    argv = [str(binary), "knowledge", "scan-formula", str(case), str(config), str(profile), str(output)]
    started = time.monotonic()
    result = subprocess.run(argv, env=environment, capture_output=True, timeout=130)
    (root / (state + ".stdout.bin")).write_bytes(result.stdout)
    (root / (state + ".stderr.bin")).write_bytes(result.stderr)
    assert result.returncode == 3, (state, result.returncode)
    report = json.loads((output / "scan.json").read_bytes())
    assert json.loads(result.stdout) == report
    assert report["status"] == "needs_review" and report["preview_status"] == "blocked"
    assert report["knowledge_status"] == "pending" and report["scientific_quality"] == "not_run"
    assert report["published"] is False
    assert report["api"]["usage"]["network_attempts"] == 0
    assert report["api"]["usage"]["real_model_calls"] == 0
    assert report["paid_calls"] == 0
    assert report["recognition"] is None
    assert "SCAN_BUDGET_STOP" in report["decision"]["issue_codes"]
    for forbidden in ("plan.json", "compiled.tex", "扫描公式预览.md", "response.bin", "recognition.json"):
        assert not (output / forbidden).exists(), (state, forbidden)
    body = json.loads((output / "request-body.json").read_bytes())
    assert len(body["messages"]) == 2
    assert body["messages"][0]["role"] == "system"
    user = body["messages"][1]
    assert user["role"] == "user" and len(user["content"]) == 1
    image = user["content"][0]
    assert image["type"] == "image_url" and image["image_url"]["detail"] == "original"
    encoded = image["image_url"]["url"].removeprefix("data:image/png;base64,")
    assert base64.b64decode(encoded, validate=True) == (case / "region.png").read_bytes()
    semantics = json.loads((case / "semantics.json").read_bytes())
    if semantics["expression"] is not None:
        assert semantics["expression"] not in body["messages"][0]["content"]
    for copied, source in (("original.png", "region.png"), ("page.png", "page.png"),
                           ("original.pdf", "page.pdf"), ("source-case.json", "case.json"),
                           ("source-semantics.json", "semantics.json")):
        assert pin(output / copied) == pin(case / source), (state, copied)
    assert pin(sentinel) == sentinel_before
    records.append({"state": state, "argv": argv, "exit_code": result.returncode,
                    "elapsed_seconds": time.monotonic() - started, "output": str(output),
                    "stdout": pin(root / (state + ".stdout.bin")),
                    "stderr": pin(root / (state + ".stderr.bin")),
                    "report": pin(output / "scan.json"), "request_body": pin(output / "request-body.json"),
                    "original_crop": pin(output / "original.png"), "issue_codes": report["decision"]["issue_codes"]})
after = source_pins()
assert before == after
assert selected_before == {"binary": pin(binary), "config": pin(config), "profile": pin(profile)}
write(root / "receipt.json", {"schema_version": 1, "status": "passed", "purpose": "actual_authorized_zero_request_stops",
      "commands": records, "command_count": 3, "network_attempts": 0, "real_model_calls": 0,
      "paid_calls": 0, "credentials_read": False, "cumulative_user_ledger_read": False,
      "cumulative_user_balance": None, "original_source_file_count": len(files),
      "original_source_before": before, "original_source_after": after,
      "selected_before_after": selected_before, "sentinel_before_after": sentinel_before,
      "recognition_acceptance": "not_run", "scientific_quality": "not_run"})
print(json.dumps({"status": "passed", "commands": 3, "network_attempts": 0, "source_files": len(files)}))
