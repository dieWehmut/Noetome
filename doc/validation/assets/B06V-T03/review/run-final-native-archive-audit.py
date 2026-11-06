"""Independent read-only archive and native SVG pixel audit; no app launch."""
from pathlib import Path
import base64
import copy
import hashlib
from html.parser import HTMLParser
import io
import importlib.util
import json
import sys
from urllib.parse import urlparse, unquote
import zipfile
from PIL import Image, ImageChops

REPO = Path(__file__).resolve().parents[5]
HERE = Path(__file__).resolve().parent
BASE = HERE.parent
sys.path.insert(0, str(REPO / "scripts"))
import prepare_visual_compatibility as V


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def expected_pin(value):
    return {name: value[name] for name in ("sha256", "byte_length")}


def png(raw):
    im = Image.open(io.BytesIO(raw))
    im.load()
    return im.convert("RGBA")


archive_checks = []
archives = {}
before_archive_pins = {}
for role in ("current", "minimum"):
    for kind in ("native", "source"):
        path = BASE / role / (kind + "-originals.zip")
        raw = path.read_bytes()
        if role == "current":
            receipt = json.loads((BASE / role / (kind + "-archive-receipt.json")).read_bytes())
            declared = receipt["archive"] if kind == "native" else receipt
        else:
            declared = json.loads((BASE / role / (kind + "-originals-manifest.json")).read_bytes())
        assert pin(raw) == expected_pin(declared)
        z = zipfile.ZipFile(io.BytesIO(raw))
        assert z.testzip() is None
        inventory = json.loads(z.read("inventory.json")) if role == "current" else declared["members"]
        assert set(z.namelist()) == set(inventory) | ({"inventory.json"} if role == "current" else set())
        for name, fixed in inventory.items():
            assert pin(z.read(name)) == expected_pin(fixed), name
        key = role + "_" + kind
        archives[key] = z
        before_archive_pins[str(path.relative_to(REPO))] = pin(raw)
        archive_checks.append({"role": role, "kind": kind, "archive_pin": pin(raw),
                               "independently_crc_and_sha_checked_members": len(inventory)})

current_receipt = json.loads((BASE / "current/native-archive-receipt.json").read_bytes())
minimum_receipt = json.loads((BASE / "minimum/audit-result.json").read_bytes())
packs = {}
features = {}
for name in ("visual-compatibility", "visual-compatibility-corrected-v2", "visual-compatibility-flow-extensions"):
    root = REPO / "fixtures" / name
    cases = json.loads((root / "cases.json").read_bytes())["cases"]
    packs[name] = {c["case_id"]: c for c in cases}
    features.update({c["case_id"]: c["feature"] for c in cases})

current_rows = current_receipt["selected_rows"]
assert len(current_rows) == 32
minimum_rows = minimum_receipt["records"]
assert len(minimum_rows) == 32
records = []
source_files = 0
native_pass_counts = {}


def without_unobserved_svg_blocks(raw):
    text = raw.decode()
    if V.SCREENSHOT_PIXELS in text:
        assert text.count(V.SCREENSHOT_PIXELS) == 1
        text = text.replace(V.SCREENSHOT_PIXELS, "", 1)
    for start, end in (("const verifySourceSvg =", "let owned = false;"),
                       ("    if (note.svg_semantics) {", "    actual.samples.forEach")):
        assert text.count(start) == text.count(end) == 1
        a, b = text.index(start), text.index(end)
        text = text[:a] + text[b:]
    return text.encode()


