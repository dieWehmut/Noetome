"""Clean CI installation, installed payload/CLI behaviors, and owned uninstall.

No native window interactions or paid model requests are counted here. This tool
refuses existing Noetome installations and runs only on an explicit GitHub CI host.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import stat
import struct
import subprocess
import sys
import time
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
MAX_FILES = 32768
MAX_BYTES = 2 * 1024 * 1024 * 1024
# distlib carries launchers as data for creating environments on other CPUs.
# These exact four upstream wheel resources are retained, never used as runtime
# entry points, and must still match the independently selected stage bytes.
PIP_LAUNCHER_TEMPLATES = {
    "pdf-python/lib/site-packages/pip/_vendor/distlib/t32.exe": 0x14C,
    "pdf-python/lib/site-packages/pip/_vendor/distlib/w32.exe": 0x14C,
    "pdf-python/lib/site-packages/pip/_vendor/distlib/t64-arm.exe": 0xAA64,
    "pdf-python/lib/site-packages/pip/_vendor/distlib/w64-arm.exe": 0xAA64,
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def unique_json(data: bytes):
    def pairs(entries):
        result = {}
        for key, value in entries:
            if key in result:
                raise ValueError("duplicate JSON field")
            result[key] = value
        return result

    return json.loads(data.decode("utf-8-sig"), object_pairs_hook=pairs)


def fingerprint(path: Path) -> dict:
    data = path.read_bytes()
    return {"bytes": len(data), "sha256": sha(data)}


def normal(path: Path) -> Path:
    if not path.is_absolute() or ".." in path.parts or str(path).startswith("\\\\"):
        raise ValueError("paths must be ordinary absolute paths without traversal")
    for selected in (path, *path.parents):
        try:
            metadata = selected.lstat()
        except FileNotFoundError:
            continue
        if selected.is_symlink() or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("selected path passes through a link or reparse point")
        if stat.S_ISREG(metadata.st_mode) and metadata.st_nlink != 1:
            raise ValueError("selected file has multiple hard links")
    return path


def member(name: str) -> PurePosixPath:
    if not isinstance(name, str) or not name or "\\" in name or len(name) > 4096:
        raise ValueError("invalid payload member")
    path = PurePosixPath(name)
    if path.is_absolute() or path.as_posix() != name:
        raise ValueError("noncanonical payload member")
    for part in name.split("/"):
        if (
            part in ("", ".", "..")
            or part.endswith((" ", "."))
            or any(ord(c) < 32 or c in '<>:"|?*' for c in part)
        ):
            raise ValueError("unsafe payload member")
        if re.fullmatch(r"(?i)(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?", part):
            raise ValueError("Windows device payload member")
    return path


def manifest(data: bytes) -> dict:
    value = unique_json(data)
    if (
        value.get("schema_version") != 1
        or value.get("producer") != "noetome-windows-installer-payload"
        or value.get("target") != "x86_64-pc-windows-msvc"
        or value.get("build_profile") not in ("release", "debug")
    ):
        raise ValueError("unexpected installer payload manifest identity")
    if not re.fullmatch(r"[0-9a-f]{40}", value.get("source_commit", "")):
        raise ValueError("manifest source commit is missing")
    records = value.get("files")
    if not isinstance(records, list) or not 1 <= len(records) <= MAX_FILES:
        raise ValueError("payload inventory limit")
    names = set()
    total = 0
    for record in records:
        if not isinstance(record, dict) or set(record) != {
            "path",
            "bytes",
            "sha256",
            "source",
        }:
            raise ValueError("invalid manifest file DTO")
        member(record["path"])
        folded = record["path"].casefold()
        if folded in names or folded in {
            "payload-manifest.json",
            "payload-manifest.sha256",
        }:
            raise ValueError("duplicate or self-referencing payload path")
        names.add(folded)
        if (
            type(record["bytes"]) is not int
            or not 0 <= record["bytes"] <= MAX_BYTES
            or not re.fullmatch(r"[0-9a-f]{64}", record["sha256"])
            or not isinstance(record["source"], str)
        ):
            raise ValueError("invalid payload fingerprint")
        total += record["bytes"]
    if total > MAX_BYTES or value.get("totals") != {
        "files": len(records),
        "bytes": total,
    }:
        raise ValueError("payload totals do not match files")
    return value


def validate_stage_manifest(data: bytes, checksum: bytes, trusted_sha256: str) -> dict:
    if not re.fullmatch(r"[0-9a-f]{64}", trusted_sha256) or sha(data) != trusted_sha256:
        raise ValueError(
            "stage manifest differs from trusted preparation receipt SHA256"
        )
    if checksum != (trusted_sha256 + "\n").encode("ascii"):
        raise ValueError("stage manifest checksum mismatch")
    return manifest(data)


def snapshot(root: Path) -> dict:
    normal(root)
    result = {}
    todo = [root]
    while todo:
        directory = todo.pop()
        for path in directory.iterdir():
            normal(path)
            relative = path.relative_to(root).as_posix()
            if path.is_dir():
                result[relative + "/"] = {"kind": "directory"}
                todo.append(path)
            elif path.is_file():
                result[relative] = fingerprint(path)
            else:
                raise ValueError("nonordinary owned member")
            if len(result) > MAX_FILES * 2:
                raise ValueError("owned inventory limit")
    return result


def verify_payload(
    root: Path, value: dict, manifest_bytes: bytes, checksum_bytes: bytes
) -> dict:
    observed = snapshot(root)
    files = {name for name in observed if not name.endswith("/")}
    expected = {record["path"] for record in value["files"]}
    extras = {
        "noetome-desktop.exe",
        "uninstall.exe",
        "payload-manifest.json",
        "payload-manifest.sha256",
    }
    if files != expected | extras:
        raise ValueError(
            f"installed inventory mismatch: missing={sorted((expected | extras) - files)[:10]} extra={sorted(files - expected - extras)[:10]}"
        )
    for record in value["files"]:
        if observed[record["path"]] != {
            "bytes": record["bytes"],
            "sha256": record["sha256"],
        }:
            raise ValueError("installed file fingerprint mismatch: " + record["path"])
    if (root / "payload-manifest.json").read_bytes() != manifest_bytes or (
        root / "payload-manifest.sha256"
    ).read_bytes() != checksum_bytes:
        raise ValueError(
            "installed manifest differs from independently selected stage manifest"
        )
    return {
        "files": len(expected),
        "bytes": value["totals"]["bytes"],
        "manifest_sha256": sha(manifest_bytes),
        "inventory": observed,
    }


def pe_imports(data: bytes) -> tuple[int, list[str]]:
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError("payload executable has no DOS/PE header")
    offset = struct.unpack_from("<I", data, 60)[0]
    if offset + 24 > len(data) or data[offset : offset + 4] != b"PE\0\0":
        raise ValueError("invalid PE header")
    machine, sections = struct.unpack_from("<HH", data, offset + 4)
    size = struct.unpack_from("<H", data, offset + 20)[0]
    optional = offset + 24
    if optional + size > len(data) or size < 120 or not 1 <= sections <= 96:
        raise ValueError("invalid PE optional header")
    magic = struct.unpack_from("<H", data, optional)[0]
    directory = optional + (112 if magic == 0x20B else 96 if magic == 0x10B else 0)
    if directory == optional or directory + 16 > optional + size:
        raise ValueError("unsupported PE optional header")
    section_table = optional + size
    if section_table + sections * 40 > len(data):
        raise ValueError("truncated PE section table")

    def address(rva):
        for index in range(sections):
            base = section_table + index * 40
            virtual_size, virtual, raw_size, raw = struct.unpack_from(
                "<IIII", data, base + 8
            )
            if virtual <= rva < virtual + max(virtual_size, raw_size):
                relative = rva - virtual
                if relative >= raw_size or raw + relative >= len(data):
                    raise ValueError("PE RVA has no file bytes")
                return raw + relative
        raise ValueError("PE RVA is outside sections")

    rva = struct.unpack_from("<I", data, directory + 8)[0]
    imports = []
    if not rva:
        return machine, imports
    start = address(rva)
    for index in range(4096):
        pos = start + index * 20
        if pos + 20 > len(data):
            raise ValueError("truncated PE import descriptor")
        values = struct.unpack_from("<IIIII", data, pos)
        if not any(values):
            return machine, imports
        name = address(values[3])
        end = data.find(b"\0", name, min(len(data), name + 256))
        if end < 0:
            raise ValueError("unterminated PE dependency")
        imports.append(data[name:end].decode("ascii").lower())
    raise ValueError("PE import limit")


def verify_pe(root: Path, value: dict) -> list[dict]:
    paths = {record["path"].casefold(): record["path"] for record in value["files"]}
    records = {record["path"].casefold(): record for record in value["files"]}
    names = set(paths)
    if "vcruntime140.dll" not in names or "pdf-python/vcruntime140.dll" not in names:
        raise ValueError(
            "VC runtime 140 must be bundled beside the CLI and Python; System32 is insufficient"
        )
    result = []
    for name in sorted(names):
        if not name.endswith((".exe", ".dll", ".pyd")):
            continue
        data = (root / paths[name]).read_bytes()
        machine, dependencies = pe_imports(data)
        expected_machine = PIP_LAUNCHER_TEMPLATES.get(name, 0x8664)
        if name in PIP_LAUNCHER_TEMPLATES and (
            records[name]["bytes"] != len(data) or records[name]["sha256"] != sha(data)
        ):
            raise ValueError(
                "pip launcher template differs from selected stage bytes: " + name
            )
        if machine != expected_machine:
            raise ValueError("payload PE is not x64: " + name)
        for dependency in dependencies:
            if re.fullmatch(r"(?:vcruntime|msvcp|concrt)140[^/\\]*\.dll", dependency):
                local = str(PurePosixPath(name).parent / dependency)
                candidates = {local, dependency}
                if name.startswith("pdf-python/"):
                    candidates.add("pdf-python/" + dependency)
                if not candidates & names:
                    raise ValueError(
                        f"unbundled VC dependency {dependency} required by {name}"
                    )
        result.append(
            {
                "path": name,
                "machine": {0x8664: "amd64", 0x14C: "i386", 0xAA64: "arm64"}[machine],
                "role": "pip_launcher_template"
                if name in PIP_LAUNCHER_TEMPLATES
                else "runtime",
                "imports": dependencies,
            }
        )
    return result


def archive_installed(root: Path, output: Path, expected: dict) -> dict:
    if output.exists() or root == output or root in output.parents:
        raise ValueError(
            "installed payload archive must be new and outside installation"
        )
    if snapshot(root) != expected:
        raise ValueError("installed payload changed before archival")
    with zipfile.ZipFile(
        output, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6
    ) as archive:
        for name in sorted(expected):
            if name.endswith("/"):
                archive.writestr(name, b"")
            else:
                archive.write(root / name, arcname=name)
    with zipfile.ZipFile(output) as archive:
        if set(archive.namelist()) != set(expected) or len(archive.namelist()) != len(
            expected
        ):
            raise ValueError("installed archive inventory mismatch")
        for name, record in expected.items():
            data = archive.read(name)
            if name.endswith("/"):
                if data:
                    raise ValueError("installed archive directory is not empty")
            elif {"bytes": len(data), "sha256": sha(data)} != record:
                raise ValueError("installed archive bytes differ: " + name)
    if snapshot(root) != expected:
        raise ValueError("installed payload changed during archival")
    return {
        "path": str(output),
        **fingerprint(output),
        "members": len(expected),
        "installed_executable": fingerprint(root / "noetome-desktop.exe"),
    }


def registrations() -> list[dict]:
    import winreg

    found = []
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
            try:
                with winreg.OpenKey(
                    hive,
                    r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                    0,
                    winreg.KEY_READ | view,
                ) as key:
                    for index in range(winreg.QueryInfoKey(key)[0]):
                        name = winreg.EnumKey(key, index)
                        with winreg.OpenKey(key, name) as entry:
                            try:
                                display = winreg.QueryValueEx(entry, "DisplayName")[0]
                            except FileNotFoundError:
                                continue
                            if display == "Noetome":
                                facts = {"registry_key": name, "DisplayName": display}
                                for field in (
                                    "InstallLocation",
                                    "DisplayVersion",
                                    "UninstallString",
                                ):
                                    try:
                                        facts[field] = winreg.QueryValueEx(
                                            entry, field
                                        )[0]
                                    except FileNotFoundError:
                                        pass
                                found.append(facts)
            except FileNotFoundError:
                pass
    return found


def shortcut_paths() -> list[Path]:
    programs = Path(os.environ["APPDATA"]) / "Microsoft/Windows/Start Menu/Programs"
    shell = (
        Path(os.environ["SystemRoot"])
        / "System32/WindowsPowerShell/v1.0/powershell.exe"
    )
    result = subprocess.run(
        [
            str(shell),
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$OutputEncoding=[Console]::OutputEncoding=New-Object Text.UTF8Encoding($false); [Console]::Write([Environment]::GetFolderPath('Desktop'))",
        ],
        capture_output=True,
        timeout=20,
        check=True,
    )
    desktop = Path(result.stdout.decode("utf-8", "strict"))
    return [
        programs / "Noetome.lnk",
        programs / "Noetome/Noetome.lnk",
        desktop / "Noetome.lnk",
    ]


def clean_environment() -> dict:
    environment = {
        k: v
        for k, v in os.environ.items()
        if not k.upper().startswith("NOETOME_")
        and k.upper()
        not in {"PYTHONHOME", "PYTHONPATH", "VIRTUAL_ENV", "CARGO_HOME", "RUSTUP_HOME"}
    }
    environment["SystemRoot"] = os.environ["SystemRoot"]
    windows = Path(environment["SystemRoot"])
    environment["PATH"] = os.pathsep.join(
        str(p)
        for p in (
            windows / "System32",
            windows,
            windows / "System32/Wbem",
            windows / "System32/WindowsPowerShell/v1.0",
        )
    )
    return environment


def ensure_clean_install(
    install: Path,
    local: Path,
    shortcuts: list[Path],
    known: list[Path],
    records: list[dict],
):
    if local not in install.parents:
        raise ValueError("installation must remain within CI LOCALAPPDATA")
    if install.exists() or records:
        raise ValueError(
            "existing Noetome installation or registration; nothing was installed"
        )
    for selected in shortcuts + known:
        try:
            selected.lstat()
        except FileNotFoundError:
            continue
        raise ValueError(
            "existing Noetome directory or exact shortcut; nothing was installed"
        )


class Runner:
    def __init__(self, evidence: Path):
        self.evidence = evidence
        self.commands = []

    def run(
        self, name: str, command: list[str], environment: dict, expected=0, timeout=180
    ):
        start = time.monotonic()
        failure = None
        try:
            process = subprocess.run(
                command,
                check=False,
                capture_output=True,
                env=environment,
                timeout=timeout,
            )
            stdout, stderr, code, state = (
                process.stdout,
                process.stderr,
                process.returncode,
                "exited",
            )
        except subprocess.TimeoutExpired as error:
            stdout, stderr, code, state = (
                error.stdout or b"",
                error.stderr or b"",
                None,
                "timed_out",
            )
            failure = f"{name} timed out; raw partial output retained"
        except OSError as error:
            stdout, stderr, code, state = (
                b"",
                str(error).encode("utf-8"),
                None,
                "spawn_failed",
            )
            failure = f"{name} could not start; failure retained"
        (self.evidence / (name + ".stdout.log")).write_bytes(stdout)
        (self.evidence / (name + ".stderr.log")).write_bytes(stderr)
        receipt = {
            "name": name,
            "command": command,
            "state": state,
            "exit_code": code,
            "expected_exit_code": expected,
            "seconds": time.monotonic() - start,
            "stdout": fingerprint(self.evidence / (name + ".stdout.log")),
            "stderr": fingerprint(self.evidence / (name + ".stderr.log")),
        }
        self.commands.append(receipt)
        (self.evidence / (name + ".result.json")).write_text(
            json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
        )
        if failure:
            raise ValueError(failure)
        if code != expected:
            raise ValueError(
                f"{name} failed with actual exit {code}; raw output retained"
            )
        return stdout


def behavior(
    runner: Runner,
    install: Path,
    work: Path,
    environment: dict,
    pdf_auditor: Path | None,
):
    cli = str(install / "noetome.exe")
    project = work / "中文 空格资料项目"
    runner.run("project-create", [cli, "project", "init", str(project)], environment)
    runner.run(
        "project-inspect", [cli, "project", "inspect", str(project)], environment
    )
    expected = unique_json(
        (install / "fixtures/pdf-text-v1/expected.json").read_bytes()
    )
    cases = {case["file"]: case for case in expected["cases"]}
    facts = []
    for stem in ("single-page", "scanned-page"):
        source = work / ("原件 空格 " + stem + ".pdf")
        source.write_bytes((install / f"fixtures/pdf-text-v1/{stem}.pdf").read_bytes())
        want = cases[stem + ".pdf"]
        if fingerprint(source) != {
            "bytes": want["byte_length"],
            "sha256": want["raw_sha256"],
        }:
            raise ValueError("installed PDF differs from frozen author source")
        imported = unique_json(
            runner.run(
                stem + "-import",
                [cli, "source", "import", str(project), str(source)],
                environment,
                expected=3 if want["status"] == "needs_review" else 0,
            )
        )
        if (
            imported["pdf"]["status"] != want["status"]
            or imported["pdf"]["page_count"] != 1
            or imported["knowledge_status"] != "pending"
        ):
            raise ValueError(
                "installed PDF source did not produce its actual expected status"
            )
        choices = unique_json(
            runner.run(
                stem + "-choices",
                [cli, "preview", "choices", str(project), imported["import_id"]],
                environment,
            )
        )
        selectable = [item for item in choices["choices"] if item["selectable"]]
        if stem == "single-page":
            if not selectable:
                raise ValueError("confirmed real PDF text did not expose any selection")
            selection = work / "真实选择.json"
            selection.write_text(
                json.dumps([item["selection"] for item in selectable]), encoding="utf-8"
            )
            units = unique_json(
                runner.run(
                    "pdf-real-units",
                    [
                        cli,
                        "preview",
                        "units",
                        str(project),
                        imported["import_id"],
                        str(selection),
                    ],
                    environment,
                )
            )
            actual = [
                unit["text"]
                for unit in sorted(units["units"], key=lambda unit: unit["start_byte"])
            ]
            wanted = [line["text"] for line in want["pages"][0]["lines"]]
            if actual != wanted or any(
                unit["locator"]["page_index"] != 0 for unit in units["units"]
            ):
                raise ValueError(
                    "installed PDF actual text or physical page locator mismatch"
                )
        elif selectable:
            raise ValueError("scanned page was fabricated into selectable text")
        else:
            invented = work / "扫描 不存在区域.json"
            invented.write_text(
                json.dumps(
                    [
                        {
                            "kind": "pdf",
                            "page_index": 0,
                            "region_id": "invented-whole-page",
                        }
                    ]
                ),
                encoding="utf-8",
            )
            runner.run(
                "scan-invented-selection-refused",
                [
                    cli,
                    "preview",
                    "units",
                    str(project),
                    imported["import_id"],
                    str(invented),
                ],
                environment,
                expected=2,
            )
        facts.append(
            {
                "source": fingerprint(source),
                "import_id": imported["import_id"],
                "status": imported["pdf"]["status"],
                "selectable_units": len(selectable),
            }
        )
    text = work / "用户 自造说明.txt"
    text.write_text("这是安装后的自造文本实测材料。\n", encoding="utf-8", newline="\n")
    imported = unique_json(
        runner.run(
            "text-import",
            [cli, "source", "import", str(project), str(text)],
            environment,
        )
    )
    if imported["status"] == "failed" or imported["knowledge_status"] != "pending":
        raise ValueError("installed text import failed")
    fixture = install / "fixtures/run-checkpoint-v1/fixture"
    offline = work / "离线 样例运行"
    run = unique_json(
        runner.run(
            "offline-run",
            [cli, "run", "start", str(fixture), str(offline)],
            environment,
        )
    )
    if (
        run["producer"] != "fixture_run"
        or run["run_status"] != "succeeded"
        or run["scientific_quality"] != "not_run"
        or run["real_model_calls"] != 0
        or run["paid_calls"] != 0
    ):
        raise ValueError("offline run did not complete with explicit fixture identity")
    vault = offline / "output/vault"
    want = snapshot(install / "fixtures/run-checkpoint-v1/expected-markdown")
    if snapshot(vault) != want:
        raise ValueError(
            "installed fixture Vault differs from frozen author Markdown bytes"
        )
    shell = str(
        Path(environment["SystemRoot"])
        / "System32/WindowsPowerShell/v1.0/powershell.exe"
    )
    profile = str(install / "helpers/text_model_profile.ps1")
    missing_key, ledger = (
        work / "never-configured.dpapi",
        work / "test cumulative ledger.json",
    )
    status = unique_json(
        runner.run(
            "model-missing",
            [
                shell,
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                profile,
                "-Mode",
                "Status",
                "-CredentialFile",
                str(missing_key),
                "-UsageLedgerFile",
                str(ledger),
            ],
            environment,
        )
    )
    if (
        status["ready"]
        or status["credential_state"] != "missing"
        or status["ledger_state"] != "missing"
        or ledger.exists()
        or missing_key.exists()
    ):
        raise ValueError("missing installed model configuration was not refused")
    baseline = install / "fixtures/model-ledger/actual-final-ledger.json"
    proof = install / "fixtures/model-ledger/continuation-current-ledger.json"
    status = unique_json(
        runner.run(
            "model-explicit-history",
            [
                shell,
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                profile,
                "-Mode",
                "RestoreLedger",
                "-Confirmed",
                "-CredentialFile",
                str(missing_key),
                "-UsageLedgerFile",
                str(ledger),
                "-RecoveryLedger",
                str(baseline),
                "-RecoveryProof",
                str(proof),
            ],
            environment,
        )
    )
    if (
        status["ready"]
        or status["ledger_state"] != "ready"
        or status["accounted_tokens"] != 33185
        or status["model_attempts"] != 10
        or ledger.read_bytes() != baseline.read_bytes()
    ):
        raise ValueError("installed frozen cumulative history restoration mismatch")
    if pdf_auditor:
        python = str(install / "pdf-python/python.exe")
        runner.run(
            "independent-pdf-matrix",
            [
                python,
                "-I",
                "-B",
                "-X",
                "utf8",
                str(pdf_auditor),
                str(work / "独立 PDF矩阵"),
                "--binary",
                cli,
            ],
            environment,
            timeout=900,
        )
    return {
        "pdfs": facts,
        "text_import_id": imported["import_id"],
        "offline_vault": str(vault),
        "offline_files": want,
        "model_status": status,
        "independent_pdf_matrix": "passed" if pdf_auditor else "not_run",
        "external_model_calls": 0,
        "native_ui": "not_run",
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--installer", type=Path, required=True)
    parser.add_argument("--stage-manifest", type=Path, required=True)
    parser.add_argument(
        "--manifest-sha256",
        required=True,
        help="Trusted SHA256 from the preparation step receipt, outside the package",
    )
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--install-dir", type=Path, required=True)
    parser.add_argument("--pdf-auditor", type=Path)
    args = parser.parse_args(argv)
    if (
        os.name != "nt"
        or os.environ.get("GITHUB_ACTIONS", "").lower() != "true"
        or os.environ.get("CI", "").lower() != "true"
    ):
        raise SystemExit(
            "This installation/uninstall smoke runs only on the clean Windows GitHub CI host."
        )
    evidence, install = (
        normal(args.evidence_dir.absolute()),
        normal(args.install_dir.absolute()),
    )
    if (
        evidence.exists()
        or install.exists()
        or evidence in install.parents
        or install in evidence.parents
    ):
        raise SystemExit(
            "Evidence and installation roots must be new and separate; existing files are preserved."
        )
    evidence.mkdir()
    runner = Runner(evidence)
    summary = {
        "status": "failed",
        "native_ui": "not_run",
        "external_model_calls": 0,
        "uninstall": "not_run",
        "source_commit": os.environ.get("GITHUB_SHA"),
        "installer": str(args.installer),
        "install_dir": str(install),
        "host": {"platform": sys.platform, "python": sys.version},
        "source_inputs": {},
    }
    owned = False
    before = None
    shortcuts = []
    uninstaller_pin = None
    environment = None
    work = evidence / "selected-tests"
    try:
        environment = clean_environment()
        work.mkdir()
        summary["host"]["platform"] = platform.platform()
        summary["source_inputs"] = {
            name: fingerprint(ROOT / name)
            for name in (
                "scripts/verify_windows_installation.py",
                "scripts/install_desktop.ps1",
                "scripts/prepare_desktop_installer.ps1",
            )
        }
        local = normal(Path(os.environ["LOCALAPPDATA"]).absolute())
        shortcuts = shortcut_paths()
        known = [local / "Programs/Noetome", local / "Noetome"] + [
            Path(os.environ[name]) / "Noetome"
            for name in ("ProgramFiles", "ProgramFiles(x86)")
            if name in os.environ
        ]
        ensure_clean_install(install, local, shortcuts, known, registrations())
        stage = normal(args.stage_manifest.absolute())
        manifest_bytes = stage.read_bytes()
        checksum_bytes = stage.with_name("payload-manifest.sha256").read_bytes()
        value = validate_stage_manifest(
            manifest_bytes, checksum_bytes, args.manifest_sha256
        )
        if (
            os.environ.get("GITHUB_SHA")
            and value["source_commit"] != os.environ["GITHUB_SHA"]
        ):
            raise ValueError("stage source does not match CI source commit")
        installer_pin = fingerprint(normal(args.installer.absolute()))
        summary.update(
            installer_fingerprint=installer_pin,
            stage_manifest_sha256=sha(manifest_bytes),
            trusted_preparation_manifest_sha256=args.manifest_sha256,
            source_commit=value["source_commit"],
            child_path=environment["PATH"],
            no_pdf_python_override="NOETOME_PDF_PYTHON" not in environment,
        )
        shell = str(
            Path(environment["SystemRoot"])
            / "System32/WindowsPowerShell/v1.0/powershell.exe"
        )
        helper_receipt = ROOT / ".artifacts/desktop-install.json"
        if helper_receipt.exists():
            (evidence / "legacy-install.before.json").write_bytes(
                helper_receipt.read_bytes()
            )
        try:
            runner.run(
                "install-helper",
                [
                    shell,
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(ROOT / "scripts/install_desktop.ps1"),
                    "-InstallerPath",
                    str(args.installer.absolute()),
                    "-InstallDir",
                    str(install),
                    "-OutputDir",
                    str(evidence / "prepared-installer"),
                ],
                os.environ.copy(),
                timeout=600,
            )
        finally:
            if helper_receipt.exists():
                (evidence / "legacy-install.raw.json").write_bytes(
                    helper_receipt.read_bytes()
                )
        legacy = unique_json(helper_receipt.read_bytes())
        if (
            not all(
                legacy.get(key)
                for key in (
                    "matches_current_build",
                    "build_unchanged",
                    "installer_unchanged",
                    "has_matching_shortcut",
                    "has_matching_uninstall_record",
                )
            )
            or legacy["installer_exit_code"] != 0
            or Path(legacy["installed_executable"]) != install / "noetome-desktop.exe"
        ):
            raise ValueError("legacy install identity proof failed")
        owned = True
        uninstaller_pin = fingerprint(install / "uninstall.exe")
        summary["installation"] = legacy
        summary["payload"] = verify_payload(
            install, value, manifest_bytes, checksum_bytes
        )
        summary["pe_dependencies"] = verify_pe(install, value)
        (work / "manual-sentinel.md").write_text(
            "这份人工测试文件必须在卸载后逐字节保留。\n", encoding="utf-8", newline="\n"
        )
        summary["behaviors"] = behavior(
            runner, install, work, environment, args.pdf_auditor
        )
        summary["installed_payload_archive"] = archive_installed(
            install, evidence / "installed-payload.zip", summary["payload"]["inventory"]
        )
        summary["installed_payload_archive"]["source_commit"] = value["source_commit"]
        if (
            fingerprint(args.installer.absolute()) != installer_pin
            or stage.read_bytes() != manifest_bytes
        ):
            raise ValueError("installer or stage changed during smoke")
        before = snapshot(work)
        summary["status"] = "passed"
    except Exception as error:  # noqa: BLE001 - retain every failed CI smoke and continue owned cleanup.
        summary["failure"] = str(error)
    finally:
        if owned:
            try:
                if uninstaller_pin != fingerprint(install / "uninstall.exe"):
                    raise ValueError("owned uninstaller changed; preserved the site")
                if before is None:
                    before = snapshot(work)
                runner.run(
                    "uninstall",
                    [str(install / "uninstall.exe"), "/S"],
                    environment,
                    timeout=180,
                )
                deadline = time.monotonic() + 60
                while time.monotonic() < deadline and (
                    install.exists()
                    or registrations()
                    or any(path.exists() for path in shortcuts)
                ):
                    time.sleep(0.2)
                if (
                    install.exists()
                    or registrations()
                    or any(path.exists() for path in shortcuts)
                ):
                    raise ValueError(
                        "owned uninstall left its installation, registration or exact shortcut"
                    )
                if snapshot(work) != before:
                    raise ValueError(
                        "uninstall modified selected project, Vault or manual sentinel"
                    )
                summary["uninstall"] = "passed"
                summary["selected_tests_unchanged"] = True
            except Exception as error:  # noqa: BLE001 - cleanup failures must remain failed evidence.
                summary["status"] = "failed"
                summary["uninstall"] = "failed"
                summary["uninstall_failure"] = str(error)
        summary["commands"] = runner.commands
        (evidence / "summary.json").write_text(
            json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
    print(
        json.dumps(
            {
                "status": summary["status"],
                "uninstall": summary["uninstall"],
                "evidence": str(evidence),
            },
            ensure_ascii=False,
        )
    )
    return (
        0 if summary["status"] == "passed" and summary["uninstall"] == "passed" else 1
    )


if __name__ == "__main__":
    sys.exit(main())
