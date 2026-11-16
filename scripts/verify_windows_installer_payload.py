"""Read-only byte, runtime and resource audit for the opt-in Windows payload."""

from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PRODUCER = "noetome-windows-installer-payload"
ARCHIVE_SHA = "f38e68f4d612ade6dd50c894fc80b14c0be0c3b5201145d6fff5f20b9323204d"
MANIFEST = "payload-manifest.json"
CHECKSUM = "payload-manifest.sha256"
HELPERS = (
    "invoke_text_api_credential.ps1",
    "invoke_compile_credential.ps1",
    "text_api_support.ps1",
    "text_model_profile.ps1",
    "set_text_api_credential.ps1",
)
TREES = ("run-checkpoint-v1", "text-compare-v1", "pdf-text-v1")
LEDGER = "doc/validation/assets/B06V-T02/recognition/vision-live-final/actual-final-ledger.json"
PROOF = "doc/validation/assets/B06V-T02/review/continuation-current-ledger.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def strict_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate JSON key")
            result[key] = value
        return result

    return json.loads(data.decode("utf-8-sig"), object_pairs_hook=pairs)


def plain(path):
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
        raise ValueError(f"linked/reparse path: {path.name}")
    if not (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode)):
        raise ValueError(f"nonordinary path: {path.name}")
    if stat.S_ISREG(info.st_mode) and info.st_nlink != 1:
        raise ValueError(f"hardlinked file: {path.name}")
    return info


def inventory(root):
    for parent in (root, *root.parents):
        plain(parent)
    result = {}
    stack = [root]
    while stack:
        parent = stack.pop()
        for path in parent.iterdir():
            info = plain(path)
            if stat.S_ISDIR(info.st_mode):
                stack.append(path)
            else:
                result[path.relative_to(root).as_posix()] = path
    return result


def safe_relative(value):
    if (
        not isinstance(value, str)
        or not value
        or "\\" in value
        or ":" in value
        or "\0" in value
    ):
        return False
    p = PurePosixPath(value)
    return (
        not p.is_absolute()
        and str(p) == value
        and all(
            part not in (".", "..") and not part.endswith((".", " "))
            for part in p.parts
        )
    )


def pe_x64(data):
    if len(data) < 64 or data[:2] != b"MZ":
        return False
    offset = struct.unpack_from("<I", data, 60)[0]
    return (
        offset + 6 <= len(data)
        and data[offset : offset + 4] == b"PE\0\0"
        and struct.unpack_from("<H", data, offset + 4)[0] == 0x8664
    )


def repository_resources(repo):
    result = {f"helpers/{name}": repo / "scripts" / name for name in HELPERS}
    result.update(
        {
            "fixtures/model-ledger/actual-final-ledger.json": repo / LEDGER,
            "fixtures/model-ledger/continuation-current-ledger.json": repo / PROOF,
            "samples/single-page.pdf": repo / "fixtures/pdf-text-v1/single-page.pdf",
            "QUICKSTART.zh-CN.md": repo / "doc/windows/QUICKSTART.zh-CN.md",
            "licenses/winsafe-0.0.29-MIT.md": repo / "doc/licenses/winsafe-0.0.29-MIT.md",
            "runtime-lock.json": repo / "sidecars/pdf_parser/runtime-lock.json",
            "requirements-lock.txt": repo / "sidecars/pdf_parser/requirements-lock.txt",
        }
    )
    result["samples/scanned-page.pdf"] = repo / "fixtures/pdf-text-v1/scanned-page.pdf"
    for tree in TREES:
        parent = repo / "fixtures" / tree
        for name, path in inventory(parent).items():
            result[f"fixtures/{tree}/{name}"] = path
    return result