scope_path = BASE / "minimum/final-collector-scope-originals/captured-b7-preparation.py"
module_spec = importlib.util.spec_from_file_location("captured_b7_native_audit", scope_path)
captured_b7 = importlib.util.module_from_spec(module_spec)
module_spec.loader.exec_module(captured_b7)
# An archived standalone copy computes its repository-relative paths elsewhere;
# only the unchanged original collector input is supplied for derivation here.
captured_b7_image = captured_b7.derived_collector(V.P.read(V.COLLECTORS["image"]), "image")
for role, rows in (("current", current_rows), ("minimum", minimum_rows)):
    z = archives[role + "_native"]
    actual_pass = actual_fail = 0
    for name in z.namelist():
        if not name.endswith("/capture-receipt.json"):
            continue
        stem = name[:-len("capture-receipt.json")]
        if role == "current" and name.startswith("standalone/"):
            continue
        receipt = json.loads(z.read(name))
        result = json.loads(z.read(stem + "result.json")) if stem + "result.json" in z.namelist() else None
        if receipt["exit_code"] == 0:
            assert result is not None and result["status"] == "passed"
            actual_pass += 1
        else:
            actual_fail += 1
    native_pass_counts[role] = {"actual_native_passed_attempts": actual_pass, "actual_native_failed_attempts": actual_fail}
    assert (actual_pass, actual_fail) == ((32, 10) if role == "current" else (38, 4))
    combos = set()
    for row in rows:
        case_id, theme = row["case_id"], row["theme"]
        assert (case_id, theme) not in combos
        combos.add((case_id, theme))
        if role == "current":
            native_root = Path(row["capture"]["argv"][4])
            stem = native_root.parent.name + "/" + native_root.name + "/"
            pack = row["pack"]
        else:
            stem = row["result_member"][:-len("result.json")]
            pack = "visual-compatibility-flow-extensions" if case_id in packs["visual-compatibility-flow-extensions"] else "visual-compatibility-corrected-v2" if case_id in ("scan-sign", "scan-subscript") else "visual-compatibility"
        result = json.loads(z.read(stem + "result.json"))
        input_value = json.loads(z.read(stem + "visual-input.json"))
        spec = input_value["spec"]
        source = json.loads(z.read(stem + "source-before.json"))
        assert source == input_value["source_pins"]
        assert result["status"] == "passed"
        if "case" in result:
            assert result["case"] == case_id
        elif "caseName" in result:
            assert result["caseName"] == spec["bridge"]["case_name"]
        assert spec["case"] == case_id and spec["theme"] == theme
        assert json.loads(z.read(stem + "capture-receipt.json"))["exit_code"] == 0
        assert pin(z.read(stem + "capture-spec.json")) == input_value["spec_pin"]
        fixed = packs[pack][case_id]
        original_root = REPO / "fixtures" / pack / ("probe" if features[case_id] == "reader_format_probe" else "selected-output/" + case_id)
        original_bytes = V.files(original_root)
        assert {name: pin(raw) for name, raw in original_bytes.items()} == source
        for name, fixed_pin in source.items():
            assert pin(z.read(stem + "vault/" + name)) == fixed_pin
            assert z.read(stem + "vault/" + name) == original_bytes[name]
            source_files += 1
        runtime = spec["runtime"]
        identity = result["compatibilityIdentity"]
        expected_version = "1.15.0" if role == "current" else "1.6.7"
        assert identity["appVersion"] == identity["appPackageVersion"] == identity["obsidianPackageVersion"] == expected_version
        assert identity["execPath"].replace("\\", "/").lower() == runtime["executable"]["windows_path"].replace("\\", "/").lower()
        assert "theme-" + theme in identity["themeClass"].split()
        for name in ("executable", "app_asar", "obsidian_asar"):
            assert identity["files"][name] == expected_pin(runtime[name])
        modules = result["compatibilityRuntime"]["modules"]
        for name in ("app_bundle",) + (("mathjax",) if spec["require_mathjax_runtime"] else ()) + (("mermaid",) if spec["require_mermaid_runtime"] else ()):
            observed = modules[name]
            assert observed["status"] == "passed"
            assert {"sha256": observed["rawSha256"], "byte_length": observed["rawByteLength"]} == expected_pin(runtime[name]["member"])
        if spec["require_mathjax_runtime"]:
            assert result["compatibilityRuntime"]["globals"]["mathjaxVersion"] == runtime["mathjax"]["version"]
        for name in spec["blocked_absent"]:
            assert stem + "vault/" + name not in z.namelist()
        cleanup = json.loads(z.read(stem + "cleanup-result.json"))
        assert cleanup["profileProcessCount"] == 0 and cleanup["ownedListenerAbsent"] is True and cleanup["ownedPidAbsent"] is True and cleanup["emergencyTerminated"] == []
        kind = features[case_id]
        derived = V.FORMAT_COLLECTOR.replace("__RUNTIME_HELPERS__", V.RUNTIME_HELPERS).replace("__FORMAT_TEXT__", V.FORMAT_TEXT).encode("utf8") if kind == "reader_format_probe" else V.derived_collector(V.P.read(V.COLLECTORS["formula" if kind == "scan" else kind]), "formula" if kind == "scan" else kind)
        # Scan has its own collector kind while sharing the original formula file.
        if kind == "scan":
            derived = V.derived_collector(V.P.read(V.COLLECTORS["scan"]), "scan")
        script_raw = z.read(stem + "capture-script.mjs")
        final_match = script_raw == derived
        image_scope = None
        if kind == "image" and case_id == "image-curve-no-data" and not final_match:
            assert all(note.get("svg_semantics") is None for note in spec["notes"])
            assert all(note.get("svgEvidence") is None for note in result["notes"])
            assert without_unobserved_svg_blocks(script_raw) == without_unobserved_svg_blocks(derived)
            image_scope = "actual no-SVG curve fallback; exact entire non-SVG collector bytes unchanged after removing only unobserved SVG-only function/branch and added PNG helpers"
        elif role == "minimum" and kind == "image" and not final_match:
            assert script_raw == captured_b7_image
            normalized = derived.decode().replace(V.SCREENSHOT_PIXELS, captured_b7.SCREENSHOT_PIXELS, 1).replace(V.INTRINSIC_INK, "", 1)
            old_gate = "    if(bounds) assert(e.bbox.x>=bounds.x-1&&e.bbox.y>=bounds.y-1&&e.bbox.x+e.bbox.width<=bounds.x+bounds.width+1&&e.bbox.y+e.bbox.height<=bounds.y+bounds.height+1,'actual label lies in its source-associated shape/legend within one nominal font-metric pixel');"
            new_gate = "    const measuredBounds=e.actualInk?.bbox??e.bbox;\n" + old_gate.replace("e.bbox", "measuredBounds")
            assert normalized.count(new_gate) == 1
            normalized = normalized.replace(new_gate, old_gate, 1)
            assert normalized.encode() == script_raw
            assert all("pixelMethod" not in note.get("svgEvidence", {}).get("native", {}) for note in result["notes"])
            assert all("actualInk" not in e for note in result["notes"] for e in note.get("svgEvidence", {}).get("geometry", {}).get("renderedElements", []))
            image_scope = "whole b7 actual minimum SVG capture retained; removing only final current ink helper/guard/gate additions restores exact captured script; observed legacy has no pixelMethod/actualInk"
        else:
            assert final_match, (role, case_id, theme, "actual capture collector differs from final branch")
        records.append({"role": role, "case_id": case_id, "theme": theme, "result_member": stem + "result.json",
                        "result_pin": pin(z.read(stem + "result.json")), "source_files_independently_equal_frozen_originals": len(source),
                        "actual_capture_script_pin": pin(script_raw), "final_derived_byte_equal": final_match,
                        "image_scope": image_scope})
    assert len(combos) == 32 and {theme for _, theme in combos} == {"light", "dark"}


