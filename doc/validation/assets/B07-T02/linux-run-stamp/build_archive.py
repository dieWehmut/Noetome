"""Standalone source retained BEFORE generation of this transaction package.

Archives the actual completed new stamp Linux matrix and compiler-source originals. No
product commands run here. Generated references have their own manifest entry.
"""
import hashlib
import json
from pathlib import Path
import zipfile


REPO = Path("/home/lyinn/project/Noetome")
HERE = Path(__file__).parent
OUTPUT = HERE / "package"
MATRIX = REPO / ".artifacts/b07-t02-request-linux-stamp-full-01-20261107"
LAUNCHER = REPO / ".artifacts/b07-t02-linux-stamp-full-launch-01-20261107"
PAIR = REPO / ".artifacts/b07-t02-linux-stamp-pair-01-20261107"
PRIOR = REPO / ".artifacts/b07-t02-deadline-handle-history-20261107/package"


def pin(path):
    before = path.stat()
    raw = path.read_bytes()
    after = path.stat()
    assert (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_nlink) == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_nlink)
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw), "device": after.st_dev, "inode": after.st_ino, "mtime_ns": after.st_mtime_ns, "nlink": after.st_nlink}


def encode(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def save(path, value):
    with path.open("xb") as stream:
        stream.write(encode(value))


def main():
    assert not OUTPUT.exists(), "new package only"
    original_execution = json.loads((MATRIX / "actual.json").read_bytes())
    assert original_execution["status"] == "passed" and len(original_execution["cases"]) == 62
    assert original_execution["ordinary_CLI_count"] == 364 and original_execution["driver_count"] == 33
    assert len({command["PID"] for command in original_execution["commands"]}) == 397
    kills = [command for command in original_execution["commands"] if command["wait_action"] == "actual_OS_force_kill_and_wait"]
    assert len(kills) == 23 and all(command["exit_code"] == -9 for command in kills)
    assert all(original_execution["guards"].values())
    pairs = []
    for root in (MATRIX, LAUNCHER):
        for path in sorted(root.rglob("*")):
            if path.is_dir():
                continue
            assert path.is_file() and not path.is_symlink()
            pairs.append((root.name + "/" + path.relative_to(root).as_posix(), path))
    for name in ("actual.json", "source-before.json", "source-after.json", "source-originals.zip"):
        pairs.append((PAIR.name + "/" + name, PAIR / name))
    pairs.append(("builder/build_archive.py", Path(__file__).absolute()))
    originals = [{"member": name, "original": str(path), "pin": pin(path)} for name, path in pairs]
    assert len({entry["member"] for entry in originals}) == len(originals)
    references = {str(PRIOR / name): pin(PRIOR / name) for name in ("originals.zip", "manifest.json", "actual.json")}
    references.update({str(PAIR / name): pin(PAIR / name) for name in ("noetome", "request-test-driver")})
    reference = encode({
        "referenced_original_files": references,
        "generation_program_original": {"path": str(Path(__file__).absolute()), "pin": pin(Path(__file__).absolute())},
        "scope": "Prior Native6dd elapsed NoFit failures retain their own historical counts and are not reused as this source8e4d/4ca30 new Linux run. Selected executable bodies remain separately anchored actual compiler originals and are not duplicated here. This generated reference has no separately existing ordinary original file.",
    })
    OUTPUT.mkdir()
    with zipfile.ZipFile(OUTPUT / "originals.zip", "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for entry in originals:
            archive.writestr(entry["member"], Path(entry["original"]).read_bytes())
        archive.writestr("prior-native-failure-and-program-originals-reference.json", reference)
    with zipfile.ZipFile(OUTPUT / "originals.zip") as archive:
        assert len(archive.namelist()) == len(originals) + 1
        for entry in originals:
            raw = archive.read(entry["member"])
            assert hashlib.sha256(raw).hexdigest() == entry["pin"]["sha256"] and len(raw) == entry["pin"]["byte_length"]
        assert archive.read("prior-native-failure-and-program-originals-reference.json") == reference
    assert all(pin(Path(entry["original"])) == entry["pin"] for entry in originals)
    assert references == {name: pin(Path(name)) for name in references}
    scope = "Current read-handle-stamp optimized8e4d/4ca30 and checker3b/test428 Linux62 complete:364ordinaryCLI+33driver=397PIDs,70HTTP,23actualSIGKILL/wait. No extra3targets; prior matrices and NativeNoFit separate. Entire archived selected compiler source scope retained, no executable body duplication. No fullPython/Rust inferred from this matrix. Native new stamp-source matrix remains pending."
    manifest = {
        "ordinary_original_files": len(originals), "generated_reference_members": 1, "total_zip_members": len(originals) + 1,
        "members": originals,
        "embedded_reference": {"name": "prior-native-failure-and-program-originals-reference.json", "sha256": hashlib.sha256(reference).hexdigest(), "byte_length": len(reference), "generation_program": "builder/build_archive.py"},
        "scope": scope,
    }
    save(OUTPUT / "manifest.json", manifest)
    save(OUTPUT / "actual.json", {
        "status": "current_stamp_Linux_originals_CRC_SHA_length_readback_passed",
        "ordinary_original_files": len(originals), "generated_reference_members": 1, "members": len(originals) + 1,
        "CRC_SHA_length_all": True, "ordinary_original_identity_mtime_bytes_before_after_equal": True,
        "zip": pin(OUTPUT / "originals.zip"), "manifest": pin(OUTPUT / "manifest.json"),
        "generation_program": pin(Path(__file__).absolute()), "execution_original": pin(MATRIX / "actual.json"),
        "scope": scope,
    })
    print(json.dumps({"status": "passed", "package": str(OUTPUT), "ordinary": len(originals), "generated": 1, "zip": pin(OUTPUT / "originals.zip"), "manifest": pin(OUTPUT / "manifest.json"), "actual": pin(OUTPUT / "actual.json")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
