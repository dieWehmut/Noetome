"""Independently verify the four final format rows and preserved 64-row audit."""
from pathlib import Path
import hashlib
import json
import sys
import zipfile

REPO = Path(__file__).resolve().parents[5]
HERE = Path(__file__).resolve().parent
BASE = HERE.parent
sys.path.insert(0, str(REPO / "scripts"))
import prepare_visual_compatibility as V


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def fixed(value):
    return {key: value[key] for key in ("sha256", "byte_length")}


prior_path = HERE / "final-native-archive-audit.json"
prior = json.loads(prior_path.read_bytes())
assert prior["status"] == "passed" and prior["selected_unique_combinations"] == 64
assert all(pin((REPO / name).read_bytes()) == value for name, value in prior["archive_after"].items())
fixtures = []
for name, anchor in (
    ("visual-compatibility", "9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c"),
    ("visual-compatibility-flow-extensions", "e9f6a8b0e86718a38dba5cbf5073c25ffb0a6e39ca0c1d8c43a3648f4952339e"),
    ("visual-compatibility-corrected-v2", "c174f543947b6e72c7ef7e786624c4d2fa870342df79abcd9d0fc7fbb90cc60d"),
):
    _, original = V.fixture(REPO / "fixtures" / name, anchor)
    fixtures.append({"name": name, "anchor": anchor, "all_frozen_files_independently_rechecked": len(original)})
final_tool = (REPO / "scripts/prepare_visual_compatibility.py").read_bytes()
assert pin(final_tool)["sha256"] == "9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f"
with zipfile.ZipFile(HERE / "preparation-tool-validation-04-originals.zip") as z:
    old_candidates = [n for n in z.namelist() if n.endswith("scripts/prepare_visual_compatibility.py")]
    assert len(old_candidates) == 1
    old = z.read(old_candidates[0])
