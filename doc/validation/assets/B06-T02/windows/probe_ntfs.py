"""Actual Windows/NTFS admission checks for the fixed B06-T02 native snapshot.

Run only after the six native gates have terminated on the same source/CLI.
This creates a new authored CLI case and self-owned links; it never recursively
cleans a directory or treats unavailable symlink privileges as a passed check.
Held-period write/delete denial is covered separately by native Rust tests.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time
import traceback
import uuid


def pin(path):
    body = Path(path).read_bytes()
    return {"sha256": hashlib.sha256(body).hexdigest(), "byte_length": len(body)}


def write(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def reparse(path):
    return bool(path.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)


def ordinary_ancestors(path):
    for part in (path, *path.parents):
        assert not reparse(part), "probe itself must start through ordinary components: " + str(part)


def inventory(root):
    result = {}

    def visit(path):
        assert not reparse(path), "protected inventory never follows reparse points: " + str(path)
        if path.is_file():
            result[str(path)] = pin(path)
        else:
            assert path.is_dir(), "ordinary directory required"
            for child in sorted(path.iterdir()):
                visit(child)

    visit(root)
    return result


def volume_info(path):
    """Read the actual mounted filesystem via Windows volume APIs."""
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetVolumePathNameW.argtypes = [wintypes.LPCWSTR, wintypes.LPWSTR, wintypes.DWORD]
    kernel.GetVolumePathNameW.restype = wintypes.BOOL
    kernel.GetVolumeInformationW.argtypes = [
        wintypes.LPCWSTR, wintypes.LPWSTR, wintypes.DWORD,
        ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(wintypes.DWORD),
        ctypes.POINTER(wintypes.DWORD), wintypes.LPWSTR, wintypes.DWORD,
    ]
    kernel.GetVolumeInformationW.restype = wintypes.BOOL
    root = ctypes.create_unicode_buffer(32768)
    if not kernel.GetVolumePathNameW(str(path), root, len(root)):
        raise ctypes.WinError(ctypes.get_last_error())
    name, filesystem = ctypes.create_unicode_buffer(261), ctypes.create_unicode_buffer(261)
    serial, component_length, flags = wintypes.DWORD(), wintypes.DWORD(), wintypes.DWORD()
    if not kernel.GetVolumeInformationW(root.value, name, len(name), ctypes.byref(serial),
                                       ctypes.byref(component_length), ctypes.byref(flags),
                                       filesystem, len(filesystem)):
        raise ctypes.WinError(ctypes.get_last_error())
    return dict(root=root.value, name=name.value, filesystem=filesystem.value,
                serial=serial.value, maximum_component_length=component_length.value, flags=flags.value)


class Probe:
    def __init__(self, root):
        assert sys.platform == "win32" and sys.version_info[:3] == (3, 12, 14), "actual selected Windows Python required"
        self.root = root.absolute()
        ordinary_ancestors(self.root)
        self.cfg = read_json(self.root / "root.json")
        self.snapshot = read_json(self.root / "snapshot.json")
        self.native = read_json(self.root / "native-final.json")
        required = {"fmt", "workspace-check", "workspace-tests", "python-full", "presentation-audit", "process-flow-audit"}
        assert set(self.native["required_latest_exits"]) == required
        assert all(code == 0 for code in self.native["required_latest_exits"].values())
        assert self.cfg["snapshot_head"] == self.snapshot["head"] == self.native["head"]
        self.repo = Path(self.cfg["repo_native"])
        self.cli = self.repo / "target/debug/noetome.exe"
        ordinary_ancestors(self.cli)
        assert self.cli.stat().st_nlink == 1 and pin(self.cli) == self.native["cli"]
        assert os.path.samefile(sys.executable, self.cfg["python_native"])
        self.volume = volume_info(self.root)
        assert self.volume["filesystem"].upper() == "NTFS", "actual NTFS filesystem required"
        self.work = self.root / ("ntfs-probe-" + uuid.uuid4().hex[:12])
        self.work.mkdir()
        self.external = self.root.parent / (self.work.name + "-external-sentinel")
        self.external.mkdir()
        (self.external / "human-sentinel.txt").write_bytes(b"self-owned external B06-T02 sentinel; must remain byte-exact\n")
        self.base = self.work / "authored-linear-case"
        self.base.mkdir()
        self.project, self.case = self.base / "project", self.base / "case"
        self.review, self.vault = self.base / "review", self.base / "Vault"
        self.profile, self.expected = self.base / "profile.json", self.base / "expected.json"
        self.case.mkdir()
        self.commands, self.scenarios, self.links = [], [], []
        self.protected_trees = {str(self.external): inventory(self.external)}
        critical = [name for name in self.snapshot["files"] if
                    name.startswith(("crates/noetome-cli/src/process", "fixtures/process-flow-v1/"))
                    or name in {"crates/noetome-cli/src/extraction_output.rs", "crates/noetome-cli/src/extraction_source.rs",
                                "crates/noetome-cli/src/knowledge_coordinate_input.rs", "crates/noetome-cli/src/sidecar_workspace.rs",
                                "scripts/verify_process_flow.py", "scripts/verify_extraction_knowledge.py",
                                "scripts/verify_knowledge_coordination.py", "scripts/verify_knowledge_conflicts.py"}]
        assert critical, "fixed source pins required"
        self.protected_files = {str(self.repo / name): pin(self.repo / name) for name in critical}
        assert all(self.protected_files[str(self.repo / n)] == self.snapshot["files"][n] for n in critical)
        for path in [self.cli, self.root / "snapshot.json", self.root / "native-final.json", Path(__file__).absolute()]:
            self.protected_files[str(path)] = pin(path)
        runtime_temp = self.work / "runtime-temp"
        runtime_temp.mkdir()
        self.env = os.environ.copy()
        self.env.update(NOETOME_TEST_PYTHON=self.cfg["fake_python_native"], NOETOME_PDF_PYTHON=self.cfg["python_native"],
                        NOETOME_TEST_BINARY=str(self.cli), PYTHONUTF8="1", TEMP=str(runtime_temp), TMP=str(runtime_temp))
        write(self.work / "runtime.json", dict(platform=sys.platform, python=sys.version, executable=sys.executable,
              volume=self.volume, snapshot_head=self.snapshot["head"], cli={"path": str(self.cli), **pin(self.cli)},
              driver={"path": str(Path(__file__).absolute()), **pin(__file__)},
              selected_env={key: self.env[key] for key in ("NOETOME_TEST_PYTHON", "NOETOME_PDF_PYTHON", "NOETOME_TEST_BINARY", "TEMP", "TMP")}))

    def unchanged(self):
        assert {path: pin(path) for path in self.protected_files} == self.protected_files, "protected source/input/CLI bytes changed"
        assert {root: inventory(Path(root)) for root in self.protected_trees} == self.protected_trees, "protected inventory or external sentinel changed"

    def call(self, name, argv, expected=(0,), diagnostic=None):
        self.unchanged()
        prefix = self.work / (str(len(self.commands)).zfill(3) + "-" + name)
        stdout, stderr = prefix.with_suffix(".stdout.bin"), prefix.with_suffix(".stderr.bin")
        start = time.monotonic()
        started = datetime.now(timezone.utc).isoformat()
        timed_out, cleanup = False, None
        args = [str(a) for a in argv]
        print("START", name, flush=True)
        with stdout.open("xb") as out, stderr.open("xb") as err:
            proc = subprocess.Popen(args, cwd=self.repo, env=self.env, stdout=out, stderr=err)
            write(self.work / "running.json", dict(name=name, argv=args, pid=proc.pid, status="running", started_utc=started))
            try:
                code = proc.wait(timeout=90)
            except subprocess.TimeoutExpired:
                timed_out = True
                killed = subprocess.run(["taskkill.exe", "/PID", str(proc.pid), "/T", "/F"], capture_output=True, timeout=15)
                kill_out, kill_err = prefix.with_suffix(".kill.stdout.bin"), prefix.with_suffix(".kill.stderr.bin")
                kill_out.write_bytes(killed.stdout); kill_err.write_bytes(killed.stderr)
                cleanup = dict(exit_code=killed.returncode, stdout={"path": str(kill_out), **pin(kill_out)}, stderr={"path": str(kill_err), **pin(kill_err)})
                code = proc.wait(timeout=15)
        preservation = None
        try:
            self.unchanged()
        except Exception as failure:
            preservation = repr(failure)
        receipt = dict(name=name, argv=args, cwd=str(self.repo), pid=proc.pid, started_utc=started,
                       elapsed_seconds=time.monotonic() - start, exit_code=code, expected_exits=list(expected),
                       expected_diagnostic=diagnostic, timed_out=timed_out, timeout_cleanup=cleanup,
                       stdout={"path": str(stdout), **pin(stdout)}, stderr={"path": str(stderr), **pin(stderr)},
                       protected_before_after_match=preservation is None, preservation_error=preservation)
        self.commands.append(receipt)
        write(prefix.with_suffix(".receipt.json"), receipt)
        write(self.work / "commands.json", self.commands)
        write(self.work / "running.json", dict(name=name, pid=proc.pid, status="terminal", exit_code=code))
        print("END", name, code, flush=True)
        assert preservation is None, preservation
        assert not timed_out and code in expected, (name, code, stderr.read_bytes().decode("utf-8", errors="replace"))
        if diagnostic:
            assert stderr.read_bytes().startswith(diagnostic.encode()), (name, stderr.read_bytes())
        return stdout.read_bytes()

    def command(self, verb, *, project=None, case=None, review=None, profile=None, output=None):
        args = [self.cli, "knowledge", verb, project or self.project, case or self.case]
        if verb == "flow":
            args += [review or self.review, profile or self.profile]
        return args + [output]

    def rejection(self, name, verb, *, physical_output=None, diagnostic=None, **overrides):
        output = overrides.pop("output", self.work / ("rejected-" + name))
        physical = physical_output or output
        assert not os.path.lexists(physical), "new rejection target must initially be absent"
        self.call(name, self.command(verb, output=output, **overrides), (2, 3), diagnostic)
        assert not os.path.lexists(physical), "rejected command created output: " + str(physical)
        self.scenarios.append(dict(name=name, verb=verb, status="passed", observed="rejected", output_absent=True))

    def checker(self, name, *, review_only=False):
        selected = [self.project, self.case, self.review]
        selected += [self.author_expected] if review_only else [self.profile, self.vault, self.author_expected]
        self.call(name, [self.cfg["python_native"], "-X", "utf8", self.repo / "scripts/verify_process_flow.py",
                         "check-review" if review_only else "check", *selected])

    def prepare_case(self):
        fixture = self.repo / "fixtures/process-flow-v1"
        frozen = read_json(fixture / "frozen-sha256.json")
        assert all(pin(fixture / name) == expected for name, expected in frozen["files"].items())
        author = next(c for c in read_json(fixture / "authors.json")["cases"] if c["name"] == "linear")
        # The checker binds expected to the complete signed fixture directory.
        # The base/expected.json copy is retained evidence, not a new oracle.
        self.author_expected = fixture / author["expected_file"]
        for field, target in [("source_file", self.base / "source.txt"), ("gold_file", self.case / "gold.json"),
                              ("profile_file", self.profile), ("expected_file", self.expected)]:
            target.write_bytes((fixture / author[field]).read_bytes())
            self.protected_files[str(target)] = pin(target)
        write(self.work / "frozen-author.json", dict(author=author, frozen_manifest=pin(fixture / "frozen-sha256.json")))
        self.call("project-init", [self.cli, "project", "init", self.project])
        imported = json.loads(self.call("source-import", [self.cli, "source", "import", self.project, self.base / "source.txt"]))
        write(self.case / "process-review.json", dict(schema_version=1, purpose="process_source_review",
              project_id=read_json(self.project / "project.json")["project_id"], import_id=imported["import_id"],
              selection=[dict(kind="text", ordinal=0)], gold_file="gold.json", gold_sha256=pin(self.case / "gold.json")["sha256"]))
        self.protected_trees[str(self.project)] = inventory(self.project)
        self.protected_trees[str(self.case)] = inventory(self.case)
        review = json.loads(self.call("baseline-process-review", self.command("process-review", output=self.review)))
        assert review["status"] == "passed" and review["published"] is False
        self.protected_trees[str(self.review)] = inventory(self.review)
        flow = json.loads(self.call("baseline-flow", self.command("flow", output=self.vault)))
        assert flow["status"] == "passed" and flow["representation"] == "flowchart" and flow["published"] is False
        self.protected_trees = {str(self.external): inventory(self.external), str(self.base): inventory(self.base)}
        write(self.work / "protected-before.json", dict(files=self.protected_files, trees=self.protected_trees))
        self.checker("independent-review-before", review_only=True)
        self.checker("independent-flow-before")

    def link_record(self, path, target, kind):
        metadata = path.lstat()
        result = dict(path=str(path), target=str(target), kind=kind, device=metadata.st_dev, inode=metadata.st_ino,
                      file_attributes=metadata.st_file_attributes, reparse_tag=metadata.st_reparse_tag,
                      reparse_attribute=reparse(path), readlink=os.readlink(path),
                      is_junction=os.path.isjunction(path), is_symlink=path.is_symlink(), same_target=os.path.samefile(path, target))
        assert result["reparse_attribute"] and result["same_target"]
        assert result["is_junction"] if kind == "junction" else result["is_symlink"]
        self.links.append(result)
        write(self.work / "link-operations.json", self.links)
        return path

    def junction(self, name, target):
        path = self.work / name
        assert not os.path.lexists(path)
        assert not any(c in str(path) + str(target) for c in '\"%&|<>^!\r\n'), "controlled mklink paths required"
        # Pass the builtin and path tokens separately. A single /c string with
        # embedded quotes gains literal backslashes under list2cmdline.
        self.call("create-" + name, ["cmd.exe", "/d", "/c", "mklink", "/J", path, target])
        return self.link_record(path, target, "junction")

    def link_matrix(self, alias, kind, output_alias):
        for verb in ("process-review", "flow"):
            roles = ("project", "case") if verb == "process-review" else ("project", "case", "review", "profile")
            for role in roles:
                original = getattr(self, role)
                diagnostic = "PROCESS_INPUT" if role in ("case", "review") else "PROJECT_UNSAFE_PATH"
                self.rejection(kind + "-" + verb + "-" + role, verb, diagnostic=diagnostic, **{role: alias / original.name})
            leaf = kind + "-" + verb + "-output"
            self.rejection(leaf, verb, output=output_alias / leaf, physical_output=self.external / leaf, diagnostic="PROJECT_UNSAFE_PATH")

    def hardlinks(self):
        for role, original, verbs in [
            ("case-manifest", self.case / "process-review.json", ("process-review", "flow")),
            ("gold", self.case / "gold.json", ("process-review", "flow")),
            ("profile", self.profile, ("flow",)),
            ("saved-review-ir", self.review / "process-ir.json", ("flow",)),
        ]:
            assert original.stat().st_nlink == 1
            alias = self.work / ("hardlink-" + role + ".json")
            os.link(original, alias)
            identity = alias.lstat()
            try:
                observation = dict(original=str(original), alias=str(alias), original_before=pin(original), alias_pin=pin(alias),
                                   native_link_count=original.stat().st_nlink, same_file=os.path.samefile(original, alias))
                write(self.work / ("hardlink-" + role + ".json.receipt"), observation)
                assert observation["same_file"] and observation["native_link_count"] == 2
                for verb in verbs:
                    self.rejection("hardlink-" + role + "-" + verb, verb,
                                   diagnostic="PROCESS_PROFILE_PATH" if role == "profile" else "PROCESS_INPUT")
            finally:
                current = alias.lstat()
                assert (current.st_dev, current.st_ino) == (identity.st_dev, identity.st_ino) and os.path.samefile(original, alias)
                alias.unlink()  # Remove only this newly created name, never the original.
                assert original.stat().st_nlink == 1
                self.unchanged()

    def run(self):
        self.prepare_case()
        alias = self.junction("input-ancestor-junction", self.base)
        output_alias = self.junction("output-parent-junction", self.external)
        self.link_matrix(alias, "junction", output_alias)
        for verb in ("process-review", "flow"):
            lexical = str(output_alias) + "\\..\\" + output_alias.name + "\\lexical-" + verb
            self.rejection("junction-parent-traversal-" + verb, verb, output=lexical,
                           physical_output=self.external / ("lexical-" + verb), diagnostic="PROJECT_UNSAFE_PATH")
            existing = self.review if verb == "process-review" else self.vault
            self.call("existing-result-" + verb, self.command(verb, output=existing), (2,), "EXTRACT_OUTPUT_EXISTS")
            self.scenarios.append(dict(name="existing-result-" + verb, verb=verb, status="passed", observed="rejected", original_inventory_unchanged=True))
            self.call("existing-file-" + verb, self.command(verb, output=self.external / "human-sentinel.txt"), (2,), "EXTRACT_OUTPUT_EXISTS")
            self.scenarios.append(dict(name="existing-file-" + verb, verb=verb, status="passed", observed="rejected", external_sentinel_unchanged=True))
            for scope in ("project", "case") if verb == "process-review" else ("project", "case", "review"):
                self.rejection("nested-" + verb + "-" + scope, verb, output=getattr(self, scope) / ("nested-" + verb), diagnostic="EXTRACT_OUTPUT_OVERLAP")
        self.hardlinks()
        symlink, symlink_output = self.work / "input-ancestor-symlink", self.work / "output-parent-symlink"
        try:
            os.symlink(self.base, symlink, target_is_directory=True)
        except OSError as failure:
            assert not os.path.lexists(symlink)
            if getattr(failure, "winerror", None) not in (5, 50, 1314):
                raise
            self.scenarios.append(dict(name="symlink-admission", status="not_run", reason=str(failure),
                                       exception=type(failure).__name__, winerror=getattr(failure, "winerror", None),
                                       planned_cli_scenarios=8, scope="privilege/platform unavailable; no symlink CLI scenario was executed"))
            write(self.work / "symlink-unavailable.json", self.scenarios[-1])
        else:
            self.link_record(symlink, self.base, "symlink")
            os.symlink(self.external, symlink_output, target_is_directory=True)
            self.link_record(symlink_output, self.external, "symlink")
            self.link_matrix(symlink, "symlink", symlink_output)
        self.checker("independent-review-after", review_only=True)
        self.checker("independent-flow-after")
        self.unchanged()
        write(self.work / "protected-after.json", dict(files=self.protected_files, trees=self.protected_trees))

    def finish(self, failure):
        cleanup = []
        for link in reversed(self.links):
            path = Path(link["path"])
            try:
                metadata = path.lstat()
                assert reparse(path) and (metadata.st_dev, metadata.st_ino) == (link["device"], link["inode"])
                assert os.readlink(path) == link["readlink"], "preserve a changed link"
                path.rmdir()  # Only the captured reparse-point pathname; never recurse.
                cleanup.append(dict(path=str(path), removed=True, target_preserved=Path(link["target"]).is_dir()))
            except Exception as problem:
                cleanup.append(dict(path=str(path), removed=False, error=repr(problem)))
                failure = failure or "self-owned link cleanup failed"
        preservation_error = None
        try:
            self.unchanged()
        except Exception as problem:
            preservation_error = repr(problem)
            failure = failure or repr(problem)
        final = dict(status="failed" if failure else "passed", failure=failure, snapshot_head=self.snapshot["head"],
                     platform=sys.platform, python=sys.version, volume=self.volume, work=str(self.work),
                     cli=pin(self.cli), commands=self.commands, scenarios=self.scenarios, cleanup=cleanup,
                     cli_calls=sum(c["argv"][0] == str(self.cli) for c in self.commands),
                     executed_admission_scenarios=sum(s["status"] == "passed" for s in self.scenarios),
                     not_run_scenarios=[s for s in self.scenarios if s["status"] == "not_run"],
                     protected_before_after_match=preservation_error is None,
                     preservation_error=preservation_error,
                     scope="Actual native NTFS command admission and original-byte protection; unavailable symlink rows are not executions. Held-period denial and Obsidian validation are separate.")
        write(self.work / "cleanup.json", cleanup)
        write(self.work / "final.json", final)
        write(self.root / "native-ntfs-pointer.json", dict(work=str(self.work), status=final["status"], final=pin(self.work / "final.json")))
        return final


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="Fixed B06-T02 native validation root, after all six gates terminate")
    args = parser.parse_args()
    probe = Probe(args.root)
    failure = None
    try:
        probe.run()
    except Exception:
        failure = traceback.format_exc()
        (probe.work / "failure.txt").write_text(failure, encoding="utf-8")
    result = probe.finish(failure)
    print("ACTUAL_NTFS_ADMISSION_PASSED" if result["status"] == "passed" else "ACTUAL_NTFS_ADMISSION_FAILED", probe.work, flush=True)
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
