"""Archive completed B06-T02 native gates; never build, launch tests or follow links.

Run with the selected native Windows Python after the original validation owner
has exited. Destination must be new and outside the evidence/snapshot tree.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import stat
import sys
import zipfile


def fingerprint_bytes(raw):
    return dict(sha256=hashlib.sha256(raw).hexdigest(), byte_length=len(raw))


def fingerprint(path):
    digest, length = hashlib.sha256(), 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk); length += len(chunk)
    return dict(sha256=digest.hexdigest(), byte_length=length)


def load(path):
    return json.loads(path.read_bytes())


def save(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def metadata(path):
    info = path.lstat()
    return dict(mode=info.st_mode, attributes=getattr(info, "st_file_attributes", 0),
                links=info.st_nlink, identity=[info.st_dev, info.st_ino], mtime_ns=info.st_mtime_ns)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_root", type=Path)
    parser.add_argument("new_output", type=Path)
    parser.add_argument("--confirmed-terminal", required=True, action="store_true")
    args = parser.parse_args()
    assert sys.platform == "win32", "native lstat is required to identify NTFS reparse points"
    root, output = args.native_root.absolute(), args.new_output.absolute()
    assert output != root and not output.is_relative_to(root), "archive output outside evidence tree"
    assert not output.exists(), "new archive destination required"
    final, snapshot, config = (load(root / n) for n in ("native-final.json", "snapshot.json", "root.json"))
    commands = load(root / "native-commands.json")
    required = {"fmt", "workspace-check", "workspace-tests", "python-full", "presentation-audit", "process-flow-audit"}
    assert set(final["required_latest_exits"]) == required and all(v == 0 for v in final["required_latest_exits"].values())
    assert load(root / "native-running.json")["status"] == "terminal"
    assert final["head"] == snapshot["head"] == config["snapshot_head"]
    assert final["commands"] == commands and final["tracked_before_after_match"]
    repo = root / "repo"
    def local(value):
        path = Path(value.removeprefix("\\\\?\\"))
        assert path.is_relative_to(root), "evidence receipt outside explicit native root"
        return path
    for command in commands:
        assert command["tracked_before_after_match"] and command["external_sentinel_before_after_match"]
        for field in ("stdout", "stderr"):
            receipt = command[field]
            assert fingerprint(local(receipt["path"])) == {k: receipt[k] for k in ("sha256", "byte_length")}
    for name, expected in snapshot["files"].items():
        assert fingerprint(repo / name) == expected, ("snapshot changed", name)
    external = root.parent / (root.name + "-external-sentinel.txt")
    assert fingerprint(external) == final["external_sentinel_before"] == final["external_sentinel_after"]
    output.mkdir()
    save(output / "archive-runtime.json", dict(python=sys.version, executable=sys.executable, platform=sys.platform,
        observed_utc=datetime.now(timezone.utc).isoformat(), native_root=str(root), head=final["head"]))
    members, omitted, links, directories = {}, {}, [], {}
    binary_suffixes = {".exe", ".dll", ".pdb", ".rlib", ".rmeta", ".lib", ".o", ".obj", ".pyc"}
    def gather(path, archive_name):
        info = metadata(path)
        if stat.S_ISLNK(info["mode"]) or info["attributes"] & 0x400:
            record = dict(source=str(path), archive_name=archive_name, metadata=info, action="recorded_without_following")
            try: record["target_spelling"] = str(path.readlink())
            except OSError as error: record["readlink_error"] = str(error)
            links.append(record); return
        if stat.S_ISDIR(info["mode"]):
            directories[archive_name] = dict(source=str(path), metadata=info)
            for child in sorted(path.iterdir()): gather(child, archive_name + "/" + child.name)
            return
        assert stat.S_ISREG(info["mode"]), ("nonordinary evidence file", path)
        pin = fingerprint(path)
        if path.suffix.lower() in binary_suffixes or "__pycache__" in path.parts or path.name == "snapshot-git.tar":
            omitted[archive_name] = dict(source=str(path), metadata=info, reason="binary/build/cache or full Git tar retained locally", **pin)
            return
        assert archive_name not in members
        members[archive_name] = dict(source=str(path), metadata=info, **pin)
    # Root contains only selected launcher, snapshot and native gate receipts;
    # audit/NTFS/app directories are handled explicitly rather than walked here.
    for path in sorted(root.iterdir()):
        info = metadata(path)
        if stat.S_ISREG(info["mode"]) or stat.S_ISLNK(info["mode"]) or info["attributes"] & 0x400:
            gather(path, "native/" + path.name)
    gather(external, "native/external-sentinel.txt")
    audit_roots = {}
    for name in ("presentation-audit", "process-flow-audit"):
        audit_root = local(final["audits"][name]["root"])
        audit_roots[name] = str(audit_root)
        gather(audit_root, "audits/" + audit_root.name)
    # Capture only this task's real Rust fixture/guard roots announced on stdout.
    latest = {command["name"]: command for command in commands}
    rust_text = local(latest["workspace-tests"]["stdout"]["path"]).read_text(encoding="utf-8")
    roots = sorted({match.removeprefix("\\\\?\\") for match in re.findall(r"process_case=([^\r\n]+?) author=[a-z_]+", rust_text)})
    assert roots, "no announced process fixture roots"
    compiler = repo / "target/compiler-temp"
    held = []
    for spelling in roots:
        case = local(spelling)
        assert case.parent == compiler and re.fullmatch(r"process-[0-9a-f-]{36}", case.name)
        gather(case, "rust-process/" + case.name)
        receipt = case / "native-held-denials.json"
        if receipt.is_file():
            report = load(receipt)
            assert report["run_succeeded"] is True and report["rows"]
            for row in report["rows"]:
                assert row["write_succeeded"] is False and row["delete_succeeded"] is False
                assert row["write_os_error"] == row["delete_os_error"] == 32
            held.append(dict(source=str(receipt), fingerprint=fingerprint(receipt), review=report["review"], rows=len(report["rows"])))
    assert {record["review"] for record in held} == {True, False}, "both native process-review and flow hold receipts required"
    archive = output / "native-originals.zip"
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zipped:
        for name, record in sorted(directories.items()):
            zipped.writestr(name.rstrip("/") + "/", b"")
        for name, record in sorted(members.items()):
            path = Path(record["source"]); raw = path.read_bytes()
            assert fingerprint_bytes(raw) == {k: record[k] for k in ("sha256", "byte_length")}
            assert metadata(path) == record["metadata"], ("evidence metadata changed", path)
            zipped.writestr(name, raw)
    with zipfile.ZipFile(archive) as zipped:
        assert zipped.testzip() is None, "archive CRC read-back failed"
        assert set(zipped.namelist()) == set(members) | {name.rstrip("/") + "/" for name in directories}
        for name, record in members.items():
            actual = zipped.read(name)
            assert fingerprint_bytes(actual) == {k: record[k] for k in ("sha256", "byte_length")}
            assert actual == Path(record["source"]).read_bytes(), ("archive does not match original", name)
            assert metadata(Path(record["source"])) == record["metadata"]
            record["crc32"] = f"{zipped.getinfo(name).CRC:08x}"
        for name in directories: assert zipped.read(name.rstrip("/") + "/") == b""
    for name, expected in snapshot["files"].items():
        assert fingerprint(repo / name) == expected, ("snapshot changed during archive", name)
    assert fingerprint(external) == final["external_sentinel_after"]
    summaries = [tuple(map(int, match)) for match in re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", rust_text)]
    assert summaries and all(failed == 0 for _, failed, _ in summaries)
    python_text = local(latest["python-full"]["stderr"]["path"]).read_text(encoding="utf-8")
    count = re.search(r"Ran (\d+) tests? in ([0-9.]+)s", python_text)
    outcome = re.search(r"^OK(?: \(skipped=(\d+)\))?$", python_text, re.MULTILINE)
    assert count and outcome, "full Python successful summary required"
    held_rows = sum(record["rows"] for record in held)
    result = dict(status="passed", scope="Six native gates, their two audit roots and B06-T02 Rust process roots; NTFS and Obsidian separately archived",
        head=final["head"], snapshot_files=len(snapshot["files"]), snapshot_source_and_external_sentinel_unchanged=True,
        archive=dict(path=archive.name, **fingerprint(archive)), file_members=len(members), directory_members=len(directories),
        all_members_crc_sha_and_original_readback=True,
        rust=dict(suites=len(summaries), passed=sum(p for p, _, _ in summaries), failed=0, ignored=sum(i for _, _, i in summaries), summaries=summaries),
        python=dict(run=int(count[1]), skipped=int(outcome[1] or 0), passed=int(count[1])-int(outcome[1] or 0), elapsed_seconds=float(count[2])),
        audits=final["audits"], rust_process_roots=len(roots), native_held=dict(receipts=held, rows=held_rows, write_denied=held_rows, delete_denied=held_rows, os_error=32),
        cli=final["cli"], omitted_count=len(omitted), links_recorded_without_following=len(links))
    save(output / "native-archive.json", dict(**result, members=members, directories=directories, omitted=omitted, links=links))
    save(output / "native-summary.json", result)
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
