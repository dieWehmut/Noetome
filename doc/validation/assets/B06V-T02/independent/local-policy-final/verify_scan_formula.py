"""Independent frozen-author scan audit; no OCR or production parser imports.

The ordinary CLI audit uses the explicitly selected real local OCR runtime.
Synthetic responses in unit tests exercise verification only, never real OCR.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import shutil
import subprocess
import sys
import time
import uuid

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_extraction_knowledge import strict_json, fingerprint
from verify_knowledge_coordination import fields, exact, display_text, uuid_string, safe_name
from verify_knowledge_conflicts import guard_path
from verify_formula import tokens as lexical_tokens, png_info, profile_schema

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "fixtures/scan-formula"
FROZEN_SHA = "99789373318a9e2f93c5b8449e471567e58f448754e59f757164b7ad18ab4b54"
STATES = ("clear", "unreadable_sign", "unreadable_subscript")
COMMON = dict(schema_version=1, producer="knowledge_scan_formula", scope="selected_scan_formula_v1",
              knowledge_status="pending", scientific_quality="not_run", published=False,
              network_calls=0, paid_calls=0)
CASE_FIELDS = "schema_version purpose selection_id title source_kind pdf page region semantics"
SEMANTIC_FIELDS = "schema_version producer expression symbols conditions ranges exceptions uncertainties"
REPORT_FIELDS = "schema_version producer scope request_id status preview_status knowledge_status scientific_quality published network_calls paid_calls case semantics profile checks decision recognition recognition_invocations recognition_request_fingerprint recognition_response_fingerprint runtime_inputs tokens plan_fingerprint inputs files artifacts"
PLAN_FIELDS = "schema_version producer scope request_id case semantics decision tokens recognized_expression compiled_expression"
CHECK_FIELDS = "source_preservation region_match recognition expression_match symbol_definitions semantic_review note_plan rendering saved_output"
RESPONSE_FIELDS = "schema_version producer package_version request_id input model recognized_tex issues elapsed_ms status confidence runtime policy"
MODEL_PINS = {
    "weights.pth": {"sha256": "a63d9141c53d266cb682fb5a8bd83bd5cbe283145e0e78ebdc0f895195a1dfaa", "byte_length": 102113875},
    "image_resizer.pth": {"sha256": "1c3820659985ad142b526490bb25c23d977176ac2073591b3bddada692718458", "byte_length": 19441973},
    "config.yaml": {"sha256": "a9b18acf6718ffed79ab669c9957c46936714f4b983705f0b1361d90f4ea89aa", "byte_length": 917},
    "tokenizer.json": {"sha256": "1dc27b18d6a518d0d5ff3f4bb7bd98521fe80ad39e5b2a246d4109f1bb9d5019", "byte_length": 24174},
}
PACKAGES = {"pix2tex": "0.1.4", "torch": "2.5.1+cpu", "torchvision": "0.20.1+cpu",
            "numpy": "1.26.4", "albumentations": "1.3.1", "opencv-python-headless": "4.10.0.84",
            "pandas": "2.2.3", "transformers": "4.49.0", "timm": "0.5.4", "x-transformers": "0.15.0",
            "tokenizers": "0.21.4", "Pillow": "13.0.0"}
POLICY = dict(device="cpu", seed=42, temperature=0.2, resize=True, max_seq_len=512,
              network_calls=0, paid_calls=0, automatic_downloads=False, clipboard=False)
LIMITS = dict(response_bytes=131072, diagnostic_bytes=65536, timeout_ms=120000, max_tex_bytes=65536)


def read(path, maximum=8 * 1024 * 1024):
    path = Path(path)
    guard_path(path, directory=False)
    assert path.stat().st_size <= maximum, "bounded audit file"
    return path.read_bytes()


def save(path, value):
    Path(path).write_bytes((json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode())


def inventory(root):
    root = Path(root)
    guard_path(root, directory=True)
    result = {}
    for p in sorted(root.rglob("*")):
        guard_path(p, directory=p.is_dir())
        if p.is_file():
            result[p.relative_to(root).as_posix()] = read(p)
    return result


def pins(files):
    return {name: fingerprint(raw) for name, raw in sorted(files.items())}


def bounded_int(value, maximum, allow_zero=False):
    assert type(value) is int and (0 if allow_zero else 1) <= value <= maximum, "bounded integer"


def fp(value):
    fields(value, "sha256 byte_length")
    assert type(value["sha256"]) is str and re.fullmatch(r"[0-9a-f]{64}", value["sha256"]), "SHA256"
    bounded_int(value["byte_length"], 1024 * 1024 * 1024, allow_zero=True)


def math_tokens(expression):
    """Validate full finite grammar, then unwrap single-operand script braces only.

    Fraction/group braces, operators, signs, variable names and script scope
    remain intact. No algebraic inference, Unicode repair or macro deletion.
    """
    values = lexical_tokens(expression)
    result, at = [], 0
    while at < len(values):
        if (values[at] in ("_", "^") and at + 3 < len(values)
                and values[at + 1] == "{" and values[at + 3] == "}"
                and re.fullmatch(r"[A-Za-z]|[0-9]|\\(?:alpha|beta|gamma)", values[at + 2])):
            result.extend((values[at], values[at + 2])); at += 4
        else:
            result.append(values[at]); at += 1
    return result


def semantics_schema(value):
    fields(value, SEMANTIC_FIELDS)
    exact(value["schema_version"], 1, "author schema")
    exact(value["producer"], "author_scan_declaration", "manual declaration provenance")
    expression = value["expression"]
    assert expression is None or type(expression) is str and 0 < len(expression.encode()) <= 65536, "manual expression"
    assert type(value["symbols"]) is list and len(value["symbols"]) <= 64, "bounded symbol count"
    for item in value["symbols"]:
        fields(item, "symbol definition unit"); display_text(item["symbol"])
        for field in ("definition", "unit"):
            if item[field] is not None: display_text(item[field])
    for key in ("conditions", "ranges", "exceptions"):
        items = value[key]
        assert type(items) is list and len(items) <= 16 and len(set(items)) == len(items), "bounded unique context"
        for item in items: display_text(item)
    assert type(value["uncertainties"]) is list and len(value["uncertainties"]) <= 16, "bounded uncertainty count"
    for issue in value["uncertainties"]:
        fields(issue, "kind description")
        assert issue["kind"] in ("sign", "subscript", "symbol"), "uncertainty category"
        display_text(issue["description"])


def png_pixels(raw):
    size = png_info(raw)
    at = 8
    while at < len(raw):
        length = int.from_bytes(raw[at:at+4], "big")
        assert raw[at+4:at+8] not in (b"acTL", b"fcTL", b"fdAT"), "no animated PNG control"
        at += length + 12
    assert max(size) <= 2048, "bounded PNG axes"
    with Image.open(io.BytesIO(raw)) as image:
        assert image.size == size and getattr(image, "n_frames", 1) == 1, "static decoded PNG"
        assert image.mode in ("1", "L", "LA", "P", "RGB", "RGBA"), "8-bit PNG pixels"
        converted = image.convert("RGBA")
        converted.load()
        return converted


def selected_source(root):
    root = Path(root)
    actual = inventory(root)
    case = strict_json(actual["case.json"]); fields(case, CASE_FIELDS)
    exact(case["schema_version"], 1, "case version"); exact(case["purpose"], "scan_formula", "case purpose")
    exact(case["source_kind"], "scanned_page", "actual selected image page")
    assert type(case["selection_id"]) is str and re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", case["selection_id"]), "selected identity"
    display_text(case["title"])
    fields(case["page"], "file sha256 page canvas")
    fields(case["region"], "file sha256 bounds legibility")
    fields(case["semantics"], "file sha256")
    bounded_int(case["page"]["page"], 1000000)
    assert type(case["page"]["canvas"]) is list and len(case["page"]["canvas"]) == 2, "page canvas"
    for axis in case["page"]["canvas"]: bounded_int(axis, 2048)
    assert type(case["region"]["bounds"]) is list and len(case["region"]["bounds"]) == 4, "crop rectangle"
    x, y, w, h = case["region"]["bounds"]
    for v in (x, y): bounded_int(v, 2048, allow_zero=True)
    for v in (w, h): bounded_int(v, 2048)
    assert x + w <= case["page"]["canvas"][0] and y + h <= case["page"]["canvas"][1], "crop stays within selected page"
    assert case["region"]["legibility"] in ("clear", "unreadable"), "known legibility state"
    refs = [("source-semantics.json", case["semantics"]), ("page.png", case["page"]), ("original.png", case["region"])]
    if case["pdf"] is not None:
        fields(case["pdf"], "file sha256"); refs.append(("original.pdf", case["pdf"]))
    names = ["case.json"]
    source = {"source-case.json": actual["case.json"]}
    for output, ref in refs:
        name = ref["file"]; safe_name(name)
        assert "/" not in name and "\\" not in name and not name.startswith("."), "ordinary direct selected file"
        assert name not in names, "distinct selected filenames"
        names.append(name)
        raw = actual[name]
        exact(fingerprint(raw)["sha256"], ref["sha256"], "selected attachment pin")
        source[output] = raw
    exact(set(actual), set(names), "exact selected case inventory")
    semantics = strict_json(source["source-semantics.json"]); semantics_schema(semantics)
    page, crop = png_pixels(source["page.png"]), png_pixels(source["original.png"])
    exact(list(page.size), case["page"]["canvas"], "actual whole-page dimensions")
    exact(crop.size, (w, h), "actual region dimensions")
    exact(page.crop((x, y, x+w, y+h)).tobytes(), crop.tobytes(), "actual region pixels belong to selected page")
    if case["pdf"] is not None: assert source["original.pdf"].startswith(b"%PDF-"), "selected archive PDF bytes"
    return case, semantics, source


def expectation(path):
    raw = read(path); expected = strict_json(raw)
    state = expected["author_case"]
    assert state in STATES, "fixed author state"
    frozen = read(FIXTURE / "frozen-sha256.json")
    exact(fingerprint(frozen)["sha256"], FROZEN_SHA, "fixed pre-recognition freeze")
    manifest = strict_json(frozen)
    exact(fingerprint(raw), manifest["files"]["expected-" + state + ".json"], "unchanged independent author expectation")
    return expected


def verify_fixture(root=FIXTURE):
    root = Path(root)
    frozen = read(root / "frozen-sha256.json")
    exact(fingerprint(frozen)["sha256"], FROZEN_SHA, "fixed pre-recognition freeze")
    manifest = strict_json(frozen)
    exact(manifest["author_formula_count"], 1, "one author formula")
    exact(manifest["scan_states"], 3, "three selected scan states")
    for name, pin in manifest["files"].items():
        assert not PurePosixPath(name).is_absolute() and ".." not in PurePosixPath(name).parts, "safe frozen artifact path"
        exact(fingerprint(read(root / name)), pin, "frozen author bytes " + name)
    for state in STATES:
        case, semantics, source = selected_source(root / "cases" / state)
        e = strict_json(read(root / ("expected-" + state + ".json")))
        author_match(case, semantics, source, e)
    return {"status": "passed", "author_formula_count": 1, "scan_states": 3, "frozen_files": len(manifest["files"])}


def author_match(case, semantics, source, expected):
    exact(case["page"]["page"], expected["physical_page"], "frozen physical page")
    exact(case["page"]["canvas"], expected["canvas"], "frozen page canvas")
    exact(case["region"]["bounds"], expected["bounds"], "frozen selected rectangle")
    exact(case["region"]["legibility"], expected["legibility"], "frozen key-mark legibility")
    for original, output in (("page.pdf", "original.pdf"), ("page.png", "page.png"), ("region.png", "original.png")):
        exact(fingerprint(source[output]), expected["originals"][original], "frozen actual source " + original)
    for key in ("symbols", "conditions", "ranges", "exceptions"):
        exact(semantics[key], expected[key], "independent author semantics " + key)
    if expected["expression"] is None:
        exact(semantics["expression"], None, "covered key mark remains unknown")
        assert semantics["uncertainties"], "unknown key-mark issue preserved"
        exact([i["kind"] for i in semantics["uncertainties"]], ["sign" if expected["author_case"] == "unreadable_sign" else "subscript"], "affected key-mark category")
        frozen_raw = read(FIXTURE / "frozen-sha256.json")
        exact(fingerprint(frozen_raw)["sha256"], FROZEN_SHA, "fixed unknown source freeze")
        name = "cases/" + expected["author_case"] + "/semantics.json"
        frozen_semantics = read(FIXTURE / name)
        exact(fingerprint(frozen_semantics), strict_json(frozen_raw)["files"][name], "frozen unknown mark declaration")
        exact(semantics["uncertainties"], strict_json(frozen_semantics)["uncertainties"], "literal original unknown-mark issue")
    else:
        exact(math_tokens(semantics["expression"]), expected["tokens"], "independent original math structure")
        exact(semantics["uncertainties"], [], "clear source has no invented uncertainty")


def absolute_saved_path(text, basename=None):
    assert type(text) is str and 0 < len(text) <= 32768 and "\0" not in text, "saved selected absolute path"
    if PureWindowsPath(text).is_absolute():
        path = PureWindowsPath(text)
    else:
        path = PurePosixPath(text)
        assert path.is_absolute(), "absolute path required"
    assert ".." not in path.parts, "saved path traversal"
    if basename is not None: exact(path.name, basename, "request selects only the held crop")
    return path


def same_saved_path(actual, selected):
    """Compare recorded explicit paths without following aliases or saved paths."""
    a, b = absolute_saved_path(actual), absolute_saved_path(selected)
    if isinstance(a, PureWindowsPath) or isinstance(b, PureWindowsPath):
        assert isinstance(a, PureWindowsPath) and isinstance(b, PureWindowsPath), "same recorded path platform"
        def parts(text):
            text = text.replace("/", "\\")
            if text.startswith("\\\\?\\UNC\\"): text = "\\\\"+text[8:]
            elif text.startswith("\\\\?\\"): text = text[4:]
            return tuple(p.casefold() for p in PureWindowsPath(text).parts)
        exact(parts(actual), parts(selected), "ordinary invocation selected exact crop path")
    else: exact(a.parts, b.parts, "ordinary invocation selected exact crop path")


def verify_request(request, request_id, crop, crop_name="region.png"):
    fields(request, "schema_version request_id input model_directory limits")
    exact(request["schema_version"], 1, "recognition request version")
    exact(request["request_id"], request_id, "recognition request identity")
    fields(request["input"], "path sha256 byte_length")
    absolute_saved_path(request["input"]["path"], crop_name)
    exact({k: request["input"][k] for k in ("sha256", "byte_length")}, fingerprint(crop), "request contains actual selected crop bytes")
    absolute_saved_path(request["model_directory"])
    exact(request["limits"], LIMITS, "fixed bounded one-image recognition limits")


def verify_response(response, request_id, crop):
    fields(response, RESPONSE_FIELDS)
    for key, value in (("schema_version", 1), ("producer", "pix2tex"), ("package_version", "0.1.4"), ("request_id", request_id)):
        exact(response[key], value, "actual response namespace " + key)
    exact(response["input"], fingerprint(crop), "raw recognition source binding")
    exact(response["model"], {"release": "v0.0.1", "files": MODEL_PINS}, "fixed model weights/config/tokenizer")
    exact(response["runtime"], {"python": "3.12.14", "packages": PACKAGES}, "fixed actual runtime declaration")
    exact(response["policy"], POLICY, "one selected offline recognition policy")
    exact(response["confidence"], None, "uncalibrated confidence stays unknown")
    bounded_int(response["elapsed_ms"], 120000, allow_zero=True)
    text = response["recognized_tex"]
    assert text is None or type(text) is str and 0 < len(text.encode()) <= 65536, "bounded raw recognition expression"
    assert type(response["issues"]) is list and len(response["issues"]) <= 16, "bounded recognition issues"
    for issue in response["issues"]:
        fields(issue, "code message"); display_text(issue["code"]); display_text(issue["message"])
    assert ((response["status"] == "recognized" and text is not None and not response["issues"])
            or (response["status"] == "failed" and text is None and response["issues"])), "real recognition status and fields agree"


def classify(case, semantics, profile, recognition):
    """Independently derive outcome; a guessed clear-counterpart answer cannot heal loss."""
    issues, rewrites = [], []
    value = semantics["expression"]
    if case["region"]["legibility"] == "unreadable": issues.append("SCAN_REGION_UNREADABLE")
    if semantics["uncertainties"]: issues.append("SCAN_CRITICAL_SYMBOL_UNKNOWN")
    if value is None: issues.append("SCAN_SEMANTICS_UNKNOWN")
    declared = recognized = None
    try:
        if value is not None: declared = math_tokens(value)
    except AssertionError: issues.append("SCAN_UNSUPPORTED_SYNTAX")
    if recognition is None or recognition["status"] != "recognized":
        issues.append("SCAN_RECOGNITION_FAILED")
    else:
        try: recognized = math_tokens(recognition["recognized_tex"])
        except AssertionError: issues.append("SCAN_UNSUPPORTED_RECOGNITION")
    if declared is not None and recognized is not None and declared != recognized:
        issues.append("SCAN_RECOGNITION_SEMANTIC_MISMATCH")
    names = set(t for t in (declared or []) if re.fullmatch(r"[A-Za-z]|\\(?:alpha|beta|gamma)", t))
    specified = [s["symbol"] for s in semantics["symbols"]]
    if declared is not None and (len(specified) != len(set(specified)) or set(specified) != names):
        issues.append("SCAN_SYMBOLS_MISMATCH")
    if declared is not None and any(s["definition"] is None for s in semantics["symbols"]):
        issues.append("SCAN_UNKNOWN_SYMBOL")
    issues = list(dict.fromkeys(issues))
    representation, fallback = None, []
    if not issues:
        measured = (profile["app"] == "Obsidian" and profile["app_version"] == "1.6.7"
                    and profile["mathjax_version"] == "3.2.2" and profile["verified"] and profile["native_math"])
        raw_tokens = lexical_tokens(recognition["recognized_tex"])
        uses_dfrac = "\\dfrac" in recognition["recognized_tex"]
        reason = "SCAN_NATIVE_UNAVAILABLE" if not measured else "SCAN_DFRAC_UNAVAILABLE" if uses_dfrac and not profile["compatibility_dfrac"] else None
        if reason is not None:
            if profile["local_images"]: representation = "formula_image"; fallback = [reason]
            else: issues.append("SCAN_IMAGE_UNAVAILABLE")
        else:
            representation = "native_math"
            if uses_dfrac: rewrites.append("SCAN_DFRAC_COMPATIBILITY")
            if raw_tokens != recognized: rewrites.append("SCAN_SINGLE_SCRIPT_GROUP_COMPATIBILITY")
    return dict(representation=representation, fallback_reasons=fallback, compatibility_rewrites=rewrites, issue_codes=issues), declared


def markdown_nodes(raw):
    """Inspect actual note grammar, literal labels and UTF-8 physical byte positions."""
    raw.decode("utf-8")
    nodes, at = [], 0
    while at < len(raw):
        end = raw.find(b"\n", at)
        assert end >= 0, "complete Markdown physical line"
        line = raw[at:end]
        if not line: at = end+1; continue
        fence = re.fullmatch(rb"(`{3,})text", line)
        if fence:
            start = end+1
            terminal = b"\n"+fence[1]+b"\n"
            finish = raw.find(terminal, start)
            assert finish >= 0, "literal text fence closes"
            nodes.append(dict(kind="literal", value=raw[start:finish].decode(), start=start, end=finish))
            at = finish+len(terminal); continue
        if line == b"$$":
            start = end+1; finish = raw.find(b"\n$$\n", start)
            assert finish >= 0, "display formula closes"
            nodes.append(dict(kind="math", value=raw[start:finish].decode(), start=start, end=finish))
            at = finish+len(b"\n$$\n"); continue
        heading = re.fullmatch(rb"(#{1,3}) (.+)", line)
        if heading:
            nodes.append(dict(kind="heading", level=len(heading[1]), value=heading[2].decode(), start=at, end=end))
        elif line == b"![[original.png]]":
            nodes.append(dict(kind="image", value=line.decode(), start=at, end=end))
        elif re.fullmatch(rb"\[[^\]\\\r\n]+\]\((?:original\.png|page\.png|raw-response\.json|recognized\.tex|original\.pdf)\)", line):
            nodes.append(dict(kind="link", value=line.decode(), start=at, end=end))
        else:
            assert line.decode() in ("符号：", "定义：", "单位：", "选定来源未列出。", "该表达由选定扫描区域重建，并经符号与前提核对；作者原始 TeX 未知。"), "unexpected user-visible Markdown content"
            nodes.append(dict(kind="label", value=line.decode(), start=at, end=end))
        at = end+1
    return nodes


def verify_note(raw, case, semantics, decision, recognized, compiled, file_report):
    nodes = markdown_nodes(raw)
    exact((nodes[0]["kind"], nodes[0]["level"], nodes[0]["value"]), ("heading", 1, "扫描公式预览"), "scan preview heading")
    exact([n["value"] for n in nodes if n["kind"] == "heading" and n["level"] == 2], ["标题", "公式", "识别原表达", "符号定义", "适用前提", "范围", "例外", "原始附件"], "actual complete note sections")
    sections, current = {}, None
    for node in nodes[1:]:
        if node["kind"] == "heading" and node["level"] == 2:
            current = node["value"]; sections[current] = []
        else:
            assert current is not None, "content belongs to an actual section"
            sections[current].append(node)
    mappings = []
    def mapped(node, kind, field):
        mappings.append(dict(kind=kind, source_field=field, start_byte=node["start"], end_byte=node["end"]))
    title = sections["标题"]
    exact([(n["kind"], n["value"]) for n in title], [("literal", case["title"])], "actual literal title")
    mapped(title[0], "title", "case.title")
    formula = sections["公式"]
    assert len(formula) == 2, "one actual expression and exact reconstruction attribution"
    exact((formula[1]["kind"], formula[1]["value"]), ("label", "该表达由选定扫描区域重建，并经符号与前提核对；作者原始 TeX 未知。"), "recovered expression is not claimed as author TeX")
    if decision["representation"] == "native_math":
        exact(formula[0]["kind"], "math", "actual MathJax display expression")
        exact(formula[0]["value"], compiled, "actual compiled text bytes")
        exact(math_tokens(formula[0]["value"]), math_tokens(semantics["expression"]), "actual display retains sign, denominator and every subscript")
        mapped(formula[0], "expression", "compiled_expression")
    else:
        exact((formula[0]["kind"], formula[0]["value"]), ("image", "![[original.png]]"), "actual selected original-image fallback")
        mapped(formula[0], "image_embed", "case.region")
    recovered = sections["识别原表达"]
    exact([(n["kind"], n["value"]) for n in recovered], [("literal", recognized)], "raw recognized expression preserved visibly")
    mapped(recovered[0], "recognized_expression", "recognition.recognized_tex")
    symbols = sections["符号定义"]
    assert len(symbols) == len(semantics["symbols"])*7, "complete adjacent symbol groups"
    for i, symbol in enumerate(semantics["symbols"]):
        group = symbols[i*7:i*7+7]
        exact([n["kind"] for n in group], ["heading", "label", "literal", "label", "literal", "label", "literal"], "actual symbol label/value adjacency")
        exact(group[0]["level"], 3, "symbol grouping")
        exact([n["value"] for n in group], ["符号 "+str(i+1), "符号：", symbol["symbol"], "定义：", symbol["definition"], "单位：", symbol["unit"] if symbol["unit"] is not None else "来源未给出单位"], "symbol-specific definition and unit, not flattened values")
        for at, key, kind in ((2, "symbol", "symbol"), (4, "definition", "definition"), (6, "unit", "unit")):
            mapped(group[at], kind, f"semantics.symbols[{i}].{key}")
    for section, key, kind in (("适用前提", "conditions", "condition"), ("范围", "ranges", "range"), ("例外", "exceptions", "exception")):
        actual = sections[section]
        if semantics[key]:
            exact([(n["kind"], n["value"]) for n in actual], [("literal", value) for value in semantics[key]], "all actual literal "+key)
            for i, node in enumerate(actual): mapped(node, kind, f"semantics.{key}[{i}]")
        else: exact([(n["kind"], n["value"]) for n in actual], [("label", "选定来源未列出。")], "no invented context")
    links = [("[原始公式区域](original.png)", "case.region"), ("[选定扫描页](page.png)", "case.page"), ("[识别原响应](raw-response.json)", "recognition"), ("[识别表达](recognized.tex)", "recognition.recognized_tex")]
    if case["pdf"] is not None: links.append(("[归档 PDF](original.pdf)", "case.pdf"))
    attachments = sections["原始附件"]
    exact([(n["kind"], n["value"]) for n in attachments], [("link", value) for value, _ in links], "actual complete local attachments")
    for node, (_, field) in zip(attachments, links): mapped(node, "attachment_link", field)
    exact(file_report["mappings"], mappings, "actual UTF-8 mapping inventory, scope and denominator")


def verify_output(case_root, profile_path, vault, expected_path, adapter_pin=None, runtime_pins=None):
    expected = expectation(expected_path)
    case, semantics, source = selected_source(case_root)
    author_match(case, semantics, source, expected)
    profile_raw = read(profile_path); profile = strict_json(profile_raw); profile_schema(profile)
    source["profile.json"] = profile_raw
    actual = inventory(vault)
    for name, raw in source.items(): exact(actual[name], raw, "actual original preservation "+name)
    report = strict_json(actual["scan.json"]); fields(report, REPORT_FIELDS)
    for key, value in COMMON.items(): exact(report[key], value, "unpublished engineering namespace "+key)
    uuid_string(report["request_id"])
    for key, value in (("case", case), ("semantics", semantics), ("profile", profile)):
        exact(report[key], value, "report actual selected input "+key)
    request = strict_json(actual["recognition-request.json"])
    verify_request(request, report["request_id"], source["original.png"], case["region"]["file"])
    exact(report["recognition_invocations"], 1, "one actual invocation, no hidden retry")
    exact(report["recognition_request_fingerprint"], fingerprint(actual["recognition-request.json"]), "actual saved crop-only request")
    recognition = report["recognition"]
    assert recognition is not None, "this bounded author audit requires a complete real response"
    raw_response = strict_json(actual["raw-response.json"])
    exact(recognition, raw_response, "raw recognition response unchanged by report")
    verify_response(raw_response, report["request_id"], source["original.png"])
    exact(report["recognition_response_fingerprint"], fingerprint(actual["raw-response.json"]), "actual original response fingerprint")
    assert len(actual["recognition-stderr.bin"]) <= 65536, "bounded preserved diagnostics"
    runtime = report["runtime_inputs"]
    exact(set(runtime), set(MODEL_PINS) | {"python", "adapter.py"}, "complete runtime input fingerprint inventory")
    for name, pin in MODEL_PINS.items(): exact(runtime[name], pin, "fixed actual model input "+name)
    fp(runtime["python"])
    if adapter_pin is None: adapter_pin = fingerprint(read(REPO / "sidecars/scan_formula/adapter.py"))
    exact(runtime["adapter.py"], adapter_pin, "same audited adapter bytes")
    if runtime_pins is not None: exact(runtime, runtime_pins, "independently captured actual runtime byte fingerprints")
    decision, declared = classify(case, semantics, profile, recognition)
    exact(report["decision"], decision, "actual independently derived semantic outcome")
    exact(report["tokens"], declared, "actual declared structure retains all key marks")
    ready = decision["representation"] is not None
    exact(report["status"], "preview_ready" if ready else "needs_review", "true preview status")
    exact(report["preview_status"], "saved" if ready else "blocked", "affected expression outcome")
    checks = report["checks"]; fields(checks, CHECK_FIELDS)
    for key in ("source_preservation", "region_match", "saved_output"): exact(checks[key], "passed", "actual byte/pixel gate "+key)
    exact(checks["recognition"], "passed" if recognition["status"] == "recognized" else "needs_review", "recognition success separate from semantic success")
    matched = False
    try: matched = declared is not None and declared == math_tokens(recognition["recognized_tex"])
    except (AssertionError, TypeError): pass
    exact(checks["expression_match"], "passed" if matched else "needs_review", "actual source/reconstruction semantic match")
    names = set(t for t in (declared or []) if re.fullmatch(r"[A-Za-z]|\\(?:alpha|beta|gamma)", t))
    symbols = [s["symbol"] for s in semantics["symbols"]]
    known = declared is not None and len(set(symbols)) == len(symbols) and set(symbols) == names and all(s["definition"] is not None for s in semantics["symbols"])
    exact(checks["symbol_definitions"], "passed" if known else "needs_review", "all actual variables defined")
    exact(checks["semantic_review"], "passed" if ready else "needs_review", "no unreviewed success")
    for key in ("note_plan", "rendering"): exact(checks[key], "passed" if ready else "not_run", "affected success artifacts gate "+key)
    exact(report["inputs"], pins(source), "full input preservation inventory")
    exact(strict_json(actual[".owner.json"]), dict(schema_version=1, producer="knowledge_scan_formula", request_id=report["request_id"], purpose="unpublished_scan_formula"), "new test root ownership")
    wanted = set(source) | {".owner.json", "scan.json", "scan.md", "recognition-request.json", "raw-response.json", "recognition-stderr.bin"}
    recognized = recognition["recognized_tex"]
    if recognized is not None:
        wanted.add("recognized.tex")
        exact(actual["recognized.tex"], recognized.encode(), "original recognized expression bytes even on failure")
    if ready:
        wanted |= {"plan.json", "扫描公式预览.md"}
        compiled = "".join(math_tokens(recognized)) if decision["representation"] == "native_math" else None
        if compiled is not None:
            wanted.add("compiled.tex"); exact(actual["compiled.tex"], compiled.encode(), "compiled expression from actual recognition, not expected")
        plan = strict_json(actual["plan.json"]); fields(plan, PLAN_FIELDS)
        exact(plan, dict(schema_version=1, producer="knowledge_scan_formula", scope="selected_scan_formula_v1", request_id=report["request_id"], case=case, semantics=semantics, decision=decision, tokens=declared, recognized_expression=recognized, compiled_expression=compiled), "entire successful plan grounded in actual recognition and source")
        exact(report["plan_fingerprint"], fingerprint(actual["plan.json"]), "actual saved plan fingerprint")
        assert len(report["files"]) == 1, "one selected formula note"
        file_report = report["files"][0]; fields(file_report, "relative_path content_sha256 byte_length mappings")
        exact(file_report["relative_path"], "扫描公式预览.md", "actual preview filename")
        note = actual[file_report["relative_path"]]
        exact(file_report["content_sha256"], fingerprint(note)["sha256"], "actual note SHA")
        exact(file_report["byte_length"], len(note), "actual note length")
        verify_note(note, case, semantics, decision, recognized, compiled, file_report)
    else:
        exact(report["plan_fingerprint"], None, "blocked scan has no successful plan")
        exact(report["files"], [], "blocked scan has no successful note")
    exact(set(actual), wanted, "complete inventory: no false-success or hidden redraw")
    exact(report["artifacts"], pins({name: data for name, data in actual.items() if name not in ("scan.json", "scan.md")}), "full actual output artifact fingerprints")
    markdown = actual["scan.md"].decode()
    payloads = re.findall(r"```json\n(.*?)\n```", markdown, re.S)
    assert len(payloads) == 1, "single readable report JSON payload"
    exact(strict_json(payloads[0].encode()), report, "complete JSON/Markdown same facts")
    outside = re.sub(r"```json\n.*?\n```", "<checked-report-payload>", markdown, flags=re.S)
    shell = "# 选定扫描公式重建预览报告\n\n状态："+report["status"]+"；预览："+report["preview_status"]+"；知识：pending；科学质量：not_run；发布：false。\n\n仅明确选定扫描区域的本地识别与语义核对；原表达并非作者 TeX；网络与付费调用：0。\n\n<checked-report-payload>\n"
    exact(outside, shell, "complete visible report scope, status and cost claims")
    return dict(status="passed", author_formula_count=1, author_state=expected["author_case"], representation=decision["representation"], issue_codes=decision["issue_codes"], original_bytes_unchanged=True, runtime_byte_binding="passed" if runtime_pins is not None else "not_run", scientific_quality="not_run")


def resynchronize_result(vault, rebase_mappings=True):
    """Attacker repair: refresh ordinary hashes and both reports after corruption.

    No expected data changes. Mapping positions are derived from actual note
    nodes, retaining the attacker-selected source labels so adjacency checks
    still have to defend the meaning of those labels.
    """
    vault = Path(vault)
    report = strict_json(read(vault / "scan.json"))
    for key, name in (("case", "source-case.json"), ("semantics", "source-semantics.json"), ("profile", "profile.json")):
        report[key] = strict_json(read(vault / name))
    report["inputs"] = pins({name: read(vault / name) for name in report["inputs"]})
    report["recognition_request_fingerprint"] = fingerprint(read(vault / "recognition-request.json"))
    if (vault / "raw-response.json").exists():
        report["recognition"] = strict_json(read(vault / "raw-response.json"))
        report["recognition_response_fingerprint"] = fingerprint(read(vault / "raw-response.json"))
    if (vault / "plan.json").exists():
        plan = strict_json(read(vault / "plan.json"))
        for key in ("case", "semantics", "decision"): plan[key] = report[key]
        save(vault / "plan.json", plan)
        report["plan_fingerprint"] = fingerprint(read(vault / "plan.json"))
    for file in report["files"]:
        raw = read(vault / file["relative_path"])
        file.update(content_sha256=fingerprint(raw)["sha256"], byte_length=len(raw))
        if rebase_mappings:
            mapped = [n for n in markdown_nodes(raw) if n["kind"] in ("literal", "math", "image", "link")]
            assert len(mapped) == len(file["mappings"]), "attacker must preserve/remove mapping inventory consistently"
            for item, node in zip(file["mappings"], mapped):
                item.update(start_byte=node["start"], end_byte=node["end"])
    report["artifacts"] = pins({name: data for name, data in inventory(vault).items() if name not in ("scan.json", "scan.md")})
    save(vault / "scan.json", report)
    shell = "# 选定扫描公式重建预览报告\n\n状态："+report["status"]+"；预览："+report["preview_status"]+"；知识：pending；科学质量：not_run；发布：false。\n\n"
    shell += "仅明确选定扫描区域的本地识别与语义核对；原表达并非作者 TeX；网络与付费调用：0。\n\n```json\n"
    (vault / "scan.md").write_bytes((shell+read(vault / "scan.json").decode()+"\n```\n").encode())


def file_pin(path):
    """Capture selected large executable/model bytes without a large allocation."""
    path = Path(path); guard_path(path, directory=False); before = path.stat()
    assert before.st_size <= 512 * 1024 * 1024, "bounded selected runtime file"
    state = lambda v: (v.st_dev, v.st_ino, v.st_size, v.st_mtime_ns)
    digest, count = hashlib.sha256(), 0
    with path.open("rb") as handle:
        exact(state(os.fstat(handle.fileno())), state(before), "runtime file unchanged before reading")
        while chunk := handle.read(1024 * 1024): digest.update(chunk); count += len(chunk)
        exact(state(os.fstat(handle.fileno())), state(before), "runtime file unchanged while reading")
    guard_path(path, directory=False)
    exact(state(path.stat()), state(before), "runtime file unchanged after reading")
    exact(count, before.st_size, "full selected runtime bytes")
    return {"sha256": digest.hexdigest(), "byte_length": count}


def actual_runtime_pins():
    python, model = os.environ.get("NOETOME_SCAN_PYTHON"), os.environ.get("NOETOME_SCAN_MODEL_DIR")
    assert python and model, "explicit real OCR interpreter/model environment required"
    assert Path(python).is_absolute() and Path(model).is_absolute(), "explicit absolute OCR runtime"
    model = Path(model); guard_path(model, directory=True)
    exact({p.name for p in model.iterdir()}, set(MODEL_PINS), "exact actual selected model directory")
    actual = {name: file_pin(model/name) for name in MODEL_PINS}
    exact(actual, MODEL_PINS, "independently captured actual fixed model bytes")
    actual["python"] = file_pin(python)
    actual["adapter.py"] = fingerprint(read(REPO/"sidecars/scan_formula/adapter.py"))
    return actual


def tamper_saved_outputs(case_root, profile_path, vault, expected_path, attack_root, runtime_pins=None, source_mode="saved ordinary CLI output"):
    """Mutate real saved CLI outputs, preserving every attempt and fresh hash shell."""
    attack_root = Path(attack_root)
    assert not attack_root.exists(), "brand-new mutation root required"
    attack_root.mkdir()
    attacks = []

    def attempt(name, action, reason):
        root = attack_root/name; root.mkdir()
        changed = root/"vault"; shutil.copytree(vault, changed)
        selected = root/"case"; shutil.copytree(case_root, selected)
        local_profile = root/"profile.json"; shutil.copyfile(profile_path, local_profile)
        action(selected, changed)
        caught = None
        try: verify_output(selected, local_profile, changed, expected_path, runtime_pins=runtime_pins)
        except AssertionError as error: caught = str(error)
        record = {"name": name, "expected_rejection": reason, "actual_rejection": caught,
                  "mode": source_mode+" mutation; no additional recognition", "files": pins(inventory(root))}
        save(root/"receipt.json", record)
        assert caught is not None and reason in caught, f"{name}: expected semantic rejection {reason!r}, got {caught!r}"
        attacks.append(record)

    def alter_response(selected, changed, mutation):
        response = strict_json(read(changed/"raw-response.json"))
        expression = "".join(math_tokens(response["recognized_tex"]))
        expression = mutation(expression)
        response["recognized_tex"] = expression; save(changed/"raw-response.json", response)
        (changed/"recognized.tex").write_bytes(expression.encode())
        plan = strict_json(read(changed/"plan.json")); plan["recognized_expression"] = expression
        plan["compiled_expression"] = expression; save(changed/"plan.json", plan)
        (changed/"compiled.tex").write_bytes(expression.encode())
        resynchronize_result(changed)

    for name, change in (("raw-minus-removed", lambda t:t.replace("=-", "=", 1)),
                         ("raw-left-subscript-changed", lambda t:t.replace("x_i", "x_j", 1)),
                         ("raw-numerator-subscript-changed", lambda t:t.replace("a_i", "a_j", 1)),
                         ("raw-denominator-subscript-changed", lambda t:t.replace("b_i", "b_j", 1)),
                         ("raw-fraction-swapped", lambda t:t.replace("a_i", "SWAP").replace("b_i", "a_i").replace("SWAP", "b_i"))):
        attempt(name, lambda selected, changed, change=change:alter_response(selected,changed,change), "derived semantic outcome")

    def display_change(selected, changed):
        p=changed/"扫描公式预览.md";p.write_bytes(read(p).replace(b"x_i=-",b"x_i=",1));resynchronize_result(changed)
    attempt("display-minus-removed", display_change, "actual compiled text bytes")

    def labels_swapped(selected, changed):
        p=changed/"扫描公式预览.md";text=read(p).decode().replace("定义：","交换：",1).replace("单位：","定义：",1).replace("交换：","单位：",1);p.write_bytes(text.encode());resynchronize_result(changed)
    attempt("literal-symbol-label-association", labels_swapped, "symbol-specific definition and unit")

    for name, key, kind in (("condition-removed","conditions","condition"),("range-removed","ranges","range"),("exception-removed","exceptions","exception")):
        def omit(selected,changed,key=key):
            report=strict_json(read(changed/"scan.json"));value=report["semantics"][key][0]
            p=changed/"扫描公式预览.md";p.write_bytes(read(p).replace(("```text\n"+value+"\n```\n\n").encode(),b"",1))
            report["files"][0]["mappings"]=[m for m in report["files"][0]["mappings"] if m["source_field"]!=f"semantics.{key}[0]"]
            save(changed/"scan.json",report);resynchronize_result(changed)
        attempt(name,omit,"all actual literal "+key)

    def unit_changed(selected,changed):
        c,s,_=selected_source(selected);s["symbols"][0]["unit"]="dimensionless";save(selected/c["semantics"]["file"],s)
        c["semantics"]["sha256"]=fingerprint(read(selected/c["semantics"]["file"]))["sha256"];save(selected/"case.json",c)
        (changed/"source-case.json").write_bytes(read(selected/"case.json"));(changed/"source-semantics.json").write_bytes(read(selected/c["semantics"]["file"]));resynchronize_result(changed)
    attempt("source-plan-reports-unit-sync",unit_changed,"independent author semantics symbols")

    def rect_changed(selected,changed):
        c=strict_json(read(selected/"case.json"));c["region"]["bounds"][0]+=1;save(selected/"case.json",c)
        (changed/"source-case.json").write_bytes(read(selected/"case.json"));resynchronize_result(changed)
    attempt("source-plan-reports-rectangle-sync",rect_changed,"region pixels")

    def wrong_crop(selected,changed):
        c=strict_json(read(selected/"case.json"));size=tuple(c["region"]["bounds"][2:]);Image.new("RGB",size,"white").save(selected/c["region"]["file"],format="PNG")
        raw=read(selected/c["region"]["file"]);c["region"]["sha256"]=fingerprint(raw)["sha256"];save(selected/"case.json",c)
        (changed/"original.png").write_bytes(raw);(changed/"source-case.json").write_bytes(read(selected/"case.json"))
        for name in ("recognition-request.json","raw-response.json"):
            q=strict_json(read(changed/name));q["input"].update(fingerprint(raw));save(changed/name,q)
        resynchronize_result(changed)
    attempt("crop-and-entire-pin-chain-replaced",wrong_crop,"region pixels")

    def false_report(selected,changed):
        p=changed/"scan.md";p.write_bytes(read(p)+"科学准确率100%。\n".encode())
    attempt("visible-report-false-scientific-claim",false_report,"complete visible report scope")

    def injected_input(selected,changed):
        q=strict_json(read(changed/"recognition-request.json"));q["semantics"]={"expression":"x_i=-a_i/b_i"};save(changed/"recognition-request.json",q);resynchronize_result(changed)
    attempt("request-injects-author-answer",injected_input,"schema fields")

    def hidden(selected,changed):
        (changed/"original.tex").write_bytes(b"guessed author source\n");resynchronize_result(changed)
    attempt("invented-original-tex",hidden,"complete inventory")
    save(attack_root/"summary.json", {"status":"passed","mutations":len(attacks),"records":attacks,"additional_recognition_calls":0})
    return attacks


def audit(cli, new_root):
    """Run one explicitly selected CLI and real runtime into isolated fresh Vaults."""
    fixture = verify_fixture()
    cli = Path(cli).absolute(); before_cli = file_pin(cli)
    root = Path(new_root).absolute();guard_path(root.parent,directory=True)
    assert not root.exists(), "audit output must not exist"
    root.mkdir()
    runtime = actual_runtime_pins();save(root/"runtime-pins.json",runtime)
    before_sources = pins({name: read(FIXTURE/name) for name in strict_json(read(FIXTURE/"frozen-sha256.json"))["files"]})
    records, valid = [], {}
    for name,state,profile_name in (("clear-native","clear","native"),("clear-original","clear","unverified"),("clear-no-target","clear","no-target"),("unknown-sign","unreadable_sign","native"),("unknown-subscript","unreadable_subscript","native")):
        selected=root/name;selected.mkdir();case=selected/"case";shutil.copytree(FIXTURE/"cases"/state,case)
        profile=selected/"profile.json"
        if profile_name=="no-target":
            value=strict_json(read(FIXTURE/"profiles/no-images.json"));value["native_math"]=False;save(profile,value)
        else: shutil.copyfile(FIXTURE/"profiles"/(profile_name+".json"),profile)
        sentinel=selected/"human-sentinel.bin";sentinel.write_bytes(b"human content outside the new test Vault\n")
        source_before=pins(inventory(case));sentinel_before=fingerprint(read(sentinel));command=[str(cli),"knowledge","scan-formula",str(case),str(profile),str(selected/"vault")]
        started=time.monotonic(); result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=180)
        (selected/"stdout.bin").write_bytes(result.stdout);(selected/"stderr.bin").write_bytes(result.stderr)
        record={"name":name,"author_state":state,"command":command,"exit_code":result.returncode,"wall_ms":round((time.monotonic()-started)*1000),"stdout":fingerprint(result.stdout),"stderr":fingerprint(result.stderr),"source_before":source_before,"source_after":pins(inventory(case)),"sentinel_before":sentinel_before,"sentinel_after":fingerprint(read(sentinel)),"output_exists":(selected/"vault").is_dir()}
        save(selected/"receipt.json",record)
        exact(record["source_before"],record["source_after"],"ordinary CLI source preservation")
        exact(record["sentinel_before"],record["sentinel_after"],"ordinary CLI human sentinel preservation")
        checked=verify_output(case,profile,selected/"vault",FIXTURE/("expected-"+state+".json"),runtime_pins=runtime)
        expected_representation = {"clear-native": "native_math", "clear-original": "formula_image"}.get(name)
        exact(checked["representation"],expected_representation,"frozen clear formula must really reconstruct; expected target outcome")
        report=strict_json(read(selected/"vault/scan.json"));exact(strict_json(result.stdout),report,"normal CLI stdout equals full saved report")
        exact(result.returncode,0 if checked["representation"] is not None else 3,"normal CLI actual semantic exit code")
        q=strict_json(read(selected/"vault/recognition-request.json"));same_saved_path(q["input"]["path"],str(case/report["case"]["region"]["file"]))
        record.update(independent=checked,files=pins({n:b for n,b in inventory(selected).items() if n!="receipt.json"}));save(selected/"receipt.json",record);records.append(record);valid[name]=(case,profile,selected/"vault",FIXTURE/("expected-"+state+".json"))
    # Record input faults separately: no synthetic provider and no fake recognition.
    faults=[]
    for name,edit in (("out-of-bounds",lambda c:c["region"].update(bounds=[799,175,560,200])),("wrong-crop-location",lambda c:c["region"]["bounds"].__setitem__(0,121)),("missing-nullable",lambda c:c.pop("pdf")),("extra-answer-field",lambda c:c.update(expected="forbidden")),("path-traversal",lambda c:c["semantics"].update(file="../outside.json")),("duplicate-attachment",lambda c:c["region"].update(file=c["page"]["file"],sha256=c["page"]["sha256"]))):
        selected=root/"faults"/name;selected.mkdir(parents=True);case=selected/"case";shutil.copytree(FIXTURE/"cases/clear",case);c=strict_json(read(case/"case.json"));edit(c);save(case/"case.json",c)
        command=[str(cli),"knowledge","scan-formula",str(case),str(FIXTURE/"profiles/native.json"),str(selected/"vault")]
        before=pins(inventory(case));result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=180)
        (selected/"stdout.bin").write_bytes(result.stdout);(selected/"stderr.bin").write_bytes(result.stderr)
        record={"name":name,"command":command,"exit_code":result.returncode,"stdout":fingerprint(result.stdout),"stderr":fingerprint(result.stderr),"output_not_created":not(selected/"vault").exists(),"selected_inputs_unchanged":before==pins(inventory(case)),"mode":"ordinary CLI invalid-input fault, separate from author/model-success count"};save(selected/"receipt.json",record)
        exact(result.returncode,2,"normal CLI invalid scan input rejected");assert record["output_not_created"] and record["selected_inputs_unchanged"],"fault cannot write/alter selected data";faults.append(record)
    mutations=tamper_saved_outputs(*valid["clear-native"],root/"mutations",runtime_pins=runtime)
    unknown=[]
    for name in ("unknown-sign","unknown-subscript"):
        case,profile,vault,expected=valid[name];target=root/(name+"-false-success");shutil.copytree(vault,target)
        good=valid["clear-native"][2];r=strict_json(read(target/"scan.json"));g=strict_json(read(good/"scan.json"))
        for filename in ("plan.json","compiled.tex","扫描公式预览.md"):shutil.copyfile(good/filename,target/filename)
        for key in ("status","preview_status","decision","tokens","checks","files"):r[key]=g[key]
        save(target/"scan.json",r);resynchronize_result(target);caught=None
        try:verify_output(case,profile,target,expected,runtime_pins=runtime)
        except AssertionError as error:caught=str(error)
        assert caught and "derived semantic outcome" in caught,"unknown key mark cannot inherit clear successful plan"
        unknown.append({"name":name,"actual_rejection":caught,"files":pins(inventory(target))})
    exact(file_pin(cli),before_cli,"fixed ordinary CLI bytes unchanged")
    exact(actual_runtime_pins(),runtime,"actual runtime/model bytes unchanged across entire audit")
    exact(pins({name: read(FIXTURE/name) for name in before_sources}),before_sources,"all frozen author files unchanged")
    summary={"status":"passed",**fixture,"cli":before_cli,"runtime_pins":runtime,"ordinary_cli_calls":len(records)+len(faults),"real_recognition_invocations_verified":len(records),"author_formula_count":1,"scan_states":3,"ordinary_author_target_presentations":len(records),"ordinary_input_faults":len(faults),"mutations":len(mutations)+len(unknown),"scenarios":len(records)+len(faults)+len(mutations)+len(unknown),"records":records,"faults":faults,"unknown_false_success":unknown,"frozen_author_bytes_unchanged":True,"scientific_quality":"not_run"}
    save(root/"summary.json",summary)
    return summary


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli",type=Path,help="explicit ordinary built CLI")
    parser.add_argument("--output",type=Path,help="brand-new isolated audit root")
    args=parser.parse_args()
    if args.cli is None and args.output is None: result=verify_fixture()
    else:
        assert args.cli is not None and args.output is not None,"--cli and --output must be provided together"
        result=audit(args.cli,args.output)
    print(json.dumps(result,ensure_ascii=False,indent=2))


if __name__=="__main__":main()
