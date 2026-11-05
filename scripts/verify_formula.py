"""Independent selected-formula audit against frozen, self-authored sources.

This checker does not import the Rust implementation or read expected as a
compiler input. Token grammar and literal source context are independently
checked, followed by actual Markdown math/image content, mappings and reports.
It neither performs OCR nor measures scientific correctness.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import uuid
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_extraction_knowledge import strict_json, fingerprint
from verify_knowledge_coordination import fields, exact, display_text, uuid_string, safe_name
from verify_knowledge_conflicts import guard_path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "fixtures/formula-fidelity"
FROZEN_SHA = "7b03260bbad80d2db3127e0e13ec9cfa7cc186a91c8081bf0ad0d3a8301bdd99"
COMMON = dict(schema_version=1, producer="knowledge_formula", scope="selected_formula_fidelity_v1",
              knowledge_status="pending", scientific_quality="not_run", published=False,
              network_calls=0, paid_calls=0)
CASE_FIELDS = "schema_version purpose selection_id title source_kind tex semantics region data_available"
SEMANTIC_FIELDS = "schema_version expression symbols conditions ranges exceptions"
PROFILE_FIELDS = "schema_version app app_version mathjax_version native_math compatibility_dfrac local_images verified"
EXPECTED_FIELDS = "schema_version producer author case_name scientific_quality knowledge_status published scope source_pins case semantics tokens profile_name representation issue_codes fallback_reasons compatibility_rewrites"
REPORT_FIELDS = "schema_version producer scope request_id status preview_status knowledge_status scientific_quality published network_calls paid_calls case semantics profile checks decision tokens plan_fingerprint inputs files artifacts"
PLAN_FIELDS = "schema_version producer scope request_id case semantics decision tokens compiled_expression"
COMMANDS = {"\\frac", "\\dfrac", "\\exp", "\\alpha", "\\beta", "\\gamma", "\\le", "\\ge"}
GREEK = {"\\alpha", "\\beta", "\\gamma"}


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


def text_list(value):
    assert type(value) is list and len(value) <= 16, "bounded context list"
    for item in value:
        display_text(item)
    assert len(set(value)) == len(value), "duplicate context"


def profile_schema(profile):
    fields(profile, PROFILE_FIELDS)
    exact(profile["schema_version"], 1, "profile version")
    for field in ("app", "app_version", "mathjax_version"):
        display_text(profile[field])
    for field in ("native_math", "compatibility_dfrac", "local_images", "verified"):
        assert type(profile[field]) is bool, "capability boolean"


def semantics_schema(semantics):
    fields(semantics, SEMANTIC_FIELDS)
    exact(semantics["schema_version"], 1, "semantic source version")
    expr = semantics["expression"]
    assert expr is None or type(expr) is str and 0 < len(expr.encode()) <= 65536, "declared expression"
    assert type(semantics["symbols"]) is list and len(semantics["symbols"]) <= 64, "symbol count"
    for symbol in semantics["symbols"]:
        fields(symbol, "symbol definition unit")
        name = symbol["symbol"]
        display_text(name)
        for key in ("definition", "unit"):
            if symbol[key] is not None:
                display_text(symbol[key])
    for field in ("conditions", "ranges", "exceptions"):
        text_list(semantics[field])


def tokens(expression):
    """Independent lexer; no Unicode lookalikes, hidden controls or guessed macros."""
    assert type(expression) is str and len(expression.encode()) <= 65536, "formula text"
    result = []
    at = 0
    while at < len(expression):
        c = expression[at]
        if c in " \t\r\n":
            at += 1
            continue
        if c == "\\":
            found = re.match(r"\\[A-Za-z]+", expression[at:])
            assert found and found[0] in COMMANDS, "unsupported or unknown macro"
            result.append("\\frac" if found[0] == "\\dfrac" else found[0])
            at += len(found[0])
        elif c.isascii() and c.isalpha():
            result.append(c); at += 1
        elif c.isascii() and c.isdigit():
            found = re.match(r"[0-9]+(?:\.[0-9]+)?", expression[at:])[0]
            result.append(found); at += len(found)
        elif c in "=+-*/(){}_^<>[]":
            result.append(c); at += 1
        else:
            raise AssertionError("unsupported formula character")
    assert 0 < len(result) <= 4096, "bounded nonempty formula tokens"
    Grammar(result).expression()
    return result


class Grammar:
    """Recursive expression validation preserves grouping, script scope and sign.

    Juxtaposition is multiplication. Equations/inequalities cannot have missing
    operands; fractions require two nonempty brace groups and exp an argument.
    This is an explicitly finite grammar, not a general TeX engine.
    """
    def __init__(self, values):
        self.values, self.at, self.depth = values, 0, 0

    def peek(self):
        return self.values[self.at] if self.at < len(self.values) else None

    def take(self):
        value = self.peek(); self.at += 1
        return value

    def expression(self):
        self.relation()
        assert self.at == len(self.values), "unconsumed formula tokens"

    def relation(self):
        self.sum()
        while self.peek() in ("=", "<", ">", "\\le", "\\ge"):
            self.take(); self.sum()

    def sum(self):
        self.product(True)
        while self.peek() in ("+", "-"):
            self.take(); self.product(False)

    def product(self, allow_sign):
        self.factor(allow_sign)
        while self.peek() is not None and self.peek() not in ("+", "-", "=", "<", ">", "\\le", "\\ge", ")", "}", "]"):
            if self.peek() in ("*", "/"):
                self.take()
                self.factor(True)
            else:
                self.factor(False)

    def factor(self, allow_sign):
        if allow_sign and self.peek() in ("+", "-"):
            self.take()
        self.primary()
        scripts = set()
        while self.peek() in ("_", "^"):
            op = self.take()
            assert op not in scripts, "duplicate script"
            scripts.add(op)
            if self.peek() == "{":
                self.group("{", "}")
            else:
                value = self.peek()
                assert value in GREEK or value is not None and re.fullmatch(r"[A-Za-z]|[0-9]", value), "script requires braced expression or a single symbol/digit"
                self.take()

    def group(self, left, right):
        self.depth += 1
        assert self.depth <= 64, "formula group nesting limit"
        assert self.take() == left, "group start"
        assert self.peek() != right, "empty formula group"
        self.relation()
        assert self.take() == right, "formula grouping mismatch"
        self.depth -= 1

    def primary(self):
        current = self.peek()
        assert current is not None, "missing formula operand"
        if current == "\\frac":
            self.take(); self.group("{", "}"); self.group("{", "}")
        elif current == "\\exp":
            self.take()
            opener = self.peek()
            assert opener in ("(", "{"), "exp requires a delimited argument"
            self.group(opener, ")" if opener == "(" else "}")
        elif current in ("(", "{", "["):
            self.group(current, {"(": ")", "{": "}", "[": "]"}[current])
        else:
            assert current in GREEK or re.fullmatch(r"[A-Za-z]|[0-9]+(?:\.[0-9]+)?", current), "invalid formula operand"
            self.take()


def png_info(raw):
    """Validate PNG framing, CRC and decoded scanline bytes without trusting header only."""
    assert raw.startswith(b"\x89PNG\r\n\x1a\n"), "PNG signature"
    at, chunks, data, width, height, depth, colour = 8, [], b"", None, None, None, None
    while at < len(raw):
        assert at + 12 <= len(raw), "PNG chunk framing"
        length = int.from_bytes(raw[at:at+4], "big"); name = raw[at+4:at+8]
        payload = raw[at+8:at+8+length]
        assert len(payload) == length and at + length + 12 <= len(raw), "PNG chunk bounds"
        crc = int.from_bytes(raw[at+8+length:at+12+length], "big")
        assert zlib.crc32(name + payload) == crc, "PNG CRC"
        chunks.append(name)
        if name == b"IHDR":
            assert len(chunks) == 1 and length == 13, "single first PNG IHDR"
            width = int.from_bytes(payload[:4], "big"); height = int.from_bytes(payload[4:8], "big")
            depth, colour = payload[8:10]
            assert 0 < width <= 2048 and 0 < height <= 2048, "image dimensions"
            assert payload[10:] == b"\x00\x00\x00", "bounded noninterlaced PNG"
        elif name == b"IDAT":
            data += payload
        elif name == b"IEND":
            assert length == 0 and at + 12 == len(raw), "PNG terminal boundary"
        at += length + 12
    assert chunks[-1] == b"IEND" and chunks.count(b"IHDR") == 1 and data, "complete PNG inventory"
    assert depth == 8 and colour in (0, 2, 4, 6), "selected fixture PNG format"
    stride = width * {0: 1, 2: 3, 4: 2, 6: 4}[colour] + 1
    budget = stride * height
    decoder = zlib.decompressobj()
    decoded = decoder.decompress(data, budget + 1)
    assert len(decoded) == budget, "PNG decompressed scanline extent"
    assert decoder.eof and not decoder.unconsumed_tail and not decoder.unused_data, "single complete bounded PNG compression stream"
    assert all(decoded[y * stride] <= 4 for y in range(height)), "PNG scanline filter"
    return width, height


def selected_source(root):
    root = Path(root)
    guard_path(root, directory=True)
    manifest_raw = read(root / "case.json")
    case = strict_json(manifest_raw)
    fields(case, CASE_FIELDS)
    exact(case["schema_version"], 1, "case version")
    exact(case["purpose"], "formula_fidelity", "case purpose")
    assert re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", case["selection_id"]), "selection ID"
    display_text(case["title"])
    assert case["source_kind"] in ("tex", "formula_region", "experimental_curve"), "source kind"
    assert type(case["data_available"]) is bool, "original curve data marker"
    chosen = {"source-case.json": manifest_raw}
    selected_names = {"case.json"}
    for field, output in (("tex", "original.tex"), ("semantics", "source-semantics.json"), ("region", "original.png")):
        declaration = case[field]
        if declaration is None:
            assert field != "semantics", "semantic source required"
            continue
        fields(declaration, "file sha256 page canvas bounds legibility" if field == "region" else "file sha256")
        safe_name(declaration["file"])
        assert "/" not in declaration["file"] and "\\" not in declaration["file"], "selected local basename only"
        assert declaration["file"] not in selected_names, "selected filename collision"
        selected_names.add(declaration["file"])
        raw = read(root / declaration["file"])
        exact(fingerprint(raw)["sha256"], declaration["sha256"], "original source fingerprint")
        chosen[output] = raw
        if field == "region":
            assert type(declaration["page"]) is int and 1 <= declaration["page"] <= 1000000, "physical page"
            assert type(declaration["canvas"]) is list and len(declaration["canvas"]) == 2, "canvas dimensions"
            assert type(declaration["bounds"]) is list and len(declaration["bounds"]) == 4, "selected region bounds"
            assert all(type(n) is int and n >= 0 for n in declaration["canvas"] + declaration["bounds"]), "integer coordinates"
            canvas_width, canvas_height = declaration["canvas"]
            assert 0 < canvas_width <= 10000000 and 0 < canvas_height <= 10000000, "bounded source canvas"
            x, y, width, height = declaration["bounds"]
            assert width > 0 and height > 0 and x + width <= canvas_width and y + height <= canvas_height, "selected region out of canvas"
            exact(list(png_info(raw)), [width, height], "original region dimensions")
            assert declaration["legibility"] in ("clear", "unreadable"), "declared region legibility"
    assert (case["tex"] is not None) == (case["source_kind"] == "tex"), "original TeX source kind"
    assert case["source_kind"] == "tex" or case["region"] is not None, "region source requires original"
    exact(set(p.name for p in root.iterdir()), selected_names, "only explicitly selected local source files")
    semantics = strict_json(chosen["source-semantics.json"])
    semantics_schema(semantics)
    return case, semantics, chosen


def classify(case, semantics, profile):
    """Independent source meaning gates; target capability never confirms meaning."""
    profile_schema(profile)
    if case["source_kind"] == "experimental_curve":
        return None, ["FORMULA_CURVE_UNSUPPORTED" if case["data_available"] else "FORMULA_CURVE_NO_DATA"], [], [], None
    if semantics["expression"] is None:
        return None, ["FORMULA_SEMANTICS_UNKNOWN"], [], [], None
    try:
        semantic_tokens = tokens(semantics["expression"])
    except AssertionError:
        return None, ["FORMULA_UNSUPPORTED_SYNTAX"], [], [], None
    issues = []
    names = set(t for t in semantic_tokens if re.fullmatch("[A-Za-z]", t) or t in GREEK)
    declared = {s["symbol"] for s in semantics["symbols"]}
    if names != declared or len(declared) != len(semantics["symbols"]):
        issues.append("FORMULA_SYMBOLS_MISMATCH")
    if any(s["definition"] is None for s in semantics["symbols"]):
        issues.append("FORMULA_UNKNOWN_SYMBOL")
    if case["tex"] is not None:
        # Caller passes original expression bytes in a private, verifier-only field.
        try:
            actual_tokens = tokens(case["_tex_bytes"].decode("utf-8"))
            if actual_tokens != semantic_tokens:
                issues.append("FORMULA_TEX_SEMANTIC_MISMATCH")
        except (UnicodeDecodeError, AssertionError):
            issues.append("FORMULA_UNSUPPORTED_SYNTAX")
    if issues:
        return None, issues, [], [], semantic_tokens
    reasons = []
    target_known = (profile["app"], profile["app_version"], profile["mathjax_version"]) == ("Obsidian", "1.6.7", "3.2.2")
    if case["tex"] is None:
        reasons.append("FORMULA_REGION_ONLY")
    elif not profile["verified"] or not profile["native_math"] or not target_known:
        reasons.append("FORMULA_NATIVE_UNAVAILABLE")
    elif "\\dfrac" in case["_tex_bytes"].decode() and not profile["compatibility_dfrac"]:
        reasons.append("FORMULA_DFRAC_UNAVAILABLE")
    if reasons:
        if case["region"] is None or not profile["local_images"]:
            return None, ["FORMULA_IMAGE_UNAVAILABLE"], [], [], semantic_tokens
        if case["region"]["legibility"] != "clear":
            return None, ["FORMULA_REGION_UNREADABLE"], [], [], semantic_tokens
        return "formula_image", [], reasons, [], semantic_tokens
    rewrites = ["FORMULA_DFRAC_COMPATIBILITY"] if "\\dfrac" in case["_tex_bytes"].decode() else []
    return "native_math", [], [], rewrites, semantic_tokens


def verify_fixture(root=FIXTURE):
    root = Path(root)
    frozen = read(root / "frozen-sha256.json")
    exact(fingerprint(frozen)["sha256"], FROZEN_SHA, "author manifest pin")
    manifest = strict_json(frozen)
    actual = inventory(root)
    exact(set(actual), set(manifest["files"]) | {"frozen-sha256.json"}, "frozen author inventory")
    exact(pins({k: v for k, v in actual.items() if k != "frozen-sha256.json"}), manifest["files"], "author originals")
    authors = strict_json(actual["authors.json"])
    for entry in authors["cases"]:
        expected = strict_json(actual[entry["expected_file"]])
        fields(expected, EXPECTED_FIELDS)
        exact({k: expected[k] for k in ("schema_version", "producer", "author", "scientific_quality", "knowledge_status", "published", "scope")},
              dict(schema_version=1, producer="formula_fidelity_expected", author="independent_self_authored_engineering_fixture",
                   scientific_quality="not_run", knowledge_status="pending", published=False, scope="selected_formula_only_not_ocr_or_science"), "author expected namespace")
        case, semantics, source = selected_source(root / entry["case_directory"])
        source_pins = pins(inventory(root / entry["case_directory"]))
        exact(source_pins, expected["source_pins"], "source pins to independent expected")
        exact(case, expected["case"], "source selection to author expected")
        exact(semantics, expected["semantics"], "semantic declarations to independent expected")
        private_case = copy.deepcopy(case)
        if case["tex"]:
            private_case["_tex_bytes"] = source["original.tex"]
        profile = strict_json(actual[entry["profile_file"]])
        representation, issues, fallback, rewrites, semantic_tokens = classify(private_case, semantics, profile)
        for key, value in dict(representation=representation, issue_codes=issues, fallback_reasons=fallback,
                               compatibility_rewrites=rewrites, tokens=semantic_tokens).items():
            exact(value, expected[key], "independent author " + entry["name"] + "/" + key)
    return authors


def expectation(path):
    path = Path(path)
    authors = verify_fixture(path.parent)
    assert any(e["expected_file"] == path.name for e in authors["cases"]), "selected frozen expectation"
    return strict_json(read(path))


def compiled_expression(case, profile, source):
    if case["tex"] is None:
        return None
    value = source["original.tex"].decode().strip(" \t\r\n")
    if profile["compatibility_dfrac"]:
        value = value.replace("\\dfrac", "\\frac")
    return value


def markdown_nodes(raw):
    """Read the actual restricted Markdown syntax with byte positions.

    Fenced literal text cannot create headings, embeds or links. Math and image
    expressions are parsed separately from metadata, rather than searched as
    incidental substrings in a report.
    """
    raw.decode("utf-8")
    nodes, at = [], 0
    while at < len(raw):
        end = raw.find(b"\n", at)
        assert end >= 0, "complete Markdown physical line"
        line = raw[at:end]
        if not line:
            at = end + 1
            continue
        fence = re.fullmatch(rb"(`{3,})text", line)
        if fence:
            start = end + 1
            terminal = b"\n" + fence[1] + b"\n"
            finish = raw.find(terminal, start)
            assert finish >= 0, "literal text fence closes"
            nodes.append(dict(kind="literal", value=raw[start:finish].decode(), start=start, end=finish))
            at = finish + len(terminal)
            continue
        if line == b"$$":
            start = end + 1
            finish = raw.find(b"\n$$\n", start)
            assert finish >= 0, "display formula closes"
            nodes.append(dict(kind="math", value=raw[start:finish].decode(), start=start, end=finish))
            at = finish + len(b"\n$$\n")
            continue
        heading = re.fullmatch(rb"(#{1,3}) (.+)", line)
        if heading:
            nodes.append(dict(kind="heading", level=len(heading[1]), value=heading[2].decode(), start=at, end=end))
        elif re.fullmatch(rb"!\[\[original\.png\]\]", line):
            nodes.append(dict(kind="image", value=line.decode(), start=at, end=end))
        elif re.fullmatch(rb"\[(?:[^\]\\\r\n]+)\]\((?:original\.tex|original\.png)\)", line):
            nodes.append(dict(kind="link", value=line.decode(), start=at, end=end))
        else:
            assert line.decode() in ("符号：", "定义：", "单位：", "选定来源未列出。"), "unexpected user-visible Markdown content"
            nodes.append(dict(kind="label", value=line.decode(), start=at, end=end))
        at = end + 1
    return nodes


def verify_note(raw, case, semantics, decision, expression, file_report):
    nodes = markdown_nodes(raw)
    assert nodes and nodes[0]["kind"] == "heading" and nodes[0]["level"] == 1 and nodes[0]["value"] == "公式预览", "preview title"
    names = ["标题", "公式", "符号定义", "适用前提", "范围", "例外", "原始附件"]
    headings = [n for n in nodes if n["kind"] == "heading" and n["level"] == 2]
    exact([n["value"] for n in headings], names, "actual semantic sections")
    sections = {}
    current = None
    for node in nodes[1:]:
        if node["kind"] == "heading" and node["level"] == 2:
            current = node["value"]; sections[current] = []
        else:
            assert current is not None, "content belongs to a semantic section"
            sections[current].append(node)
    expected_mappings = []
    def mapped(node, kind, field):
        expected_mappings.append(dict(kind=kind, source_field=field, start_byte=node["start"], end_byte=node["end"]))
    title = sections["标题"]
    assert len(title) == 1 and title[0]["kind"] == "literal", "title actual literal block"
    exact(title[0]["value"], case["title"], "author title visible"); mapped(title[0], "title", "case.title")
    formula = sections["公式"]
    assert len(formula) == 1, "one affected formula expression"
    if decision["representation"] == "native_math":
        assert formula[0]["kind"] == "math", "native expression must be actual display math"
        exact(tokens(formula[0]["value"]), tokens(semantics["expression"]), "actual display meaning including sign/scripts/grouping")
        exact(formula[0]["value"], expression, "actual compiler expression bytes")
        mapped(formula[0], "expression", "tex")
    else:
        assert decision["representation"] == "formula_image" and formula[0]["kind"] == "image", "fallback must embed actual original image"
        exact(formula[0]["value"], "![[original.png]]", "relative original PNG embed")
        mapped(formula[0], "image_embed", "region")
    symbols = sections["符号定义"]
    # Symbol/definition/unit association is part of meaning. Equal flattened
    # label and value lists can still assign a definition to the unit label.
    # Validate each actual adjacent group before deriving any source mapping.
    assert len(symbols) == len(semantics["symbols"]) * 7, "complete symbol groups"
    for i, symbol in enumerate(semantics["symbols"]):
        group = symbols[i*7:(i+1)*7]
        exact([n["kind"] for n in group], ["heading", "label", "literal", "label", "literal", "label", "literal"], "adjacent symbol field/value association")
        exact(group[0]["level"], 3, "symbol group heading level")
        exact([n["value"] for n in group], ["符号 " + str(i+1), "符号：", symbol["symbol"], "定义：", symbol["definition"], "单位：", symbol["unit"] if symbol["unit"] is not None else "来源未给出单位"], "actual symbol label/value association")
    literal_nodes = [n for n in symbols if n["kind"] == "literal"]
    values = []
    for symbol in semantics["symbols"]:
        values += [symbol["symbol"], symbol["definition"], symbol["unit"] if symbol["unit"] is not None else "来源未给出单位"]
    exact([n["value"] for n in literal_nodes], values, "every visible symbol/definition/unit")
    exact([n["value"] for n in symbols if n["kind"] == "label"], ["符号：", "定义：", "单位："] * len(semantics["symbols"]), "symbol labels")
    exact([n["value"] for n in symbols if n["kind"] == "heading"], ["符号 " + str(i+1) for i in range(len(semantics["symbols"]))], "symbol grouping")
    assert all(n["kind"] in ("literal", "heading", "label") for n in symbols), "symbol block node types"
    for i, symbol in enumerate(semantics["symbols"]):
        for j, (key, kind) in enumerate((("symbol", "symbol"), ("definition", "definition"), ("unit", "unit"))):
            mapped(literal_nodes[i*3+j], kind, "semantics.symbols[" + str(i) + "]." + key)
    for heading, key, kind in (("适用前提", "conditions", "condition"), ("范围", "ranges", "range"), ("例外", "exceptions", "exception")):
        actual = sections[heading]
        if semantics[key]:
            assert all(n["kind"] == "literal" for n in actual), "context literal nodes"
            exact([n["value"] for n in actual], semantics[key], "visible author " + key)
            for i, node in enumerate(actual):
                mapped(node, kind, "semantics." + key + "[" + str(i) + "]")
        else:
            exact([(n["kind"], n["value"]) for n in actual], [("label", "选定来源未列出。")], "no invented context")
    attachments = sections["原始附件"]
    expected_links = []
    if case["tex"]:
        expected_links.append(("[原始 TeX](original.tex)", "tex"))
    if case["region"]:
        expected_links.append(("[原始公式区域](original.png)", "region"))
    assert all(n["kind"] == "link" for n in attachments), "local original attachment links"
    exact([n["value"] for n in attachments], [value for value, _ in expected_links], "actual local attachment targets")
    for node, (_, field) in zip(attachments, expected_links):
        mapped(node, "attachment_link", field)
    exact(file_report["mappings"], expected_mappings, "exact content-to-source mapping denominator and byte ranges")


def verify_output(case_root, profile_path, vault, expected_path):
    expected = expectation(expected_path)
    case, semantics, source = selected_source(case_root)
    exact(case, expected["case"], "original selection equals frozen author")
    exact(semantics, expected["semantics"], "original metadata equals frozen author")
    exact(pins(inventory(case_root)), expected["source_pins"], "input source originals match expected")
    profile_raw = read(profile_path); profile = strict_json(profile_raw)
    profile_schema(profile)
    exact(profile, strict_json(read(Path(expected_path).parent / ("profile-" + expected["profile_name"] + ".json"))), "frozen selected capability profile")
    source["profile.json"] = profile_raw
    actual = inventory(vault)
    for name, raw in source.items():
        exact(actual[name], raw, "preserved original bytes " + name)
    report = strict_json(actual["formula.json"])
    fields(report, REPORT_FIELDS)
    for key, value in COMMON.items():
        exact(report[key], value, "unpublished engineering namespace " + key)
    uuid_string(report["request_id"])
    for key, value in (("case", case), ("semantics", semantics), ("profile", profile)):
        exact(report[key], value, "report binds actual source " + key)
    decision = dict(representation=expected["representation"], fallback_reasons=expected["fallback_reasons"],
                    compatibility_rewrites=expected["compatibility_rewrites"], issue_codes=expected["issue_codes"])
    exact(report["decision"], decision, "source-grounded expression outcome")
    exact(report["tokens"], expected["tokens"], "independent formula token structure")
    ready = expected["representation"] is not None
    exact(report["status"], "passed" if ready else "needs_review", "semantic status")
    exact(report["preview_status"], "rendered" if ready else "not_run", "affected preview status")
    checks = report["checks"]
    fields(checks, "source_preservation expression_match symbol_definitions semantic_review note_plan rendering saved_output")
    exact(checks["source_preservation"], "passed", "originals preserved")
    exact(checks["saved_output"], "passed", "saved bytes audited")
    semantic_tokens = expected["tokens"]
    symbol_names = set(t for t in (semantic_tokens or []) if re.fullmatch("[A-Za-z]", t) or t in GREEK)
    declared_names = {s["symbol"] for s in semantics["symbols"]}
    symbols_confirmed = semantic_tokens is not None and symbol_names == declared_names and len(declared_names) == len(semantics["symbols"]) and all(s["definition"] is not None for s in semantics["symbols"])
    expression_matches = semantic_tokens is not None
    if expression_matches and case["tex"]:
        try:
            expression_matches = tokens(source["original.tex"].decode()) == semantic_tokens
        except (AssertionError, UnicodeDecodeError):
            expression_matches = False
    exact(checks["expression_match"], "passed" if expression_matches else "needs_review", "actual structural source match")
    exact(checks["symbol_definitions"], "passed" if symbols_confirmed else "needs_review", "all and only source symbols explicitly confirmed")
    for key in ("note_plan", "rendering"):
        exact(checks[key], "passed" if ready else "not_run", "no false success " + key)
    exact(checks["semantic_review"], "passed" if ready else "needs_review", "semantic check")
    exact(report["inputs"], pins(source), "all input fingerprints")
    owner = strict_json(actual[".owner.json"])
    exact(owner, dict(schema_version=1, producer="knowledge_formula", request_id=report["request_id"], purpose="unpublished_formula"), "owned new test root")
    expected_inventory = set(source) | {".owner.json", "formula.json", "formula.md"}
    if ready:
        expected_inventory |= {"plan.json", "公式预览.md"}
        expression = compiled_expression(case, profile, source) if expected["representation"] == "native_math" else None
        if expression is not None:
            expected_inventory.add("compiled.tex")
            exact(actual["compiled.tex"], expression.encode(), "separate compiled TeX bytes")
        plan = strict_json(actual["plan.json"])
        fields(plan, PLAN_FIELDS)
        exact(plan, dict(schema_version=1, producer="knowledge_formula", scope="selected_formula_fidelity_v1",
                         request_id=report["request_id"], case=case, semantics=semantics, decision=decision,
                         tokens=expected["tokens"], compiled_expression=expression), "plan fully binds source and author meaning")
        exact(report["plan_fingerprint"], fingerprint(actual["plan.json"]), "actual saved plan")
        assert len(report["files"]) == 1, "one formula note file"
        file_report = report["files"][0]
        fields(file_report, "relative_path content_sha256 byte_length mappings")
        exact(file_report["relative_path"], "公式预览.md", "actual local preview filename")
        note = actual[file_report["relative_path"]]
        exact(file_report["content_sha256"], fingerprint(note)["sha256"], "actual note content hash")
        exact(file_report["byte_length"], len(note), "actual note byte length")
        verify_note(note, case, semantics, decision, expression, file_report)
    else:
        exact(report["plan_fingerprint"], None, "blocked case no successful plan")
        exact(report["files"], [], "blocked affected formula no note inventory")
    exact(set(actual), expected_inventory, "no unknown result/hidden redraw or falsely successful files")
    exact(report["artifacts"], pins({k: v for k, v in actual.items() if k not in ("formula.json", "formula.md")}), "full actual artifact inventory")
    markdown = actual["formula.md"].decode()
    matches = re.findall(r"```json\n(.*?)\n```", markdown, re.S)
    assert len(matches) == 1, "single readable dual-report payload"
    exact(strict_json(matches[0].encode()), report, "JSON Markdown dual-report agreement")
    summaries = re.findall(r"^状态：([^；\n]+)；预览：([^；\n]+)；知识：([^；\n]+)；科学质量：([^；\n]+)；发布：([^。\n]+)。$", markdown, re.M)
    exact(summaries, [(report["status"],report["preview_status"],"pending","not_run","false")], "dual report visible status agrees with actual meaning gates")
    outside_payload = re.sub(r"```json\n.*?\n```", "<checked-report-payload>", markdown, flags=re.S)
    expected_shell = "# 选定公式保真预览报告\n\n状态：" + report["status"] + "；预览：" + report["preview_status"] + "；知识：pending；科学质量：not_run；发布：false。\n\n仅选定公式/原区域工程核对；网络与付费调用：0。\n\n<checked-report-payload>\n"
    exact(outside_payload, expected_shell, "whole visible report shell cannot add false scope, fees or success")
    return dict(status="passed", case_name=expected["case_name"], representation=expected["representation"],
                original_bytes_unchanged=True, issue_codes=expected["issue_codes"], scientific_quality="not_run")


def resynchronize_result(vault):
    """Attacker utility: alter every ordinary hash/report after a mutation.

    This deliberately defeats checks based only on self-reported fingerprints.
    The frozen sources and their independent semantic expectation stay intact.
    """
    vault = Path(vault)
    report = strict_json(read(vault / "formula.json"))
    if (vault / "plan.json").exists():
        report["plan_fingerprint"] = fingerprint(read(vault / "plan.json"))
    for entry in report["files"]:
        raw = read(vault / entry["relative_path"])
        entry.update(content_sha256=fingerprint(raw)["sha256"], byte_length=len(raw))
    actual = inventory(vault)
    report["artifacts"] = pins({k: v for k, v in actual.items() if k not in ("formula.json", "formula.md")})
    save(vault / "formula.json", report)
    wire = read(vault / "formula.json").decode().rstrip("\n")
    markdown = read(vault / "formula.md").decode()
    markdown = re.sub(r"```json\n.*?\n```", lambda _: "```json\n" + wire + "\n```", markdown, flags=re.S)
    markdown = re.sub(r"^状态：[^\n]+$", lambda _: "状态：" + report["status"] + "；预览：" + report["preview_status"] +
                      "；知识：pending；科学质量：" + report["scientific_quality"] + "；发布：" + str(report["published"]).lower() + "。", markdown, flags=re.M)
    (vault / "formula.md").write_text(markdown, encoding="utf-8")


def misassociate_symbol_labels(vault):
    """Reviewer attack retaining each flattened value/label and exact mappings."""
    vault = Path(vault)
    p = vault / "公式预览.md"
    raw = read(p)
    before = "符号：\n\n```text\nC\n```\n\n定义：\n\n```text\ntest concentration\n```\n\n单位：\n\n".encode()
    after = "符号：\n\n定义：\n\n```text\nC\n```\n\n单位：\n\n```text\ntest concentration\n```\n\n".encode()
    assert before in raw, "real symbol association attack starts from actual baseline"
    altered = raw.replace(before,after,1)
    p.write_bytes(altered)
    old_nodes = markdown_nodes(raw);new_nodes = markdown_nodes(altered)
    report = strict_json(read(vault / "formula.json"))
    for mapping in report["files"][0]["mappings"]:
        matched = [n for n in old_nodes if n["start"]==mapping["start_byte"] and n["end"]==mapping["end_byte"]]
        assert len(matched)==1, "mapping identifies its original actual node"
        old = matched[0]
        ordinal=[n for n in old_nodes if n["kind"]==old["kind"]].index(old)
        node=[n for n in new_nodes if n["kind"]==old["kind"]][ordinal]
        exact(node["value"],old["value"],"attack preserves flattened values")
        mapping.update(start_byte=node["start"],end_byte=node["end"])
    save(vault / "formula.json",report)


def run_audit(cli, root):
    cli, root = Path(cli), Path(root)
    assert cli.is_absolute() and root.is_absolute(), "absolute ordinary CLI/audit paths"
    guard_path(cli, directory=False); guard_path(root.parent, directory=True)
    assert not root.exists(), "audit must use a new selected test root"
    authors = verify_fixture()
    root.mkdir()
    shutil.copytree(FIXTURE, root / "frozen-authors")
    for folder in ("commands", "vaults", "mutations", "guards", "source-pins"):
        (root / folder).mkdir()
    source_names = ["scripts/verify_formula.py", "tests/test_verify_formula.py"]
    source_names += [str(p.relative_to(REPO)) for p in sorted((REPO / "crates/noetome-cli/src").glob("formula*.rs"))]
    for name in source_names:
        target = root / "source-pins" / name
        target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(read(REPO / name))
    sentinel = root / "external-sentinel.txt"
    sentinel.write_bytes(b"User owned external content: do not edit.\r\n")
    baseline = dict(cli=str(cli), cli_fingerprint=fingerprint(read(cli, 256*1024*1024)),
                    python=sys.executable, platform=sys.platform, fixture_manifest=fingerprint(read(FIXTURE / "frozen-sha256.json")),
                    source_pins=pins(inventory(root / "source-pins")), sentinel=fingerprint(read(sentinel)))
    commands, scenarios = [], []
    def record(status="running"):
        save(root / "audit.json", dict(status=status, before=baseline, commands=commands, scenarios=scenarios))
    record()
    def command(label, args, codes, protected=()):
        def capture(p):
            p = Path(p)
            if p.is_dir():
                return pins(inventory(p))
            if sys.platform != "win32" and p.stat().st_nlink > 1:
                # This is the explicitly created external hardlink alias in
                # our own guard test, not a compiler source admission. Read
                # its bytes to prove preservation without admitting that
                # topology into selected_source or the output verifier.
                guard_path(p.parent, directory=True)
                assert not p.is_symlink() and p.is_file(), "known regular hardlink alias"
                return fingerprint(p.read_bytes())
            return fingerprint(read(p))
        before = {str(p): capture(p) for p in protected}
        before_sentinel = read(sentinel)
        argv = [str(cli), *map(str, args)]
        start = time.monotonic(); result = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
        prefix = root / "commands" / (str(len(commands)).zfill(3) + "-" + label)
        prefix.with_suffix(".stdout.bin").write_bytes(result.stdout)
        prefix.with_suffix(".stderr.bin").write_bytes(result.stderr)
        after = {str(p): capture(p) for p in protected}
        receipt = dict(label=label, argv=argv, exit_code=result.returncode, elapsed_seconds=time.monotonic()-start,
                       stdout=fingerprint(result.stdout), stderr=fingerprint(result.stderr), protected_before=before, protected_after=after,
                       sentinel_before=fingerprint(before_sentinel), sentinel_after=fingerprint(read(sentinel)))
        save(prefix.with_suffix(".receipt.json"), receipt); commands.append(receipt); record()
        exact(before, after, "readonly source and existing target " + label)
        exact(before_sentinel, read(sentinel), "external sentinel " + label)
        assert result.returncode in codes, label + " exit=" + str(result.returncode) + " " + result.stderr.decode(errors="replace")
        return result
    def rejected(label, function):
        try:
            function()
        except (AssertionError, ValueError, KeyError, OSError) as failure:
            scenarios.append(dict(name=label, status="rejected", reason=str(failure), exception=type(failure).__name__))
            record(); return
        raise AssertionError("independent checker accepted synchronized mutation " + label)
    frozen = root / "frozen-authors"
    by_name = {e["name"]: e for e in authors["cases"]}
    for entry in authors["cases"]:
        name = entry["name"]
        case = frozen / entry["case_directory"]; profile = frozen / entry["profile_file"]
        target = root / "vaults" / name; expected = frozen / entry["expected_file"]
        gold = strict_json(read(expected))
        result = command(name, ["knowledge", "formula", case, profile, target], {0} if gold["representation"] else {3}, (case, profile))
        checked = verify_output(case, profile, target, expected)
        exact(strict_json(result.stdout), strict_json(read(target / "formula.json")), "ordinary CLI stdout links saved report")
        scenarios.append(dict(name=name, status="passed", verification=checked)); record()
    native = by_name["native"]
    native_case = frozen / native["case_directory"]; native_profile = frozen / native["profile_file"]
    native_expected = frozen / native["expected_file"]
    def mutate(label, operation, source_name="native"):
        target = root / "mutations" / label
        shutil.copytree(root / "vaults" / source_name, target)
        operation(target)
        resynchronize_result(target)
        entry = by_name[source_name]
        rejected(label, lambda: verify_output(frozen / entry["case_directory"], frozen / entry["profile_file"], target, frozen / entry["expected_file"]))
    def text_replace(target, old, new):
        path = target / "公式预览.md"
        raw = read(path); assert old.encode() in raw, "mutation must affect original output"
        path.write_bytes(raw.replace(old.encode(), new.encode(), 1))
    for label, old, new in [
        ("negative-sign", "= -", "= +"), ("wrong-subscript", "C_i", "C_j"),
        ("wrong-superscript", "t^{2}", "t^{3}"),
        ("condition-loss", "a > 0 and b > 0", "a > 0"),
        ("unit-loss", "mmol/L", "kg"), ("range-loss", "0 <= t <= 10 s", "0 <= t <= 99 s"),
        ("definition-replaced", "positive nonzero divisor", "arbitrary divisor"),
        ("exception-loss", "Do not apply outside the selected time interval.", "No exceptions."),
        ("wrong-original-tex-link", "[原始 TeX](original.tex)", "[原始 TeX](outside.tex)")]:
        mutate(label, lambda t, old=old, new=new: text_replace(t, old, new))
    mutate("relational-sign", lambda t: text_replace(t, "\\le", "\\ge"), "native_relational")
    def replace_image(t):
        (t / "original.png").write_bytes(read(frozen / "unreadable_region/region.png"))
    mutate("original-image-replaced", replace_image, "image_fallback")
    mutate("image-path-traversal", lambda t: text_replace(t, "![[original.png]]", "![[../external.png]]"), "image_fallback")
    mutate("image-redrawn", lambda t: text_replace(t, "![[original.png]]", "```mermaid\nflowchart TD\nA-->B\n```"), "image_fallback")
    def changed_original(t):
        p = t / "original.tex"; p.write_bytes(read(p).replace(b"= -", b"= +"))
    mutate("preserved-tex-replaced", changed_original)
    def corrupt_mapping(t):
        r = strict_json(read(t / "formula.json"));r["files"][0]["mappings"][0]["start_byte"] += 1;save(t / "formula.json", r)
    mutate("mapping-out-of-range", corrupt_mapping)
    def remove_mapping(t):
        r = strict_json(read(t / "formula.json"));r["files"][0]["mappings"].pop();save(t / "formula.json", r)
    mutate("mapping-denominator-loss", remove_mapping)
    mutate("symbol-label-association", misassociate_symbol_labels)
    def fake_report_body(t):
        p=t/"formula.md";raw=read(p);original="仅选定公式/原区域工程核对；网络与付费调用：0。".encode();assert original in raw
        p.write_bytes(raw.replace(original,"科学质量：passed；所有扫描公式已准确恢复；发布：true；网络与付费调用：2。".encode(),1))
    mutate("false-visible-report-body",fake_report_body)
    def fake_published(t):
        r = strict_json(read(t / "formula.json"));r["published"] = True;r["scientific_quality"] = "passed";save(t / "formula.json", r)
    mutate("fake-scientific-publication", fake_published)
    def fake_success(t):
        r = strict_json(read(t / "formula.json"));r["status"] = "passed";r["preview_status"] = "rendered";r["decision"]["representation"] = "formula_image";r["decision"]["issue_codes"] = [];save(t / "formula.json", r)
    for source_name in ("unknown_symbol", "unreadable_region", "unknown_macro", "curve_no_data"):
        mutate("false-success-" + source_name, fake_success, source_name)
    def fake_individual_check(t, key):
        r = strict_json(read(t / "formula.json"));r["checks"][key] = "passed";save(t / "formula.json",r)
    mutate("false-symbol-definition-check", lambda t: fake_individual_check(t,"symbol_definitions"), "unknown_symbol")
    mutate("false-expression-match-check", lambda t: fake_individual_check(t,"expression_match"), "expression_mismatch")
    def curve_redraw(t):
        (t / "fake-curve.csv").write_bytes(b"time,response\n0,0\n1,100\n")
    mutate("fabricated-curve-data", curve_redraw, "curve_no_data")
    # Change the real source, metadata and their declared pins together. The
    # compiler can truthfully compile this new declaration; independent frozen
    # author truth must still refuse to call it the original successful case.
    def forged_source(label, change):
        folder=root/"guards"/label;folder.mkdir();source=folder/"case";shutil.copytree(native_case,source)
        sem=strict_json(read(source/"semantics.json"));tex=read(source/"original.tex")
        tex,sem=change(tex,sem)
        (source/"original.tex").write_bytes(tex);save(source/"semantics.json",sem)
        c=strict_json(read(source/"case.json"));c["tex"]["sha256"]=fingerprint(tex)["sha256"];c["semantics"]["sha256"]=fingerprint(read(source/"semantics.json"))["sha256"];save(source/"case.json",c)
        target=folder/"vault"
        command(label,["knowledge","formula",source,native_profile,target],{0},(source,native_profile))
        rejected(label,lambda:verify_output(source,native_profile,target,native_expected))
    def flip_sign(tex,sem):
        sem["expression"]=sem["expression"].replace("= -","= +");return tex.replace(b"= -",b"= +"),sem
    forged_source("fully-synchronized-negative-source",flip_sign)
    def flip_subscript(tex,sem):
        sem["expression"]=sem["expression"].replace("C_i","C_j")
        for s in sem["symbols"]:
            if s["symbol"]=="i":s["symbol"]="j"
        return tex.replace(b"C_i",b"C_j"),sem
    forged_source("fully-synchronized-subscript-source",flip_subscript)
    def lose_condition(tex,sem):sem["conditions"]=[];return tex,sem
    forged_source("fully-synchronized-condition-source",lose_condition)
    def lose_unit(tex,sem):sem["symbols"][0]["unit"]=None;return tex,sem
    forged_source("fully-synchronized-unit-source",lose_unit)
    # Invalid source declarations must fail before creating any test Vault.
    def guard(label, change, profile_change=None):
        folder = root / "guards" / label; folder.mkdir()
        source = folder / "case"; shutil.copytree(native_case, source)
        profile = folder / "profile.json"; profile.write_bytes(read(native_profile))
        change(source)
        if profile_change:
            profile_change(profile)
        target = folder / "vault"
        command(label, ["knowledge", "formula", source, profile, target], {2, 3}, (source, profile))
        assert not target.exists(), "invalid source created success/test root " + label
        scenarios.append(dict(name=label, status="rejected_before_output")); record()
    def edit_manifest(source, callback):
        p = source / "case.json"; value = strict_json(read(p));callback(value);save(p, value)
    guard("original-sha-mismatch", lambda s: (s / "original.tex").write_bytes(read(s / "original.tex") + b" "))
    guard("region-out-of-canvas", lambda s: edit_manifest(s, lambda c: c["region"].update(bounds=[900,120,800,240])))
    guard("region-dimension-mismatch", lambda s: edit_manifest(s, lambda c: c["region"].update(bounds=[100,120,700,240])))
    def corrupt_png(s):
        p = s / "region.png";raw=bytearray(read(p));raw[50] ^= 0xff;p.write_bytes(raw)
        edit_manifest(s, lambda c: c["region"].update(sha256=fingerprint(raw)["sha256"]))
    guard("png-corrupted-with-new-sha", corrupt_png)
    guard("input-traversal", lambda s: edit_manifest(s, lambda c: c["tex"].update(file="../external-sentinel.txt")))
    guard("input-absolute-path", lambda s: edit_manifest(s, lambda c: c["tex"].update(file=str(sentinel))))
    guard("selected-filename-collision", lambda s: edit_manifest(s, lambda c: c["tex"].update(file="region.png", sha256=c["region"]["sha256"])))
    guard("required-nullable-missing", lambda s: edit_manifest(s, lambda c: c.pop("region")))
    guard("unknown-schema-field", lambda s: edit_manifest(s, lambda c: c.update(allow_redraw=True)))
    occupied = root / "guards" / "occupied-vault"; occupied.mkdir();(occupied / "manual.md").write_bytes(b"Manual notes\n")
    command("occupied-vault", ["knowledge", "formula", native_case, native_profile, occupied], {2,3}, (native_case,native_profile,occupied))
    scenarios.append(dict(name="occupied-vault", status="rejected_unchanged")); record()
    if sys.platform != "win32":
        for label, mode in (("input-file-symlink", "file"), ("input-file-hardlink", "hardlink")):
            folder=root/"guards"/label;folder.mkdir();source=folder/"case";shutil.copytree(native_case,source)
            p=source/"original.tex";raw=read(p);p.unlink();external=folder/"external.tex";external.write_bytes(raw)
            if mode=="file":p.symlink_to(external)
            else:import os;os.link(external,p)
            target=folder/"vault"
            command(label,["knowledge","formula",source,native_profile,target],{2,3},(external,native_profile))
            assert not target.exists(),"linked input created output"
            scenarios.append(dict(name=label,status="rejected_before_output"));record()
        real = root / "guards" / "real-parent";real.mkdir()
        link = root / "guards" / "linked-parent";link.symlink_to(real,target_is_directory=True)
        target=link/"new-vault"
        command("output-ancestor-symlink",["knowledge","formula",native_case,native_profile,target],{2,3},(native_case,native_profile,real))
        assert not target.exists();scenarios.append(dict(name="output-ancestor-symlink",status="rejected_before_output"));record()
        linked_case=root/"guards"/"linked-case";linked_case.symlink_to(native_case,target_is_directory=True)
        target=root/"guards"/"input-ancestor-vault"
        command("input-ancestor-symlink",["knowledge","formula",linked_case,native_profile,target],{2,3},(native_case,native_profile))
        assert not target.exists();scenarios.append(dict(name="input-ancestor-symlink",status="rejected_before_output"));record()
    else:
        scenarios.append(dict(name="Unix-symlink-matrix",status="not_run",reason="Windows native guards recorded separately"))
    exact(read(sentinel), b"User owned external content: do not edit.\r\n", "final external sentinel")
    exact(baseline["cli_fingerprint"], fingerprint(read(cli,256*1024*1024)), "audit uses one stable CLI")
    verify_fixture(frozen)
    record("passed")
    return dict(status="passed", original_cases=15, cli_commands=len(commands), scenarios=len(scenarios),
                source_manifest=FROZEN_SHA, output=str(root), scientific_quality="not_run")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("fixture")
    check = sub.add_parser("check")
    check.add_argument("case", type=Path); check.add_argument("profile", type=Path)
    check.add_argument("vault", type=Path); check.add_argument("expected", type=Path)
    audit = sub.add_parser("audit")
    audit.add_argument("cli", type=Path); audit.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.command == "fixture":
        print(json.dumps(dict(status="passed", cases=len(verify_fixture()["cases"])), ensure_ascii=False))
    elif args.command == "check":
        print(json.dumps(verify_output(args.case, args.profile, args.vault, args.expected), ensure_ascii=False))
    else:
        print(json.dumps(run_audit(args.cli, args.output), ensure_ascii=False))


if __name__ == "__main__":
    main()
