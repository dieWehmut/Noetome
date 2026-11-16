"""Synthetic PE/runtime negative vectors; native staging proves the actual runtime separately."""

from __future__ import annotations
import copy
import hashlib
import json
import os
from pathlib import Path
import struct
import tempfile
import unittest

from scripts import verify_windows_installer_payload as audit_module

ROOT = Path(__file__).resolve().parents[1]


class WindowsInstallerPayloadTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="noetome-payload-audit-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / "payload"
        self.root.mkdir()
        for name, source in audit_module.repository_resources(ROOT).items():
            self.write(name, source.read_bytes())
        pe = bytearray(128)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 64)
        pe[64:68] = b"PE\0\0"
        struct.pack_into("<H", pe, 68, 0x8664)
        for name in (
            "noetome.exe",
            "vcruntime140.dll",
            "pdf-python/python.exe",
            "pdf-python/python312.dll",
            "pdf-python/vcruntime140.dll",
        ):
            self.write(name, pe)
        for name in (
            "README.md",
            "pdf-python/LICENSE.txt",
            "pdf-python/Lib/os.py",
            "pdf-python/Lib/site.py",
        ):
            self.write(name, b"public synthetic audit vector only\n")
        lock = json.loads((self.root / "runtime-lock.json").read_bytes())
        licenses = {}
        for name in lock["packages"]:
            path = name.replace(".", "_") + ".dist-info/licenses/LICENSE"
            licenses[name] = [path]
            self.write(
                "pdf-python/Lib/site-packages/" + path,
                b"public synthetic license vector\n",
            )
        receipt = {
            "schema_version": 1,
            "producer": "noetome-locked-windows-pdf-runtime",
            "archive_sha256": audit_module.ARCHIVE_SHA,
            "python_executable_sha256": self.sha(self.root / "pdf-python/python.exe"),
            "requirements_sha256": self.sha(self.root / "requirements-lock.txt"),
            "identity": {
                "python_version": "3.12.14",
                "platform": "win32",
                "architecture": "AMD64",
                "packages": lock["packages"],
                "runtime_lock_sha256": self.sha(self.root / "runtime-lock.json"),
                "licenses": licenses,
            },
        }
        self.write("runtime-receipt.json", json.dumps(receipt).encode())
        self.regenerate()

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def sha(self, path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def regenerate(self):
        rows = [
            {
                "path": p.relative_to(self.root).as_posix(),
                "bytes": p.stat().st_size,
                "sha256": self.sha(p),
                "source": "public-synthetic-vector",
            }
            for p in self.root.rglob("*")
            if p.is_file()
            and p.name not in (audit_module.MANIFEST, audit_module.CHECKSUM)
        ]
        rows.sort(key=lambda row: row["path"])
        self.manifest = {
            "schema_version": 1,
            "producer": audit_module.PRODUCER,
            "source_commit": "a" * 40,
            "target": "x86_64-pc-windows-msvc",
            "build_profile": "debug",
            "version": json.loads((ROOT / "crates/noetome-desktop/tauri.conf.json").read_text(encoding="utf-8"))["version"],
            "source_inputs": {
                "cli": {
                    "bytes": (self.root / "noetome.exe").stat().st_size,
                    "sha256": self.sha(self.root / "noetome.exe"),
                },
                "runtime_receipt": {
                    "sha256": self.sha(self.root / "runtime-receipt.json")
                },
            },
            "totals": {"files": len(rows), "bytes": sum(row["bytes"] for row in rows)},
            "files": rows,
        }
        self.save_manifest()

    def save_manifest(self):
        self.write(
            audit_module.MANIFEST,
            (json.dumps(self.manifest, sort_keys=True, indent=2) + "\n").encode(),
        )
        self.write(
            audit_module.CHECKSUM,
            (self.sha(self.root / audit_module.MANIFEST) + "\n").encode(),
        )

    def result(self):
        return audit_module.audit(self.root)

    def failed(self):
        self.assertGreater(self.result()["failures"], 0)

    def test_complete_synthetic_payload_static_audit_passes(self):
        self.assertEqual(self.result()["failures"], 0, self.result())

    def test_missing_runtime_module_is_a_real_inventory_failure(self):
        (self.root / "pdf-python/Lib/site.py").unlink()
        self.failed()

    def test_tampered_cli_and_wrong_manifest_digest_are_rejected(self):
        self.write("noetome.exe", b"tampered")
        self.failed()

    def test_deleted_license_cannot_be_hidden_by_recomputed_manifest(self):
        (next((self.root / "pdf-python/Lib/site-packages").rglob("LICENSE"))).unlink()
        self.regenerate()
        self.failed()

    def test_changed_frozen_history_cannot_be_hidden_by_new_hashes(self):
        path = self.root / "fixtures/model-ledger/actual-final-ledger.json"
        data = json.loads(path.read_bytes())
        data["accounted_tokens"] = 0
        path.write_text(json.dumps(data))
        self.regenerate()
        self.failed()

    def test_wrong_package_version_with_matching_manifest_is_rejected(self):
        p = self.root / "runtime-receipt.json"
        d = json.loads(p.read_bytes())
        d["identity"]["packages"]["pdfplumber"] = "0.0.0"
        p.write_text(json.dumps(d))
        self.regenerate()
        self.failed()

    def test_vc_runtime_must_match_packaged_python_copy(self):
        self.write("vcruntime140.dll", b"wrong VC bytes")
        self.regenerate()
        self.failed()

    def test_duplicate_and_traversal_manifest_rows_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for value in ("../outside", "a\\b", "C:/outside", "./inside"):
            self.manifest = copy.deepcopy(original)
            self.manifest["files"][0]["path"] = value
            self.save_manifest()
            self.failed()
        self.manifest = original
        self.manifest["files"].append(copy.deepcopy(self.manifest["files"][0]))
        self.save_manifest()
        self.failed()

    def test_non_x64_pe_cannot_be_hidden_by_manifest_hashes(self):
        p = self.root / "noetome.exe"
        data = bytearray(p.read_bytes())
        struct.pack_into("<H", data, 68, 0x14C)
        p.write_bytes(data)
        self.regenerate()
        self.failed()

    def test_external_manifest_anchor_catches_complete_regeneration(self):
        expected = self.sha(self.root / audit_module.MANIFEST)
        self.write("README.md", b"changed")
        self.regenerate()
        result = audit_module.audit(self.root, manifest_sha256=expected)
        self.assertGreater(result["failures"], 0)

    def test_hardlink_and_symlink_are_rejected_without_following(self):
        path = self.root / "README.md"
        os.link(path, self.root / "other-link")
        self.failed()
        (self.root / "other-link").unlink()
        try:
            (self.root / "directory-link").symlink_to(
                self.root / "pdf-python", target_is_directory=True
            )
        except OSError:
            self.skipTest("Symlink creation unavailable on this host")
        self.failed()

    def test_an_extra_credential_file_cannot_become_a_declared_installer_resource(self):
        self.write("credentials/private.json", b"public fake sensitive file")
        self.regenerate()
        self.failed()

    def test_opt_in_configuration_only_overrides_bundle_and_preserves_directory_tree(
        self,
    ):
        config = json.loads(
            (ROOT / "crates/noetome-desktop/tauri.install.conf.json").read_bytes()
        )
        self.assertEqual(set(config), {"$schema", "bundle"})
        self.assertEqual(
            config["bundle"]["resources"],
            {"../../target/windows-installer-payload/": ""},
        )
        self.assertEqual(
            config["bundle"]["windows"]["webviewInstallMode"],
            {"type": "offlineInstaller", "silent": True},
        )
        self.assertFalse(
            (ROOT / "crates/noetome-desktop/tauri.windows.conf.json").exists()
        )
