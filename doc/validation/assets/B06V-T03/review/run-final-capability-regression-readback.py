"""Read fixed final evidence; no app, CLI, model or verifier mutation is run."""
import contextlib
import hashlib
import json
import re
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[5]
ASSETS = ROOT / "doc/validation/assets/B06V-T03"


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def load(path):
    return json.loads(path.read_bytes())


def archive(stack, base, manifest_name):
    manifest = load(base / manifest_name)
    path = base / manifest["archive"]
    assert pin(path.read_bytes()) == manifest["archive_pin"], path
    z = stack.enter_context(zipfile.ZipFile(path))
    names = z.namelist()
    assert len(names) == len(set(names)) == manifest["members"]
    assert set(names) == set(manifest["files"])
    assert z.testzip() is None
    for name, expected in manifest["files"].items():
        assert ".." not in Path(name).parts and not name.startswith("/")
        assert pin(z.read(name)) == {k: expected[k] for k in ("sha256", "byte_length")}, name
    return z, {"archive": str(path.relative_to(ROOT)), "pin": manifest["archive_pin"], "crc_sha_length_members": len(names)}


def current_cli(stack):
    base = ASSETS / "review/current-final-pe"
    source, source_audit = archive(stack, base, "source-originals-manifest.json")
    native, native_audit = archive(stack, base, "native-originals-manifest.json")
    checks = load(base / "actual-checks.json")
    assert native.read("final02/actual-checks.json") == (base / "actual-checks.json").read_bytes()
    before = load(base / "source-before.json")
    assert before == load(base / "source-after.json")
    for name, expected in before.items():
        assert pin(source.read("runtime-repo/" + name)) == expected, name
        if name.startswith(("scripts/", "tests/", "fixtures/", "schemas/", "crates/")) and "__pycache__" not in Path(name).parts:
            assert pin((ROOT / name).read_bytes()) == expected, name
    dependencies = load(base / "rust-dependency-subset.json")["files"]
    snapshot = load(base / "snapshot.json")["files"]
    originals = load(base / "complete-compiled-source-reconstruction.json")["compiler_originals"]
    for name, expected in snapshot.items():
        member = ("compiled-source-variants/" if name in originals else "runtime-repo/") + name
        assert pin(source.read(member)) == expected, name
    for name, expected in dependencies.items():
        assert pin(source.read("runtime-repo/" + name)) == expected, name
        assert pin((ROOT / name).read_bytes()) == expected, name
    assert len(snapshot) == 1915 and len(dependencies) == 1825 and len(before) == 1928
    authority = load(base / "build-source-binding.json")
    for name, expected in authority["current_independent_python_and_collectors"].items():
        assert before[name] == expected == pin((ROOT / name).read_bytes()), name
    assert pin((base / "build-source-binding.json").read_bytes()) == checks["build_source_authority"]
    assert authority["actual_build_receipt"] == load(base / "cli-build-attempt-1.json")
    assert authority["actual_build_receipt"]["exit_code"] == 0
    assert authority["actual_build_receipt"]["source_before_after_equal"]
    assert authority["actual_build_receipt"]["snapshot_fingerprint"] == pin((base / "snapshot.json").read_bytes())
    rust_native = stack.enter_context(zipfile.ZipFile(ASSETS / "windows-rust/native-originals.zip"))
    rust_source = stack.enter_context(zipfile.ZipFile(ASSETS / "windows-rust/source-originals.zip"))
    for name in ("native", "source"):
        assert pin((ASSETS / ("windows-rust/" + name + "-originals.zip")).read_bytes()) == authority[name + "_archive"]
    for name, expected in snapshot.items():
        assert pin(rust_source.read("c/repo/" + name)) == expected, name
    pe = rust_native.read("c/noetome-selected.exe")
    assert pin(pe) == checks["selected_pe"] == authority["selected_pe"]
    assert source.read("runtime-repo/target/debug/noetome.exe") == pe
    assert rust_native.read("c/cli-build-attempt-1.json") == (base / "cli-build-attempt-1.json").read_bytes()
    for stream in ("stdout", "stderr"):
        raw = source.read("authority/cli-build-attempt-1." + stream + ".bin")
        expected = authority["actual_build_receipt"][stream]
        assert pin(raw) == {k: expected[k] for k in ("sha256", "byte_length")}
    command_count = 0
    for kind, directory in (("primary_commands", "commands"), ("additional_setup_commands", "setup-commands")):
        for index, command in enumerate(checks[kind]):
            stem = "final02/" + directory + f"/{index:02d}-" + command["label"]
            assert json.loads(native.read(stem + ".receipt.json")) == command
            assert command["exit_code"] in (0, 3)
            assert not command["inherited_real_credential_environment"]
            for stream in ("stdout", "stderr"):
                assert pin(native.read(stem + "." + stream + ".bin")) == command[stream]
            command_count += 1
    file_count = markdown_count = 0
    for binding in checks["display_bindings"]:
        cid, domain = binding["case_id"], binding["domain"]
        pack = "visual-compatibility-flow-extensions" if cid in ("flow-complex-labels", "flow-promotes", "flow-inhibits") else "visual-compatibility"
        output = "final02/scan/" + cid + "/Vault/" if domain == "scan" else "final02/cases/" + cid + "/"
        for entry in binding["files"]:
            raw = native.read(output + entry["file"])
            frozen = source.read("runtime-repo/fixtures/" + pack + "/selected-output/" + cid + "/" + entry["file"])
            assert raw == frozen
            assert pin(raw) == entry["current"] == entry["captured_frozen_original"]
            file_count += 1
            markdown_count += entry["file"].endswith(".md")
    for rejection in checks["synchronized_forgery_rejections"] + checks["preliminary_schema_rejections"]:
        prefix = "final02/mutations/" + rejection["name"] + "/"
        for name, expected in rejection["files"].items():
            assert pin(native.read(prefix + name)) == expected
    substantive = checks["synchronized_forgery_rejections"]
    assert len(substantive) == 7 and len(checks["preliminary_schema_rejections"]) == 2
    assert all("schema fields" not in rejection["reason"] for rejection in substantive)
    assert all("source-grounded" in rejection["reason"] for rejection in substantive if rejection["name"].endswith("-semantic"))
    for cid in ("scan-clear", "scan-fallback", "scan-sign", "scan-subscript"):
        transport = json.loads(native.read("final02/scan/" + cid + "/transport.json"))
        assert transport["network_attempts"] == 1 and transport["http_status"] == 200
        assert native.read("final02/scan/" + cid + "/actual-http-request.bin")
        assert native.read("final02/scan/" + cid + "/Vault/response.bin") == source.read("runtime-repo/fixtures/visual-compatibility/selected-output/" + cid + "/response.bin")
    assert command_count == 45 and len(checks["rows"]) == 27 and markdown_count == 16
    return {"status": "passed", "archives": [source_audit, native_audit], "result_pin": pin((base / "actual-checks.json").read_bytes()), "runtime_source_files": len(before), "current_independent_source_files_equal": True, "generated_python_cache_scope": "all archived original cache bytes verified against the frozen runtime input; warmed workspace __pycache__ is excluded from current source equality", "compiled_source_files": len(snapshot), "compiled_rust_dependencies": len(dependencies), "actual_cli_calls": command_count, "primary_calls": 30, "additional_setup_calls": 15, "domain_rows": len(checks["rows"]), "bound_cases": len(checks["display_bindings"]), "bound_files": file_count, "bound_markdown": markdown_count, "substantive_rejections": [{k: rejection[k] for k in ("name", "reason")} for rejection in substantive], "preliminary_structure_rejections": 2, "synthetic_post_calls": 4, "scan_original_responses_byte_equal": 4, "actual_pe": pin(pe), "old_elf_build_source_gap": "closed for the final replacement checks; old records and all harness failures retained"}


