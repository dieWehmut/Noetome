"""Pin current product/test bytes in a new Windows-only regression checkout.

Uncommitted current-task sources are included; the old Windows checkout is
read only. A snapshot is never replaced or updated after tests start.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def save(path, value):
    with path.open("x", encoding="utf8") as f:
        json.dump(value, f, ensure_ascii=False, indent=2)
        f.write("\n")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("new_root", type=Path)
parser.add_argument("--image-checker", default="scripts/verify_image.py")
args = parser.parse_args()
current = Path.cwd()
root = args.new_root
root.mkdir()
repo = root / "repo"
repo.mkdir()
names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"]).decode("utf8").split("\0")
prefixes = ("crates/", "scripts/", "sidecars/", "tests/", "fixtures/", "apps/", "schemas/")
top = {".gitattributes", ".gitignore", "AGENTS.md", "Cargo.lock", "Cargo.toml", "README.md", "package-lock.json", "package.json", "rust-toolchain.toml"}
selected = {n for n in names if n and ((n.startswith(prefixes) and "node_modules" not in Path(n).parts and "__pycache__" not in Path(n).parts) or n in top)}
# Resolve every literal compile-time source include; this captures the four
# existing B04 API response originals without copying unrelated validation.
for name in tuple(selected):
    p = current / name
    if p.suffix == ".rs":
        for rel in re.findall(r'include_(?:str|bytes)!\s*\(\s*"([^"\n]+)"', p.read_text(encoding="utf8")):
            target = (p.parent / rel).resolve()
            selected.add(target.relative_to(current).as_posix())
    if name.startswith("tests/") and p.suffix == ".py":
        for archive in re.findall(r'B[0-9A-Z-]+[a-zA-Z0-9_-]*\.zip', p.read_text(encoding="utf8")):
            target = current / "doc/validation/assets" / archive
            if target.is_file():
                selected.add(target.relative_to(current).as_posix())
    if name.startswith("scripts/") and p.suffix == ".py":
        for doc in re.findall(r'"(doc/[A-Za-z0-9_/.-]+\.md)"',p.read_text(encoding="utf8")):
            target=current/doc
            if target.is_file():selected.add(doc)
selected.update({"doc/technical/PROCESS_FLOW_CONTRACT.md", "doc/technical/IMAGE_FIDELITY_CONTRACT.md", "doc/technical/FORMULA_FIDELITY_CONTRACT.md", "doc/technical/SCAN_FORMULA_CONTRACT.md", "doc/technical/ACTIVE_PRESENTATION_CONTRACT.md", "doc/technical/EXTRACTION_API_CONTRACT.md", "doc/requirements/MODEL_SERVICE.md"})
selected.add("doc/validation/assets/B06V-T02/recognition/linux-direct-01/clear/response.json")
assert args.image_checker in selected, "selected image checker exists; Python and independent audits are a later separately pinned stage"
files = {}
for name in sorted(selected):
    p = current / name
    assert p.is_file() and not p.is_symlink(), name
    raw = p.read_bytes()
    files[name] = pin(raw)
    destination = repo / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("xb") as f:
        f.write(raw)
assert all(pin((current / n).read_bytes()) == value == pin((repo / n).read_bytes()) for n, value in files.items()), "stable copied source bytes"
save(root / "snapshot.json", {"scope": "exact current-worktree product/test inputs including uncommitted task sources; only selected old Windows repo status read",
    "base_head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(), "files": files,
    "old_windows_git_status": subprocess.check_output(["git", "-C", "/mnt/d/Noetome", "status", "--short", "--branch"], text=True)})
win = lambda p: subprocess.check_output(["wslpath", "-w", str(p)], text=True).strip()
save(root / "root.json", {"kind": "B06V-T02 final Python/helper and independent audits exact current-worktree Windows phase-two snapshot", "wsl": str(root), "native": win(root), "repo_native": win(repo),
    "target_native": "D:/Noetome/target", "python_native": "D:/Noetome/.artifacts/b03-t02-python312/Scripts/python.exe",
    "fake_python_native": "C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe", "image_checker": args.image_checker,
    "audits": ["verify_knowledge_presentation.py", "verify_process_flow.py", "verify_formula.py", Path(args.image_checker).name, "verify_scan_formula.py"]})
print(json.dumps({"status": "pinned", "root": str(root), "files": len(files)}))
