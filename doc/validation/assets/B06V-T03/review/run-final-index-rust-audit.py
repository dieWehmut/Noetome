"""Read the final native index and frozen Windows Rust originals independently."""
from pathlib import Path
import hashlib
import json
import re
import zipfile

REPO = Path(__file__).resolve().parents[5]
HERE = Path(__file__).resolve().parent
BASE = HERE.parent


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def fixed(value):
    return {key: value[key] for key in ("sha256", "byte_length")}


matrix_path = BASE / "matrix.json"
matrix_before = matrix_path.read_bytes()
assert pin(matrix_before) == {"sha256": "8934eeef639665d0dc7c2e652dc1590d38fe452a21d4c1e0bae59d0c03bc294d", "byte_length": 254957}
matrix = json.loads(matrix_before)
prior_path = HERE / "final-native-format-overrides-audit.json"
prior = json.loads(prior_path.read_bytes())
accepted = {(row["role"], row["case_id"], row["theme"]): row for row in prior["final_records"]}
assert matrix["native_combination_count"] == len(matrix["records"]) == len(accepted) == 64
assert matrix["case_count"] == 16 and matrix["product_case_count"] == 15 and matrix["hand_authored_probe_count"] == 1
assert matrix["scientific_quality"] == "not_run" and matrix["published"] is False and matrix["real_model_calls"] == 0
archives = {}
checked = []
for row in matrix["records"]:
    key = row["role"], row["case_id"], row["theme"]
    assert key in accepted
    assert row["result_pin"] == accepted[key]["result_pin"] and row["result_member"] == accepted[key]["result_member"]
    archive_path = BASE / row["archive"]
    if str(archive_path) not in archives:
        archives[str(archive_path)] = zipfile.ZipFile(archive_path)
    z = archives[str(archive_path)]
    raw = z.read(row["result_member"])
    assert pin(raw) == row["result_pin"]
    result = json.loads(raw)
    actual_input = json.loads(z.read(row["input_member"]))
    cleanup = json.loads(z.read(row["cleanup_member"]))
    assert row["actual_identity"] == result["identity"]
    assert row["actual_installation"] == result["compatibilityIdentity"]
    assert row["actual_runtime_modules"] == result["compatibilityRuntime"]["modules"]
    assert row["source_files_checked"] == len(actual_input["source_pins"])
    assert row["runtime_sha256"] == ("f7f74bbfcd029d6a9effca499fffd4e380b0b17877fcb17c448f7c573fac60cd" if row["role"] == "current" else "88e00bc3481311959e6e13f0fdaf57cf16f13cae937058d541f1eb4a46dabfa5")
    expectation = (REPO / row["expected"]).read_bytes()
    assert pin(expectation) == row["expected_pin"]
    assert pin((REPO / row["fixture"] / "frozen-sha256.json").read_bytes())["sha256"] == row["fixture_sha256"]
    assert cleanup["profileProcessCount"] == 0 and cleanup["ownedPidAbsent"] and cleanup["ownedListenerAbsent"] and cleanup["emergencyTerminated"] == []
    checked.append({"role": key[0], "case_id": key[1], "theme": key[2], "archive": row["archive"], "result_member": row["result_member"], "result_pin": row["result_pin"], "expectation_pin": row["expected_pin"]})
assert len({(r["role"], r["case_id"], r["theme"]) for r in checked}) == 64
assert sum(r["representative_kind"] == "product_output" for r in matrix["records"]) == 60
assert sum(r["representative_kind"] == "hand_authored_format_probe" for r in matrix["records"]) == 4
assert matrix_path.read_bytes() == matrix_before

RUST = BASE / "windows-rust"
rust_archives = {}
for kind in ("native", "source"):
    path = RUST / (kind + "-originals.zip")
    declared = json.loads((RUST / (kind + "-originals-manifest.json")).read_bytes())
    assert pin(path.read_bytes()) == fixed(declared)
    z = zipfile.ZipFile(path)
    assert z.testzip() is None and set(z.namelist()) == set(declared["members"])
    assert all(pin(z.read(name)) == fixed(value) for name, value in declared["members"].items())
    rust_archives[kind] = z