class Images(HTMLParser):
    def __init__(self):
        super().__init__(); self.images = []

    def handle_starttag(self, tag, attrs):
        if tag == "img":
            self.images.append(dict(attrs))


svg_audits = []
z = archives["current_native"]
for theme in ("light", "dark"):
    stem = "b06v-t03-current-matrix-round-05-20261106/capture-image-signed-arrows-" + theme + "/"
    result = json.loads(z.read(stem + "result.json"))
    evidence = result["notes"][0]["svgEvidence"]
    native = evidence["native"]
    source = z.read(stem + "vault/redrawn.svg")
    assert native["svgSha256"] == pin(source)["sha256"] and native["sourceByteLength"] == len(source)
    uri = urlparse(native["src"])
    assert uri.scheme == "app"
    assert unquote(uri.path).lstrip("/").replace("\\", "/").lower() == native["sourceFile"].replace("\\", "/").lower()
    parser = Images(); parser.feed(z.read(stem + "note-0-svg-native-before-pixels.html").decode())
    native_elements = [e for e in parser.images if e.get("src") == native["src"]]
    assert len(native_elements) == 1 and not native_elements[0]["src"].startswith("data:")
    matched_parser = Images(); matched_parser.feed(z.read(stem + "note-0-svg-matched-auxiliary.html").decode())
    matched = [e for e in matched_parser.images if e.get("id") == "matched-image"]
    assert len(matched) == 1 and matched[0]["src"].startswith("data:image/svg+xml;base64,")
    assert base64.b64decode(matched[0]["src"].split(",", 1)[1], validate=True) == source
    native_png = png(z.read(stem + evidence["nativeDecodedPng"]))
    auxiliary_png = png(z.read(stem + evidence["matchedScreenshot"]["auxiliaryFile"]))
    assert native_png.size == auxiliary_png.size == (700, 380)
    assert native_png.tobytes() == auxiliary_png.tobytes()
    rgba_sha = hashlib.sha256(native_png.tobytes()).hexdigest()
    assert rgba_sha == native["decodedRgbaSha256"] == evidence["auxiliaryRgbaSha256"]
    ink = evidence["intrinsicInk"]
    full = png(z.read(stem + ink["fullFile"]))
    restored = png(z.read(stem + ink["restoredFile"]))
    assert full.size == restored.size == (960, 520)
    assert full.tobytes() == restored.tobytes()
    assert full.getchannel("A").getextrema() == (255, 255)
    assert hashlib.sha256(full.tobytes()).hexdigest() == ink["fullRgbaSha256"] == ink["restoredRgbaSha256"]
    source_semantics = evidence["semanticExpected"]["source"]
    bounds = {}
    for item in source_semantics["elements"]:
        bounds["element-" + item["id"] + "-label"] = item["bbox"]
        if item["unit"] is not None:
            bounds["element-" + item["id"] + "-unit"] = item["bbox"]
    for item in source_semantics["legend"]:
        bounds["legend-" + item["id"] + "-label"] = item["bbox"]
        bounds["legend-" + item["id"] + "-meaning"] = item["bbox"]
    labels = []
    geometry = {e["id"]: e for e in evidence["geometry"]["renderedElements"]}
    for label in ink["labels"]:
        hidden = png(z.read(stem + label["hiddenFile"]))
        assert hidden.size == full.size and hidden.getchannel("A").getextrema() == (255, 255)
        assert hashlib.sha256(hidden.tobytes()).hexdigest() == label["hiddenRgbaSha256"]
        diff = ImageChops.difference(full, hidden)
        channels = diff.split()
        union = channels[0]
        for channel in channels[1:]:
            union = ImageChops.lighter(union, channel)
        box = union.getbbox()
        assert box is not None
        bbox = {"x": box[0], "y": box[1], "width": box[2] - box[0], "height": box[3] - box[1]}
        changed_pixels = sum(1 for value in union.getdata() if value)
        assert bbox == label["bbox"] and changed_pixels == label["changedPixels"]
        assert geometry[label["id"]]["actualInk"]["bbox"] == bbox
        original_bounds = bounds.get(label["id"])
        if original_bounds is not None:
            x, y, width, height = original_bounds
            assert bbox["x"] >= x - 1 and bbox["y"] >= y - 1
            assert bbox["x"] + bbox["width"] <= x + width + 1 and bbox["y"] + bbox["height"] <= y + height + 1
        labels.append({"id": label["id"], "actual_rgba_difference_bbox": bbox,
                       "changed_pixels": changed_pixels, "source_bounds": original_bounds,
                       "nominal_bbox_retained": label["nominalBbox"]})
    assert len(labels) == 16
    # Valid PNG with fresh CRC, not a corrupt-file rejection: alter one pixel.
    mutant = native_png.copy(); pixel = list(mutant.getpixel((30, 30))); pixel[0] ^= 1; mutant.putpixel((30, 30), tuple(pixel))
    buffer = io.BytesIO(); mutant.save(buffer, format="PNG")
    changed = png(buffer.getvalue())
    assert hashlib.sha256(changed.tobytes()).hexdigest() != rgba_sha
    svg_audits.append({"theme": theme, "native_src": native["src"], "owned_source_path": native["sourceFile"],
                       "source_pin": pin(source), "native_src_is_original_app_protocol": True,
                       "auxiliary_data_url_equals_pinned_source_bytes": True,
                       "displayed_size": list(native_png.size), "native_aux_rgba_sha256": rgba_sha,
                       "intrinsic_size": list(full.size), "all_labels_restored_rgba_equal": True,
                       "labels": labels, "valid_crc_one_pixel_mutation_rejected": True})

after_archive_pins = {name: pin((REPO / name).read_bytes()) for name in before_archive_pins}
assert before_archive_pins == after_archive_pins
result = {"schema_version": 1, "status": "passed", "scope": "read-only fixed Windows minimum/current archive audit; independent SHA/CRC, all 64 capture source files, installed/runtime pins, script identity, no false flags; current SVG complete RGBA, source bytes, 16 real label masks and full restore",
          "archive_checks": archive_checks, "actual_attempts": native_pass_counts,
          "selected_unique_combinations": len(records), "source_files_independently_rechecked": source_files,
          "records": records, "current_svg_pixel_audits": svg_audits,
          "archive_before": before_archive_pins, "archive_after": after_archive_pins,
          "app_launches": 0, "model_requests": 0, "process_control_calls": 0,
          "linux_full_rust": "not_run", "scientific_quality": "not_run", "published": False}
(HERE / "final-native-archive-audit.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"status": "passed", "selected_unique_combinations": len(records), "source_files_independently_rechecked": source_files, "archive_member_counts": [v["independently_crc_and_sha_checked_members"] for v in archive_checks], "actual_attempts": native_pass_counts, "svg_actual_masks": sum(len(s["labels"]) for s in svg_audits)}))