def linux_python(stack):
    base = ASSETS / "linux-final-after-portable-test"
    source, source_audit = archive(stack, base, "source-originals-manifest.json")
    native, native_audit = archive(stack, base, "final-run-originals-manifest.json")
    before = json.loads(source.read("metadata/source-before.json"))
    assert before == json.loads(native.read("round-04/source-before.json")) == json.loads(native.read("round-04/source-after.json"))
    assert native.read("round-04/worktree-related-before.json") == native.read("round-04/worktree-related-after.json")
    current_differences = []
    for name, expected in before.items():
        raw = source.read("repo/" + name)
        assert pin(raw) == expected, name
        if not name.startswith("target/") and pin((ROOT / name).read_bytes()) != expected:
            current_differences.append(name)
    assert all(name.startswith("doc/") or name == "README.md" for name in current_differences), current_differences
    receipt = json.loads(native.read("round-04/runs/full-python/receipt.json"))
    for stream in ("stdout", "stderr"):
        assert pin(native.read("round-04/runs/full-python/" + stream + ".bin")) == receipt[stream]
    stderr = native.read("round-04/runs/full-python/stderr.bin").decode("utf-8")
    ok = re.findall(r"^test_.* \.\.\. ok$", stderr, re.M)
    skips = re.findall(r"^test_.* \.\.\. skipped .*$", stderr, re.M)
    assert receipt["exit_code"] == 0 and receipt["tests_run"] == 380 and len(ok) == 361 and len(skips) == 19
    assert "Ran 380 tests in 175.940s" in stderr and "OK (skipped=19)" in stderr
    assert source.read("repo/scripts/prepare_visual_compatibility.py") == (ROOT / "scripts/prepare_visual_compatibility.py").read_bytes()
    assert source.read("repo/tests/test_verify_process_flow.py") == (ROOT / "tests/test_verify_process_flow.py").read_bytes()
    return {"status": "passed", "archives": [source_audit, native_audit], "source_files": len(before), "source_before_after_equal": True, "worktree_related_before_after_equal": True, "actual_run": 380, "actual_ok_lines": len(ok), "actual_skip_lines": len(skips), "actual_exit_code": receipt["exit_code"], "current_document_only_differences": current_differences, "final_prepare_pin": before["scripts/prepare_visual_compatibility.py"], "final_flow_test_pin": before["tests/test_verify_process_flow.py"], "elf_alias_pins": {name: before[name] for name in before if name.startswith("target/debug/")}, "full_linux_rust": "not_run"}


with contextlib.ExitStack() as stack:
    result = {"schema_version": 1, "status": "passed", "scope": "Independent complete fixed ZIP/member/raw-stream/build-source/display-byte and final test-source readback; no subprocess CLI, app, model or mutation executed by this auditor", "current_capability": current_cli(stack), "linux_final_python": linux_python(stack), "windows_final_python": "final-windows-python-source-readback.json", "final_native64": "final-native-format-overrides-audit.json", "models_requested": 0, "scientific_quality": "not_run", "published": False}
    output = ASSETS / "review/final-capability-regression-readback.json"
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "report": pin(output.read_bytes()), "cli": result["current_capability"]["actual_cli_calls"], "linux": result["linux_final_python"]["actual_run"]}))