def audit(root, *, repo=ROOT, probe_runtime=False, manifest_sha256=None):
    checks = []

    def check(name, ok, detail=""):
        checks.append(
            {"name": name, "status": "passed" if ok else "failed", "detail": detail}
        )

    try:
        actual = inventory(root)
        raw = actual[MANIFEST].read_bytes()
        manifest = strict_json(raw)
        check(
            "manifest_checksum",
            actual[CHECKSUM].read_bytes() == (digest(raw) + "\n").encode("ascii"),
        )
        if manifest_sha256:
            check("external_manifest_sha256", digest(raw) == manifest_sha256)
        check(
            "manifest_shape",
            set(manifest)
            == {
                "schema_version",
                "producer",
                "source_commit",
                "target",
                "build_profile",
                "version",
                "source_inputs",
                "totals",
                "files",
            },
        )
        check(
            "manifest_identity",
            manifest["schema_version"] == 1
            and manifest["producer"] == PRODUCER
            and manifest["target"] == "x86_64-pc-windows-msvc"
            and manifest["build_profile"] in ("release", "debug")
            and bool(re.fullmatch("[0-9a-f]{40}", manifest["source_commit"])),
        )
        entries = manifest["files"]
        if not isinstance(entries, list):
            raise ValueError("files must be an array")
        names = [row["path"] for row in entries]
        check(
            "paths_sorted_unique_safe",
            names == sorted(set(names)) and all(safe_relative(name) for name in names),
        )
        check("exact_file_inventory", set(names) == set(actual) - {MANIFEST, CHECKSUM})
        trusted_resources = repository_resources(repo)
        allowed = set(trusted_resources) | {
            "noetome.exe",
            "vcruntime140.dll",
            "README.md",
            "runtime-receipt.json",
        }
        check(
            "only_declared_resource_roots",
            all(name in allowed or name.startswith("pdf-python/") for name in names),
        )
        check(
            "windows_case_unique",
            len({name.casefold() for name in names}) == len(names),
        )
        check(
            "version_matches_source",
            manifest["version"]
            == strict_json(
                (repo / "crates/noetome-desktop/tauri.conf.json").read_bytes()
            )["version"],
        )

        for row in entries:
            if set(row) != {"path", "bytes", "sha256", "source"} or not safe_relative(
                row["path"]
            ):
                raise ValueError("invalid file row")
            data = actual[row["path"]].read_bytes()
            check(
                "bytes:" + row["path"],
                type(row["bytes"]) is int
                and row["bytes"] >= 0
                and len(data) == row["bytes"]
                and bool(re.fullmatch("[0-9a-f]{64}", row["sha256"]))
                and digest(data) == row["sha256"]
                and isinstance(row["source"], str)
                and bool(row["source"]),
            )
        check(
            "totals",
            manifest["totals"]
            == {"files": len(entries), "bytes": sum(row["bytes"] for row in entries)},
        )
        for name, source in trusted_resources.items():
            check(
                "source:" + name,
                name in actual and actual[name].read_bytes() == source.read_bytes(),
            )
        opening_ledger = actual[
            "fixtures/model-ledger/actual-final-ledger.json"
        ].read_bytes()
        check(
            "frozen_full_ledger_anchor",
            len(opening_ledger) == 23989
            and digest(opening_ledger)
            == "ea6b32432cb8bc1c325be48be01ce42f1dde933f2fc35f8c739c9d3fc07d8ffd",
        )
        required = (
            "noetome.exe",
            "vcruntime140.dll",
            "README.md",
            "runtime-receipt.json",
            "pdf-python/python.exe",
            "pdf-python/python312.dll",
            "pdf-python/vcruntime140.dll",
            "pdf-python/LICENSE.txt",
            "pdf-python/Lib/os.py",
            "pdf-python/Lib/site.py",
        )
        for name in required:
            check("required:" + name, name in actual)
        for name in (
            "noetome.exe",
            "pdf-python/python.exe",
            "pdf-python/python312.dll",
            "vcruntime140.dll",
        ):
            check(
                "pe_x64:" + name, name in actual and pe_x64(actual[name].read_bytes())
            )
        check(
            "vc_runtime_same_trusted_tree",
            actual["vcruntime140.dll"].read_bytes()
            == actual["pdf-python/vcruntime140.dll"].read_bytes(),
        )
        lock = strict_json(actual["runtime-lock.json"].read_bytes())
        receipt = strict_json(actual["runtime-receipt.json"].read_bytes())
        identity = receipt["identity"]
        check(
            "runtime_pin",
            receipt["schema_version"] == 1
            and receipt["producer"] == "noetome-locked-windows-pdf-runtime"
            and receipt["archive_sha256"] == ARCHIVE_SHA
            and receipt["python_executable_sha256"]
            == digest(actual["pdf-python/python.exe"].read_bytes())
            and receipt["requirements_sha256"]
            == digest(actual["requirements-lock.txt"].read_bytes())
            and identity["runtime_lock_sha256"]
            == digest(actual["runtime-lock.json"].read_bytes())
            and identity["python_version"] == lock["python_version"] == "3.12.14"
            and identity["platform"] == "win32"
            and identity["architecture"].lower() in ("amd64", "x86_64")
            and identity["packages"] == lock["packages"],
        )
        licenses = identity["licenses"]
        check("license_packages", set(licenses) == set(lock["packages"]))
        for package in lock["packages"]:
            paths = licenses.get(package, [])
            check(
                "licenses:" + package,
                bool(paths)
                and all(
                    safe_relative(path)
                    and "pdf-python/Lib/site-packages/" + path in actual
                    for path in paths
                ),
            )
        inputs = manifest["source_inputs"]
        check(
            "source_inputs",
            inputs["cli"]["sha256"] == digest(actual["noetome.exe"].read_bytes())
            and inputs["cli"]["bytes"] == len(actual["noetome.exe"].read_bytes())
            and inputs["runtime_receipt"]["sha256"]
            == digest(actual["runtime-receipt.json"].read_bytes()),
        )
        if probe_runtime:
            if os.name != "nt":
                raise ValueError("--probe-runtime requires actual native Windows")
            code = (
                "import importlib.metadata,json,platform,sys; print(json.dumps({'python':sys.version.split()[0],'platform':sys.platform,'architecture':platform.machine(),'packages':{n:importlib.metadata.version(n) for n in "
                + repr(list(lock["packages"]))
                + "}}))"
            )
            process = subprocess.run(
                [
                    str(root / "pdf-python/python.exe"),
                    "-I",
                    "-B",
                    "-X",
                    "utf8",
                    "-c",
                    code,
                ],
                capture_output=True,
                timeout=30,
            )
            observed = strict_json(process.stdout) if process.returncode == 0 else {}
            check(
                "native_runtime_probe",
                process.returncode == 0
                and observed.get("python") == "3.12.14"
                and observed.get("platform") == "win32"
                and observed.get("architecture", "").lower() in ("amd64", "x86_64")
                and observed.get("packages") == lock["packages"],
                process.stderr.decode("utf-8", "replace"),
            )
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        struct.error,
        subprocess.TimeoutExpired,
    ) as error:
        check("audit_exception", False, str(error))
    failed = sum(row["status"] == "failed" for row in checks)
    return {
        "schema_version": 1,
        "producer": "noetome-windows-installer-payload-audit",
        "status": "passed" if failed == 0 else "failed",
        "failures": failed,
        "warnings": 0,
        "checks": checks,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--probe-runtime", action="store_true")
    parser.add_argument("--manifest-sha256")
    args = parser.parse_args()
    result = audit(
        args.root,
        probe_runtime=args.probe_runtime,
        manifest_sha256=args.manifest_sha256,
    )
    print(
        json.dumps(result, ensure_ascii=False, indent=2)
        if args.json
        else f"{result['status']}: {result['failures']} failures, {len(result['checks'])} checks"
    )
    return 0 if result["failures"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
