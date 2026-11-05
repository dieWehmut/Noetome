"""Archive recorded formula inputs/captures without rerunning or fabricating results."""
import argparse
import hashlib
import json
import zipfile
from pathlib import Path


def pin(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def write(path, value):
    with path.open("x", encoding="utf8") as f:
        json.dump(value, f, ensure_ascii=False, indent=2)
        f.write("\n")


def input_files(root):
    for p in sorted(root.rglob("*")):
        assert not p.is_symlink(), "archive only explicitly owned ordinary test trees"
        if p.is_file() and p.name not in {"noetome", "noetome.exe"}:
            yield p


def capture_files(root):
    # User-data cache directories are not needed to assess rendering. Retain
    # all direct runtime receipts, scripts, DOM and screenshots, the exact
    # generated Vault, and explicit profile selection/plugin settings.
    for p in sorted(root.iterdir()):
        assert not p.is_symlink()
        if p.is_file():
            yield p
        elif p.name == "vault":
            for v in sorted(p.iterdir()):
                assert not v.is_symlink()
                if v.is_file():
                    yield v
                elif v.name == ".obsidian":
                    for q in sorted(v.glob("*.json")):
                        assert not q.is_symlink()
                        yield q
        elif p.name == "profile":
            q = p / "obsidian.json"
            if q.exists():
                yield q


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("roots", type=Path, help="JSON mapping of own input/probe/capture paths")
parser.add_argument("new_archive", type=Path)
parser.add_argument("new_manifest", type=Path)
args = parser.parse_args()
roots = json.loads(args.roots.read_bytes())
files = {}
for label, root_value in roots.get("inputs", {}).items():
    root = Path(root_value)
    for p in input_files(root):
        files["inputs/" + label + "/" + p.relative_to(root).as_posix()] = p
for label, root_value in roots.get("probe", {}).items():
    root = Path(root_value)
    for p in root.iterdir():
        if p.is_file():
            files["probe/" + label + "/" + p.name] = p
        elif p.name == "selected":
            for q in p.iterdir():
                assert q.is_file() and not q.is_symlink()
                files["probe/" + label + "/selected/" + q.name] = q
        else:
            for q in capture_files(p):
                files["probe/" + label + "/" + q.relative_to(root).as_posix()] = q
for label, root_value in roots.get("captures", {}).items():
    root = Path(root_value)
    for p in root.iterdir():
        if p.is_file():
            files["captures/" + label + "/" + p.name] = p
        else:
            for q in capture_files(p):
                files["captures/" + label + "/" + q.relative_to(root).as_posix()] = q
manifest = {}
with zipfile.ZipFile(args.new_archive, "x", zipfile.ZIP_DEFLATED) as z:
    for name, p in sorted(files.items()):
        data = p.read_bytes()
        z.writestr(name, data)
        manifest[name] = pin(data)
with zipfile.ZipFile(args.new_archive) as z:
    assert z.testzip() is None
    assert set(z.namelist()) == set(manifest)
    for name in z.namelist():
        assert pin(z.read(name)) == manifest[name]
write(args.new_manifest, {"status": "passed", "archive": pin(args.new_archive.read_bytes()),
                          "roots": roots, "member_count": len(manifest), "members": manifest,
                          "scope": "recorded selected inputs, commands, DOM/CHTML/screenshots, own plugin/vault configuration and cleanup; cache directories and reproducible CLI binary excluded"})
print(json.dumps({"status": "passed", "members": len(manifest), "archive": pin(args.new_archive.read_bytes())}))