native, source = rust_archives["native"], rust_archives["source"]
final = json.loads(native.read("c/native-rust-final.json"))
subset = json.loads(native.read("c/rust-dependency-subset.json"))["files"]
assert len(subset) == 1825
for name, value in subset.items():
    assert pin(source.read("c/repo/" + name)) == value
    assert pin((REPO / name).read_bytes()) == value
stdout = native.read("c/workspace-tests-attempt-1.stdout.bin")
stderr = native.read("c/workspace-tests-attempt-1.stderr.bin")
summaries = [tuple(map(int, result)) for result in re.findall(rb"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", stdout)]
assert summaries == [tuple(values) for values in final["rust_suite_summaries"]]
assert tuple(sum(s[i] for s in summaries) for i in range(3)) == (726, 0, 0)
for mode in range(4):
    assert b"Observed live raw sharing violation; mode=" + str(mode).encode() + b", outcome=" in stderr
commands = json.loads(native.read("c/native-commands.json"))
for command in commands:
    assert command["exit_code"] == 0 and not command["timed_out"]
    assert command["source_before_after_equal"] and command["external_sentinel_before_after_equal"]
    for stream in ("stdout", "stderr"):
        name = command[stream]["path"].replace("\\", "/").rsplit("/", 1)[-1]
        assert pin(native.read("c/" + name)) == fixed(command[stream])
assert pin(native.read("c/noetome-selected.exe")) == final["cli"]
assert final["cli"] == {"sha256": "e42c534d07f66662411fc52f22561510b79ba1c63f14f8b6f419db7c4e0e3274", "byte_length": 25939968}
old_cache = source.read("a/repo/crates/noetome-cli/tests/support/source_cache_race_tests.rs")
new_cache = source.read("c/repo/crates/noetome-cli/tests/support/source_cache_race_tests.rs")
assert pin(new_cache)["sha256"] == "2702b9dfb0d782cdf6262a98bb68e98099f2650ef963633e4e3dcafcfadfca7a"
assert b"sync_channel(0)" in new_cache and b"raw_os_error() == Some(32)" in new_cache
assert b"no live raw guard observed" in new_cache and b"Duration::from_secs(5)" in new_cache
assert json.loads(native.read("a/workspace-tests-attempt-1.json"))["exit_code"] == 101
assert new_cache == (REPO / "crates/noetome-cli/tests/support/source_cache_race_tests.rs").read_bytes()
receipt = {"schema_version": 1, "status": "passed", "matrix_pin": pin(matrix_before), "actual_index_records": checked,
           "final_native_overrides_audit_pin": pin(prior_path.read_bytes()), "actual_identity_equals_raw_for_all_64": True,
           "windows_rust": {"actual_totals": {"passed": 726, "failed": 0, "ignored": 0}, "actual_raw_cache_guard_modes": [0, 1, 2, 3], "current_dependency_files_equal_frozen_c": len(subset), "actual_selected_cli": final["cli"], "raw_commands_checked": len(commands), "first_failure_exit_101_preserved": True},
           "cache_test_scope": {"old_source": pin(old_cache), "final_source": pin(new_cache), "change": "observer ready/release sync-channel handshake before original guarded validation; 5s actual guard deadline still required, real Win32 error32 and original mutations retained; only test helper changed"},
           "full_rust_executions_by_this_audit": 0, "linux_full_rust": "not_run", "model_requests": 0, "scientific_quality": "not_run", "published": False}
(HERE / "final-index-rust-source-audit.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"status": "passed", "index_combinations": len(checked), "windows_rust_actual_passed": 726, "rust_source_files_checked": 1825, "matrix_pin": pin(matrix_before)}))