assert final_tool == old.replace(b"  owned=true;assert.deepEqual(identity.communityPlugins,[]);\n  result.compatibilityIdentity", b"  owned=true;assert.deepEqual(identity.communityPlugins,[]);\n  result.identity=identity;\n  result.compatibilityIdentity", 1)
format_raw = V.FORMAT_COLLECTOR.replace("__RUNTIME_HELPERS__", V.RUNTIME_HELPERS).replace("__FORMAT_TEXT__", V.FORMAT_TEXT).encode()
case = "hand-authored-format-probe"
original = (REPO / "fixtures/visual-compatibility/probe/格式兼容探针.md").read_bytes()
expected = json.loads((REPO / "fixtures/visual-compatibility/expected-hand-authored-format-probe.json").read_bytes())
archive_checks = []
records = []
for role, override_path in (
    ("current", BASE / "current/format-identity-supplement/override-records.json"),
    ("minimum", BASE / "minimum/format-identity-override.json"),
):
    override = json.loads(override_path.read_bytes())
    if role == "current":
        declared = override["native_archive"]
        archive = REPO / declared["path"]
        inventory = None
    else:
        declared = override["archive"]
        archive = BASE / "minimum" / declared["archive"]
        inventory = json.loads((BASE / "minimum/format-identity-originals-manifest.json").read_bytes())["members"]
    assert pin(archive.read_bytes()) == fixed(declared)
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None
        if inventory is None:
            inventory = json.loads(z.read("inventory.json"))
            assert set(z.namelist()) == set(inventory) | {"inventory.json"}
        else:
            assert set(z.namelist()) == set(inventory)
        assert all(pin(z.read(name)) == fixed(value) for name, value in inventory.items())
        archive_checks.append({"role": role, "archive": str(archive.relative_to(REPO)), "archive_pin": pin(archive.read_bytes()), "independent_member_count": len(inventory)})
        assert len(override["records"]) == 2
        for row in override["records"]:
            assert row["case_id"] == case and row["status"] == "passed"
            stem = row["result_member"][:-len("result.json")]
            result = json.loads(z.read(row["result_member"]))
            actual_input = json.loads(z.read(row["input_member"]))
            cleanup = json.loads(z.read(row["cleanup_member"]))
            assert pin(z.read(row["result_member"])) == row["result_pin"]
            assert pin(z.read(row["input_member"])) == row["input_pin"]
            assert pin(z.read(row["cleanup_member"])) == row["cleanup_pin"]
            assert result["status"] == "passed"
            assert z.read(stem + "capture-script.mjs") == format_raw
            assert z.read(stem + "vault/格式兼容探针.md") == original
            assert actual_input["source_pins"] == {"格式兼容探针.md": pin(original)}
            identity = result["identity"]
            normalize = lambda path: path.replace("\\", "/").lower()
            assert normalize(identity["userData"]) == normalize(actual_input["profile"])
            assert normalize(identity["vaultPath"]) == normalize(actual_input["vault"])
            assert identity["communityPlugins"] == []
            assert cleanup["profileProcessCount"] == 0 and cleanup["ownedPidAbsent"] and cleanup["ownedListenerAbsent"] and cleanup["emergencyTerminated"] == []
            version = "1.15.0" if role == "current" else "1.6.7"
            native_runtime = result["compatibilityIdentity"]
            assert native_runtime["appVersion"] == native_runtime["appPackageVersion"] == native_runtime["obsidianPackageVersion"] == version
            assert "theme-" + row["theme"] in native_runtime["themeClass"].split()
            actual = result["notes"][0]["actual"]
            assert actual["rawSource"].encode() == original
            for key, value in expected["semantic_source"]["frontmatter"].items():
                assert actual["frontmatter"][key] == value
            properties = {p["key"]: p for p in actual["properties"]}
            assert properties["title"]["fieldLabel"] == "title" and properties["title"]["titleValue"] == "中文格式兼容探针"
            assert properties["reviewed"]["fieldLabel"] == "reviewed" and properties["reviewed"]["checkboxChecked"] is False
            assert properties["aliases"]["fieldLabel"] == "aliases" and properties["aliases"]["chips"] == ["兼容别名", "第二名称"]
            observed_types = {key: properties[key]["valueType" if role == "current" else "parentType"] for key in ("title", "reviewed", "aliases")}
            assert observed_types == {"title": "text", "reviewed": "checkbox", "aliases": "aliases" if role == "current" else "multitext"}
            assert len(result["clicks"]) == 2
            for click in result["clicks"]:
                assert click["actualClick"] is True and click["status"] == "passed"
                assert click["finalObservation"]["visible"] is True and click["finalObservation"]["targetCount"] == 1
            block = result["clicks"][1]["finalObservation"]
            assert block["cachedBlock"]["id"] == "compat-block"
            assert block["targets"][0]["paragraphText"] == "这是一段用于定位的原始中文块，内容必须完整保留。\n"
            assert block["targets"][0]["comparisonText"] == "这是一段用于定位的原始中文块，内容必须完整保留。"
            records.append({"role": role, "case_id": case, "theme": row["theme"], "archive": str(archive.relative_to(REPO)),
                            "result_member": row["result_member"], "result_pin": row["result_pin"], "actual_identity": identity,
                            "actual_property_types": observed_types, "actual_clicks": 2, "full_original_source_equal": True,
                            "actual_capture_script_equals_final": True, "cleanup_pid": cleanup["ownedPid"], "cleanup_port": cleanup["ownedPort"]})
final = [row for row in prior["records"] if row["case_id"] != case] + records
assert len(final) == 64 and len({(r["role"], r["case_id"], r["theme"]) for r in final}) == 64
assert pin((REPO / "scripts/prepare_visual_compatibility.py").read_bytes()) == pin(final_tool)
result = {"schema_version": 1, "status": "passed", "scope": "independent final 64 native combinations; replace only four format rows using actual serialized identity, preserve prior 64 audit and all failures; exact single-line format delta",
          "prior_native_audit": str(prior_path.relative_to(REPO)), "prior_native_audit_pin": pin(prior_path.read_bytes()),
          "prior_archives_unchanged": True, "fixtures": fixtures, "source_tool": pin(final_tool),
          "exact_single_line_delta": True, "archive_checks": archive_checks, "four_format_overrides": records,
          "final_selected_unique_combinations": len(final), "final_records": final,
          "source_scoped_reuse": "60 unchanged final combinations retain prior raw/source audit; current image curve no-SVG code unchanged outside dormant SVG-only blocks; minimum b7 SVG actual legacy path unchanged after explicit current-only ink additions removed; final metadata line is format-only",
          "independent_svg_complete_rgba_and_mask_evidence": "prior fixed archive two themes, 700x380 complete native/aux RGBA equal, exact source path/aux source bytes, 960x520 full/restored equal, 16 real masks per theme within unchanged source 1px bounds",
          "app_launches_by_this_readonly_audit": 0, "model_requests": 0, "linux_full_rust": "not_run", "scientific_quality": "not_run", "published": False}
(HERE / "final-native-format-overrides-audit.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"status": "passed", "final_selected_unique_combinations": 64, "actual_format_overrides": 4, "source_tool": pin(final_tool), "archives": archive_checks}))
