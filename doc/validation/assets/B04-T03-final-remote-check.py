import argparse
import hashlib
import json
from pathlib import Path
import subprocess

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
ASSETS = REPO / "doc/validation/assets"
parser = argparse.ArgumentParser()
parser.add_argument("--phase", choices=("closure", "batch"), required=True)
args = parser.parse_args()
branch = "codex/b04-t03-knowledge-verify"
head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO).decode().strip()
assert subprocess.check_output(["git", "branch", "--show-current"], cwd=REPO).decode().strip() == branch
assert subprocess.check_output(["git", "remote", "get-url", "origin"], cwd=REPO).decode().strip() == "https://github.com/dieWehmut/Noetome.git"
push = json.loads((SDK / f"root-final-{args.phase}-push-command.json").read_bytes())
assert push["passed"] and push["exit_code"] == 0 and push["head_before"] == push["head_after"] == head
run = subprocess.run(["git", "ls-remote", "--exit-code", "origin", f"refs/heads/{branch}"], cwd=REPO, capture_output=True, timeout=60)
assert run.returncode == 0, run.stderr.decode()
assert run.stdout.decode().strip() == f"{head}\trefs/heads/{branch}"
assert not subprocess.check_output(["git", "diff", "--cached", "--name-only"], cwd=REPO)
status = subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO).decode().splitlines()
if args.phase == "closure":
    assert set(status) == {" D docs/Noetome-PRD-v1.0.html", " D docs/Noetome-PRD-v1.0.md"}
result = {
    "phase": args.phase, "branch": branch, "local_head": head, "remote_head": head,
    "origin": "https://github.com/dieWehmut/Noetome.git",
    "actual_ls_remote_argv": run.args, "exit_code": run.returncode,
    "stdout": run.stdout.decode(), "stderr": run.stderr.decode(),
    "push_command_receipt": f"root-final-{args.phase}-push-command.json",
    "push_exit_code": push["exit_code"], "code_and_head_unchanged_during_push": True,
    "status_before_receipt_publication": status, "passed": True,
}
output = SDK / f"final-{args.phase}-remote.json"
with output.open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(result, stream, indent=2)
    stream.write("\n")
copies = []
if args.phase == "closure":
    pairs = [(output, "B04-T03-final-closure-remote.json"), (SDK / "final-closure-index-bytes.json", "B04-T03-final-closure-index-bytes.json"), (Path(__file__), "B04-T03-final-remote-check.py")]
    for label in ("final-closure-push", "final-devlog-complete"):
        for suffix in ("command.json", "stdout.log", "stderr.log"):
            pairs.append((SDK / f"root-{label}-{suffix}", f"B04-T03-{label}-{suffix}"))
    for source, name in pairs:
        data = source.read_bytes()
        with (ASSETS / name).open("xb") as stream:
            stream.write(data)
        assert (ASSETS / name).read_bytes() == data
        copies.append({"source": source.name, "public": name, "sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)})
    with (SDK / "final-closure-public-remote-copies.json").open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(copies, stream, indent=2)
        stream.write("\n")
print(json.dumps({"phase": args.phase, "local_head": head, "remote_head": head, "actual_ls_remote_exit": run.returncode, "new_readable_copies": len(copies), "passed": True}))
