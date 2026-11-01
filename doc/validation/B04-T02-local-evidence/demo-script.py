"""Explicit local-only CLI demonstration; all writes stay in a fresh test root."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import uuid


def sha(data):
    return hashlib.sha256(data).hexdigest()


def snapshot(root):
    return {p.relative_to(root).as_posix(): {"bytes": p.stat().st_size, "sha256": sha(p.read_bytes())}
            for p in sorted(root.rglob("*")) if p.is_file()}


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


parser = argparse.ArgumentParser()
parser.add_argument("--cli", type=pathlib.Path, required=True)
args = parser.parse_args()
repo = pathlib.Path.cwd()
root = repo / ".artifacts" / "B04-T02-local-demo" / str(uuid.uuid4())
root.mkdir(parents=True)
assert args.cli.is_absolute() and args.cli.is_file()
cli_calls = []


def call(case, name, command, expected):
    process = subprocess.run([str(args.cli), *map(str, command)], capture_output=True, timeout=30)
    (case / f"{name}.stdout.log").write_bytes(process.stdout)
    (case / f"{name}.stderr.log").write_bytes(process.stderr)
    cli_calls.append({"case": case.name, "name": name, "args": list(map(str, command)), "exit_code": process.returncode})
    assert process.returncode == expected, (name, process.returncode, process.stderr)
    return process


fiction = (repo / "tests/fixtures/model-candidate-v1/source.md").read_bytes()
assert sha(fiction) == "5aa51f5816ec39ac4eefd7a8299def081d33a2277a8fdddb66243b4a741c7591"
cases = [("fictional-r7", fiction, fiction, 0),
         ("crlf-exact-limit", b"\xef\xbb\xbf" + b"a\r\n" * 4095 + b"ab", b"a\n" * 4095 + b"ab", 0),
         ("over-limit", b"a" * 8193, b"a" * 8193, 2)]
results = []
for name, raw, normalized, expected in cases:
    case = root / name
    case.mkdir()
    project = case / "project"
    source = case / "selected.md"
    source.write_bytes(raw)
    sentinel = case / "outside-project.txt"
    sentinel.write_bytes(b"outside project protected\n")
    call(case, "init", ["project", "init", project], 0)
    imported = json.loads(call(case, "import", ["source", "import", project, source], 0).stdout)
    source.rename(case / "original-moved.md")
    before = snapshot(project)
    protected_before = {"source": sha(raw), "sentinel": sha(sentinel.read_bytes())}
    outputs = [call(case, f"preflight-{i}", ["knowledge", "preflight", project, imported["version_id"]], expected)
               for i in range(1, 3)]
    assert before == snapshot(project)
    assert protected_before == {"source": sha((case / "original-moved.md").read_bytes()), "sentinel": sha(sentinel.read_bytes())}
    assert outputs[0].stdout == outputs[1].stdout and outputs[0].stderr == outputs[1].stderr
    if expected == 0:
        value = json.loads(outputs[0].stdout)
        assert value["result"] == "source_ready" and value["api_status"] == "not_called"
        assert value["externalization_authorized"] is False
        assert value["source"]["raw_byte_length"] == len(raw)
        assert value["source"]["normalized_byte_length"] == len(normalized)
        assert value["source"]["raw_sha256"] == sha(raw)
        assert value["source"]["normalized_sha256"] == sha(normalized)
    else:
        assert not outputs[0].stdout and b"MODEL_" in outputs[0].stderr
    save(case / "project-before.json", before)
    save(case / "project-after.json", snapshot(project))
    results.append({"case": name, "raw_bytes": len(raw), "normalized_bytes": len(normalized),
                    "project_files": len(before), "preflight_calls": 2, "exit_code": expected,
                    "project_unchanged": True, "protected_files_unchanged": True, "outputs_identical": True})
save(root / "results.json", {"schema_version": 1, "real_api_calls": 0, "real_credentials_read": False,
                            "cli_sha256": sha(args.cli.read_bytes()), "cases": results, "cli_calls": cli_calls})
print(json.dumps({"root": str(root), "cases": results, "real_api_calls": 0}, ensure_ascii=False))
