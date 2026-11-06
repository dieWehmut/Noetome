"""Read-only continuation audit of the explicitly selected B06V-T02 evidence.

This invokes no CLI, model, credential reader, or native application. Original
absolute manifests are checked at their preserved capture paths without edits.
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import sys
import zipfile
from datetime import datetime, timezone

REPO = pathlib.Path(__file__).resolve().parents[5]
sys.path.insert(0, str(REPO / "scripts"))
import verify_scan_formula as scan

ROOT = REPO / "doc/validation/assets/B06V-T02"
LIVE = ROOT / "recognition/vision-live-final"
NATIVE = ROOT / "native-live-final"
ORIGINAL = pathlib.Path("/mnt/d/Noetome/.artifacts/b06v-t02-live-scan-20261106")


def load(path):
    return json.loads(pathlib.Path(path).read_bytes())


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def same(value, expected, label):
    assert value == expected, label


def windows_path(raw):
    p = pathlib.PureWindowsPath(raw)
    assert p.is_absolute() and p.drive == "D:", "explicit original task path"
    return pathlib.Path("/mnt/d", *p.parts[1:])


def live_audit():
    receipt = load(LIVE / "live-receipt.json")
    calls = []
    previous = None
    first_history = None
    all_members = 0
    for recorded in receipt["calls"]:
        archive = LIVE / recorded["archive"]
        same(pin(archive.read_bytes()), {
            "sha256": recorded["archive_sha256"],
            "byte_length": recorded["archive_byte_length"],
        }, "live archive original SHA and size")
        with zipfile.ZipFile(archive) as zipped:
            assert zipped.testzip() is None, "live archive CRC"
            same(set(zipped.namelist()), set(recorded["inventory"]), "complete archive inventory")
            for name, expected_pin in recorded["inventory"].items():
                raw = zipped.read(name)
                same(pin(raw), expected_pin, "live archive member " + name)
                original = ORIGINAL / name
                if name == "selected-profile.json":
                    profile_name = "unverified.json" if recorded["name"] == "fallback" else "native.json"
                    original = ORIGINAL / "fixtures/scan-formula/profiles" / profile_name
                same(original.read_bytes(), raw, "archive/original bytes " + name)
            all_members += len(zipped.namelist())
        name = recorded["name"]
        capture = ORIGINAL / (name + "-budget-capture")
        binding_raw = (capture / "budget-binding.json").read_bytes()
        anchor = recorded["budget_binding_sha256"]
        same(pin(binding_raw)["sha256"], anchor, "independent original manifest anchor")
        binding = json.loads(binding_raw)
        phase = {k: load(v["file"]) for k, v in binding["receipts"].items()}
        invocation = load(capture / "invocation-receipt.json")
        cmd = invocation["command"]
        state = "unreadable_sign" if name == "sign" else "unreadable_subscript" if name == "subscript" else "clear"
        case, config, profile, vault = [windows_path(v) for v in cmd[3:]]
        result = scan.verify_output(case, config, profile, vault,
            REPO / "fixtures/scan-formula" / ("expected-" + state + ".json"),
            invocation_receipt=invocation, budget_receipt=binding_raw, budget_anchor=anchor)
        raw_response = load(vault / "response.bin")
        same(raw_response["model"], "deepseek-flash", "observed model alias")
        same(raw_response["choices"][0]["finish_reason"], "stop", "actual finish reason")
        candidate = json.loads(raw_response["choices"][0]["message"]["content"])
        same(candidate, recorded["recognition"], "actual original recognition")
        same(raw_response["usage"]["total_tokens"], recorded["usage"]["model_usage"]["total_tokens"], "actual usage")
        report = load(vault / "scan.json")
        before = load(next(v["file"] for k, v in binding["copies"].items() if k == phase["before"]["ledger_snapshot"]["path"]))
        settled = load(next(v["file"] for k, v in binding["copies"].items() if k == phase["settled"]["ledger_snapshot"]["path"]))
        if previous is not None:
            same(before, previous, "successive calls retain exact prior settled ledger")
        else:
            same(before["accounted_tokens"], 30097, "restored six-record baseline")
            same(len(before["records"]), 6, "complete six-record historical baseline")
            first_history = before["records"]
        same(settled["records"][:6], first_history, "historical six records preserved")
        previous = settled
        calls.append({"name": name, "independent_output": result,
                      "actual_original_recognition": candidate,
                      "raw_usage": raw_response["usage"],
                      "actual_model": raw_response["model"],
                      "http_status": report["api"]["http_status"],
                      "ordinary_cli_exit": invocation["exit_code"],
                      "budget_anchor": anchor,
                      "four_phase_originals": "passed",
                      "confirmed_cumulative_tokens": settled["accounted_tokens"],
                      "pending_reserved_tokens": settled["pending_reserved_tokens"]})
    same(previous["accounted_tokens"], 33185, "final confirmed arithmetic")
    same(previous["pending_reserved_tokens"], 0, "all new calls settled")
    same(previous["remaining_tokens"], 9966815, "final approved-budget remaining")
    same(len(previous["records"]), 10, "ten complete historical plus new records")
    return {"calls": calls, "archive_members_checked": all_members,
            "new_confirmed_tokens": sum(c["raw_usage"]["total_tokens"] for c in calls),
            "final_accounted_tokens": previous["accounted_tokens"],
            "pending_reserved_tokens": previous["pending_reserved_tokens"],
            "remaining_tokens": previous["remaining_tokens"],
            "missing_interval_basis": "user_directed_assumption_zero",
            "service_balance_independently_verified": False}


def native_audit():
    receipt = load(NATIVE / "receipt.json")
    expected_tree = ["row", [
        ["msub", [["mi", "x"], ["mi", "i"]]], ["mo", "="], ["mo", "−"],
        ["mfrac", [["msub", [["mi", "a"], ["mi", "i"]]],
                   ["msub", [["mi", "b"], ["mi", "i"]]]]],
    ]]
    records = []
    expected = load(REPO / "fixtures/scan-formula/expected-clear.json")
    members = 0
    for case in receipt["cases"]:
        directory = NATIVE / case["name"]
        same({p.name for p in directory.iterdir()}, set(case["pins"]), "complete native inventory")
        for name, expected_pin in case["pins"].items():
            same(pin((directory / name).read_bytes()), expected_pin, "native original SHA " + name)
        members += len(case["pins"])
        result = load(directory / "result.json")
        input_ = load(directory / "visual-input.json")
        spec = load(directory / "capture-spec.json")
        same(pin((directory / "capture-spec.json").read_bytes()), input_["spec_pin"], "actual selected capture spec")
        same(result["status"], "passed", "native capture terminal status")
        identity = result["identity"]
        same(identity["appVersion"], "1.6.7", "actual runtime app version")
        same(identity["communityPlugins"], [], "clean community plugins")
        same(identity["userData"], input_["profile"], "actual explicit owned profile")
        same(identity["vaultPath"], input_["vault"], "actual selected test Vault")
        assert "theme-" + spec["theme"] in identity["bodyClass"], "actual requested theme"
        same(spec["semantic_source"], {k: expected[k] for k in ("symbols", "conditions", "ranges", "exceptions")}, "independent frozen context")
        same(spec["expected_fingerprint"], pin((REPO / "fixtures/scan-formula/expected-clear.json").read_bytes()), "unchanged independent expectation")
        same(result["openedOriginalFiles"], spec["original_inventory"], "actual opened source bytes")
        copied = pathlib.Path(input_["source"])
        same({p.name: pin(p.read_bytes()) for p in copied.iterdir()}, spec["original_inventory"], "prepared selected output bytes")
        same({p.name: pin(p.read_bytes()) for p in windows_path(input_["vault"]).iterdir() if p.is_file()}, spec["original_inventory"], "actual display Vault bytes")
        note = result["notes"][0]
        actual = note["actual"]
        if spec["representation"] == "native_math":
            same(result["mathjax"]["version"], "3.2.2", "actual MathJax runtime")
            same([m["tree"] for m in actual["math"]], [expected_tree], "independent math structure")
            for m in actual["math"]:
                assert m["rect"]["width"] > 0 and m["rect"]["height"] > 0, "visible math"
                same(m["errors"], [], "no MathJax error")
                for s in m["scriptGeometry"]:
                    assert s["script"]["y"] > s["base"]["y"], "visible subscript below base"
                for f in m["fractionGeometry"]:
                    assert f["numerator"]["y"] < f["denominator"]["y"], "visible numerator above denominator"
                same((directory / "note-0-math-0.chtml.html").read_text().rstrip(), m["chtml"], "original CHTML tree bytes")
        else:
            same(actual["math"], [], "fallback no invented native formula")
            same(len(actual["images"]), 1, "one original fallback image")
            img = actual["images"][0]
            same((img["naturalWidth"], img["naturalHeight"]), (560, 200), "fallback original dimensions")
            assert img["complete"] and not img["src"].startswith("http"), "loaded local original image"
        same(actual["activeTags"], [], "no active markup")
        same([p["text"].removesuffix("\n") for p in actual["preformatted"]], note["expected"]["preformatted_exact"], "exact visible source context")
        same(len(result["clicks"]), 2, "actual two attachment clicks")
        for click, target, size in zip(result["clicks"], ["original.png", "page.png"], [(560, 200), (800, 1000)]):
            same(click["actual_click"], True, "real click recorded")
            same(click["target"], target, "selected actual click target")
            image = click["actual"]["image"]["actual"]
            same(image["activeFile"], target, "actual opened attachment")
            same(image["viewType"], "image", "actual native image view")
            same((image["naturalWidth"], image["naturalHeight"]), size, "actual clicked image dimensions")
            assert image["complete"] and image["rect"]["width"] > 0 and image["rect"]["height"] > 0, "loaded visible attachment"
            assert image["src"].startswith("app://") and target in image["src"], "local source image URL"
        launch = load(directory / "launch-receipt.json")
        same(launch["exit_code"], 0, "actual launcher exit")
        same(launch["argv"][1], "--user-data-dir=" + identity["userData"], "explicit new profile at launch")
        same(load(directory / "capture-receipt.json")["exit_code"], 0, "actual capture process exit")
        cleanup = load(directory / "cleanup-result.json")
        same(cleanup["ownedPid"], launch["pid"], "owned native PID cleanup")
        same(cleanup["profileProcessCount"], 0, "no owned profile processes")
        same(cleanup["ownedListenerAbsent"], True, "no owned listener")
        same(cleanup["ownedPidAbsent"], True, "native PID absent")
        records.append({"name": case["name"], "representation": spec["representation"],
                        "runtime_app_version": identity["appVersion"], "theme": spec["theme"],
                        "mathjax": result["mathjax"]["version"], "source_pin_count": len(spec["original_inventory"]),
                        "actual_attachment_clicks": len(result["clicks"]),
                        "launch_pid": launch["pid"], "launcher_pin": launch["launcher"],
                        "cleanup": cleanup, "status": "passed"})
    return {"cases": records, "original_native_members_checked": members,
            "actual_attachment_clicks": sum(r["actual_attachment_clicks"] for r in records)}


if __name__ == "__main__":
    result = {"recorded_at": datetime.now(timezone.utc).isoformat(), "status": "passed",
              "command": [sys.executable, str(pathlib.Path(__file__).resolve())],
              "fixture": scan.verify_fixture(), "live": live_audit(), "native": native_audit(),
              "model_calls_from_audit": 0, "credential_reads": 0, "ledger_writes": 0,
              "native_launches_from_audit": 0, "scientific_quality": "not_run", "published": False}
    output = pathlib.Path(__file__).with_suffix(".json")
    with output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps({"status": result["status"], "output": str(output),
                      "live_calls_read_back": len(result["live"]["calls"]),
                      "native_cases_read_back": len(result["native"]["cases"]),
                      "actual_attachment_clicks": result["native"]["actual_attachment_clicks"]}))
