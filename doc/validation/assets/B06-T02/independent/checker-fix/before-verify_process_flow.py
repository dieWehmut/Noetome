"""Independent bounded process-source / flow audit from frozen author text.

The parser below reads only actual selected source text. It never imports the
Rust parser, accepts candidate graphs, or uses a saved report as its source of
truth. Engineering fixtures and Markdown read-back do not measure science or
claim to execute Obsidian. CLI receipts retain original bytes, including errors.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import heapq
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import unicodedata
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_extraction_knowledge import actual_source, strict_json, fingerprint
from verify_knowledge_coordination import (fields, exact, display_text, identifier,
                                          uuid_string, safe_name)
from verify_knowledge_conflicts import guard_path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "fixtures/process-flow-v1"
FROZEN_SHA = "adadba19effe8c7a3cffb688d04dd94a2632b38bb6d473a17aea0767f66bab11"
KINDS = "sequence branch causes promotes inhibits correlates unknown".split()
DOC_FIELDS = "process steps relations evidence"
REVIEW_FIELDS = "schema_version producer scope request_id binding selection status knowledge_status scientific_quality research_status published network_calls paid_calls checks issue_codes unparsed_statements parsed_document totals expected_totals gold_sha256 ir_fingerprint inputs artifacts"
FLOW_FIELDS = "schema_version producer scope request_id binding status preview_status knowledge_status scientific_quality research_status published network_calls paid_calls checks issue_codes issues process_document profile representation decision review_ir_fingerprint plan_fingerprint totals inputs files artifacts"
COMMON = dict(knowledge_status="pending", scientific_quality="not_run",
              research_status="not_requested", published=False, network_calls=0, paid_calls=0)


def encoded(value):
    """The declared output wire codec has no final newline."""
    return json.dumps(value, ensure_ascii=False, indent=2).encode("utf-8")


def save(path, value):
    Path(path).write_bytes(encoded(value) + b"\n")


def read(path):
    path = Path(path)
    guard_path(path, directory=False)
    assert path.stat().st_size <= 16 * 1024 * 1024, "bounded audit input"
    return path.read_bytes()


def tree(root):
    root = Path(root)
    guard_path(root, directory=True)
    result = {}
    for path in sorted(root.rglob("*")):
        guard_path(path, directory=path.is_dir())
        if path.is_file():
            result[path.relative_to(root).as_posix()] = read(path)
    return result


def pins(files):
    return {name: fingerprint(raw) for name, raw in sorted(files.items())}


def finite_list(value, *, ids=False):
    assert type(value) is list and len(value) <= 8, "bounded required array"
    for item in value:
        (identifier if ids else display_text)(item)
    assert len(set(value)) == len(value), "duplicate literal array item"


def normalize(document):
    value = copy.deepcopy(document)
    for key, ident in (("steps", "id"), ("relations", "id"), ("evidence", "object_id")):
        value[key].sort(key=lambda item: item[ident])
    return value


def document_schema(document):
    fields(document, DOC_FIELDS)
    p = document["process"]
    fields(p, "title entry goals conditions exceptions")
    display_text(p["title"]); identifier(p["entry"])
    finite_list(p["goals"], ids=True)
    for key in ("conditions", "exceptions"):
        finite_list(p[key])
    assert type(document["steps"]) is list and 2 <= len(document["steps"]) <= 12, "node bounds"
    assert type(document["relations"]) is list and len(document["relations"]) <= 24, "relation bounds"
    ids = {"process"}
    for s in document["steps"]:
        fields(s, "id kind label description criterion inputs outputs conditions exceptions")
        identifier(s["id"])
        assert s["id"] not in ids, "duplicate object ID"
        ids.add(s["id"])
        assert s["kind"] in ("step", "decision"), "step kind"
        display_text(s["label"]); display_text(s["description"])
        if s["kind"] == "step":
            assert s["criterion"] is None, "step criterion must be null"
        elif s["criterion"] is not None:
            display_text(s["criterion"])
        for key in ("inputs", "outputs", "conditions", "exceptions"):
            finite_list(s[key])
    node_ids = ids - {"process"}
    node_kinds = {s["id"]: s["kind"] for s in document["steps"]}
    assert p["entry"] in node_ids and set(p["goals"]) <= node_ids, "process endpoint"
    for r in document["relations"]:
        fields(r, "id left right kind branch_outcome conditions exceptions")
        identifier(r["id"])
        assert r["id"] not in ids, "duplicate object ID"
        ids.add(r["id"])
        assert r["left"] in node_ids and r["right"] in node_ids, "relation endpoint"
        assert r["kind"] in KINDS, "relation kind"
        assert (type(r["branch_outcome"]) is bool if r["kind"] == "branch" else
                r["branch_outcome"] is None), "relation nullable outcome"
        assert r["kind"] != "branch" or node_kinds[r["left"]] == "decision", "branch source must be a decision"
        for key in ("conditions", "exceptions"):
            finite_list(r[key])
    assert type(document["evidence"]) is list and len(document["evidence"]) == len(ids), "evidence denominator"
    assert {e["object_id"] for e in document["evidence"]} == ids, "one evidence per object"
    for e in document["evidence"]:
        fields(e, "object_id start_byte end_byte quote")
        assert all(type(e[k]) is int for k in ("start_byte", "end_byte")), "byte offset type"
        assert 0 <= e["start_byte"] < e["end_byte"] <= 65536, "evidence bounds"
        assert type(e["quote"]) is str and not any(c in e["quote"] for c in "\r\n"), "full physical line evidence"
        assert e["end_byte"] - e["start_byte"] == len(e["quote"].encode()), "evidence byte length"
    intervals = sorted((e["start_byte"], e["end_byte"]) for e in document["evidence"])
    assert all(left[1] <= right[0] for left, right in zip(intervals, intervals[1:])), "evidence overlap"
    ordered = sorted(document["evidence"], key=lambda e: e["start_byte"])
    assert ordered[0]["object_id"] == "process" and ordered[0]["start_byte"] == 0, "process evidence first"
    relation_stage = False
    for e in ordered[1:]:
        if e["object_id"] in node_ids:
            assert not relation_stage, "step evidence before relations"
        else:
            relation_stage = True


class Sentence:
    """Exact Chinese grammar with JSON-decoder token boundaries."""
    def __init__(self, line):
        self.line, self.at = line, 0

    def take(self, literal):
        assert self.line.startswith(literal, self.at), "source literal"
        self.at += len(literal)

    def ident(self):
        match = re.match(r"[a-z][a-z0-9_]{0,31}", self.line[self.at:])
        assert match, "source ID"
        self.at += len(match[0])
        return match[0]

    def value(self, kind):
        decoder = json.JSONDecoder(parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
        value, end = decoder.raw_decode(self.line, self.at)
        self.at = end
        if kind == "strings":
            finite_list(value)
        elif kind == "ids":
            finite_list(value, ids=True)
        elif value is not None or kind != "criterion":
            display_text(value)
        return value

    def suffix(self, keys):
        labels = dict(description="说明", inputs="输入", outputs="输出", conditions="条件", exceptions="例外")
        result = {}
        for key in keys:
            self.take("。" + labels[key] + "：")
            result[key] = self.value("string" if key == "description" else "strings")
        self.take("。")
        assert self.at == len(self.line), "unconsumed source suffix"
        return result


def parse_statement(line):
    c = Sentence(line)
    if line.startswith("过程："):
        c.take("过程："); title = c.value("string")
        c.take("。入口："); entry = c.ident()
        c.take("。目标："); goals = c.value("ids")
        p = dict(title=title, entry=entry, goals=goals, **c.suffix(["conditions", "exceptions"]))
        return "process", "process", p
    for token, kind in (("步骤 ", "step"), ("判定 ", "decision")):
        if line.startswith(token):
            c.take(token); ident = c.ident(); c.take("："); label = c.value("string")
            criterion = None
            if kind == "decision":
                c.take("。判据："); criterion = c.value("criterion")
            rest = c.suffix(["description", "inputs", "outputs", "conditions", "exceptions"])
            node = dict(id=ident, kind=kind, label=label, description=rest.pop("description"),
                        criterion=criterion, **rest)
            return "steps", ident, node
    c.take("关系 "); ident = c.ident(); c.take("："); left = c.ident()
    variants = [(" 完成后执行 ", "sequence", None), (" 的判据为真时执行 ", "branch", True),
                (" 的判据为假时执行 ", "branch", False), (" 导致 ", "causes", None),
                (" 促进 ", "promotes", None), (" 抑制 ", "inhibits", None)]
    for token, kind, outcome in variants:
        if line.startswith(token, c.at):
            c.take(token); right = c.ident()
            break
    else:
        c.take(" 与 "); right = c.ident(); outcome = None
        if line.startswith(" 相关", c.at):
            c.take(" 相关"); kind = "correlates"
        else:
            c.take(" 的方向未说明"); kind = "unknown"
    rel = dict(id=ident, left=left, right=right, kind=kind, branch_outcome=outcome,
               **c.suffix(["conditions", "exceptions"]))
    return "relations", ident, rel


def parse_source(raw):
    assert type(raw) is bytes and 0 < len(raw) <= 65536, "selected source bound"
    text = raw.decode("utf-8")
    lines = text.removesuffix("\n").split("\n")
    partial = dict(process=None, steps=[], relations=[], evidence=[])
    unparsed, phase, byte_at = [], 0, 0
    for index, line in enumerate(lines):
        try:
            category, ident, value = parse_statement(line)
            order = {"process": 0, "steps": 1, "relations": 2}[category]
            assert order >= phase and (category != "process" or index == 0), "source declaration order"
            assert index != 0 or category == "process", "process first"
            phase = order
            if category == "process":
                assert partial[category] is None, "unique process"
                partial[category] = value
            else:
                partial[category].append(value)
            partial["evidence"].append(dict(object_id=ident, start_byte=byte_at,
                end_byte=byte_at + len(line.encode()), quote=line))
        except (AssertionError, ValueError, UnicodeError):
            unparsed.append(line)
        byte_at += len(line.encode()) + 1
    document_schema(partial)
    return normalize(partial), unparsed, len(lines)


def gold_document(gold):
    fields(gold, "schema_version producer author scientific_quality process steps relations evidence")
    exact({k: gold[k] for k in ("schema_version", "producer", "scientific_quality")},
          dict(schema_version=1, producer="process_source_gold", scientific_quality="not_run"), "gold fixed")
    display_text(gold["author"])
    doc = {k: gold[k] for k in DOC_FIELDS.split()}
    document_schema(doc)
    return normalize(doc)


def semantic_totals(doc, unparsed=0):
    steps, relations = (doc["steps"], doc["relations"]) if doc else ([], [])
    result = dict(source_statements=(len(doc["evidence"]) if doc else 0) + unparsed,
                  parsed_statements=len(doc["evidence"]) if doc else 0, unparsed_statements=unparsed,
                  steps=len(steps), decision_steps=sum(s["kind"] == "decision" for s in steps),
                  relations=len(relations))
    result.update({kind + "_relations": sum(r["kind"] == kind for r in relations) for kind in KINDS})
    result["evidence_records"] = len(doc["evidence"]) if doc else 0
    for key in ("inputs", "outputs", "conditions", "exceptions"):
        result["node_" + key] = sum(len(s[key]) for s in steps)
    for key in ("conditions", "exceptions"):
        result["process_" + key] = len(doc["process"][key]) if doc else 0
        result["relation_" + key] = sum(len(r[key]) for r in relations)
    return result


def profile_schema(profile):
    fields(profile, "schema_version producer profile_id app app_version mermaid_version flowchart_support label_encoding html_labels")
    exact({k: profile[k] for k in ("schema_version", "producer", "label_encoding")},
          dict(schema_version=1, producer="flow_render_profile", label_encoding="decimal_ascii_punctuation_v1"), "profile fixed")
    for key in ("profile_id", "app", "app_version", "mermaid_version"):
        display_text(profile[key])
    assert profile["flowchart_support"] in ("supported", "unverified", "unavailable"), "profile support enum"
    assert type(profile["html_labels"]) is bool, "profile HTML flag"


def classify(doc, profile):
    """Independent graph support; tie-breaking cannot create semantic edges."""
    profile_schema(profile)
    nodes = {s["id"]: s for s in doc["steps"]}
    rels = doc["relations"]
    if any(s["kind"] == "decision" and s["criterion"] is None for s in nodes.values()):
        return None, ["FLOW_UNKNOWN_CRITERION"], []
    for kind, code in (("correlates", "FLOW_CORRELATION_UNDIRECTED"), ("unknown", "FLOW_DIRECTION_UNKNOWN")):
        if any(r["kind"] == kind for r in rels):
            return None, [code], []
    outgoing = {ident: [r for r in rels if r["left"] == ident] for ident in nodes}
    for ident, s in nodes.items():
        edges = outgoing[ident]
        if ((s["kind"] == "decision" and
             (len(edges) != 2 or any(r["kind"] != "branch" for r in edges) or
              {r["branch_outcome"] for r in edges} != {True, False})) or
            (s["kind"] == "step" and any(r["kind"] == "branch" for r in edges))):
            return None, ["FLOW_DECISION_INCOMPLETE"], []
    indegree = {ident: sum(r["right"] == ident for r in rels) for ident in nodes}
    ready = [ident for ident in nodes if indegree[ident] == 0]; heapq.heapify(ready)
    order = []
    while ready:
        ident = heapq.heappop(ready); order.append(ident)
        for r in outgoing[ident]:
            indegree[r["right"]] -= 1
            if indegree[r["right"]] == 0:
                heapq.heappush(ready, r["right"])
    if len(order) != len(nodes):
        return None, ["FLOW_UNSUPPORTED_CYCLE"], []
    reached, todo = set(), [doc["process"]["entry"]]
    while todo:
        ident = todo.pop()
        if ident not in reached:
            reached.add(ident); todo.extend(r["right"] for r in outgoing[ident])
    if reached != set(nodes) or {n for n in nodes if not outgoing[n]} != set(doc["process"]["goals"]):
        return None, ["FLOW_DISCONNECTED"], []
    reasons = []
    supported = dict(schema_version=1, producer="flow_render_profile", profile_id="obsidian_1_6_7_flowchart",
        app="Obsidian", app_version="1.6.7", mermaid_version="10.6.1", flowchart_support="supported",
        label_encoding="decimal_ascii_punctuation_v1", html_labels=True)
    if profile != supported:
        reasons.append("FLOW_TARGET_UNVERIFIED")
    if any(label_whitespace(s["label"]) or label_whitespace(s["criterion"] or "") for s in nodes.values()):
        reasons.append("FLOW_LABEL_WHITESPACE_FALLBACK")
    return "ordered_list" if reasons else "flowchart", reasons, order


def label_whitespace(value):
    return (value.startswith(" ") or value.endswith(" ") or "  " in value or
            any(c.isspace() and c != " " for c in value))


def verify_fixture(root=FIXTURE):
    root = Path(root)
    manifest_raw = read(root / "frozen-sha256.json")
    assert fingerprint(manifest_raw)["sha256"] == FROZEN_SHA, "author manifest changed"
    manifest = strict_json(manifest_raw)
    actual = tree(root)
    exact(set(actual), set(manifest["files"]) | {"frozen-sha256.json"}, "frozen inventory")
    exact(pins({k: v for k, v in actual.items() if k != "frozen-sha256.json"}), manifest["files"], "frozen author bytes")
    authors = strict_json(actual["authors.json"])
    for case in authors["cases"]:
        expected = strict_json(actual[case["expected_file"]])
        doc, unmatched, total = parse_source(actual[case["source_file"]])
        exact(doc, gold_document(strict_json(actual[case["gold_file"]])), "author source to gold " + case["name"])
        verify_expectation(expected, doc, unmatched, total, strict_json(actual[case["profile_file"]]))
    return authors


def verify_expectation(e, doc, unmatched, count, profile):
    fields(e, "schema_version producer author scientific_quality knowledge_status published case_name profile review_status flow_status representation issues process nodes edges required_precedence totals unparsed_lines empty_list_meaning scope")
    exact({k: e[k] for k in ("schema_version", "producer", "scientific_quality", "knowledge_status", "published", "empty_list_meaning", "scope")},
          dict(schema_version=1, producer="process_flow_expected", scientific_quality="not_run", knowledge_status="pending",
               published=False, empty_list_meaning="not_listed_in_selected_source", scope="bounded_self_authored_engineering_only"), "expected fixed")
    exact(e["process"], doc["process"], "author process")
    exact(sorted(e["nodes"], key=lambda s: s["id"]), doc["steps"], "author nodes")
    exact(sorted(e["edges"], key=lambda s: s["id"]), doc["relations"], "author edges")
    representation, issues, _ = classify(doc, profile) if not unmatched else (None, ["PROCESS_UNPARSED_TEXT"], [])
    exact(e["representation"], representation, "author representation")
    exact(e["issues"], issues, "author issue")
    exact(e["review_status"], "needs_review" if unmatched else "passed", "author review")
    exact(e["flow_status"], "not_run" if unmatched else "passed" if representation else "blocked", "author flow")
    exact(e["profile"], profile["profile_id"], "author profile")
    exact(e["unparsed_lines"], unmatched, "author unmatched source")
    totals = semantic_totals(doc, len(unmatched))
    totals.update(source_statements=count, parsed_statements=count-len(unmatched), unparsed_statements=len(unmatched),
                  flow_nodes=len(doc["steps"]) if representation else 0,
                  flow_edges=len(doc["relations"]) if representation else 0)
    exact(e["totals"], totals, "author full denominators")
    directed = sorted([r["left"], r["right"]] for r in doc["relations"] if r["kind"] not in ("correlates", "unknown"))
    exact(sorted(e["required_precedence"]), directed if representation else [], "author precedence")


def load_expectation(path):
    path = Path(path)
    authors = verify_fixture(path.parent)
    case = next(c for c in authors["cases"] if c["expected_file"] == path.name)
    return strict_json(read(path)), case


def source_inputs(project, case):
    files = tree(case)
    assert "process-review.json" in files, "missing case manifest"
    config = strict_json(files["process-review.json"])
    fields(config, "schema_version purpose project_id import_id selection gold_file gold_sha256")
    exact({k: config[k] for k in ("schema_version", "purpose")},
          dict(schema_version=1, purpose="process_source_review"), "case fixed")
    uuid_string(config["project_id"]); uuid_string(config["import_id"])
    safe_name(config["gold_file"])
    assert not config["gold_file"].startswith("."), "nonhidden gold filename"
    assert config["gold_file"] != "process-review.json", "distinct gold filename"
    assert set(files) == {"process-review.json", config["gold_file"]}, "exact case inventory"
    selection = config["selection"]
    assert type(selection) is list and len(selection) == 1, "single selected unit"
    fields(selection[0], "kind ordinal")
    assert selection[0]["kind"] == "text" and type(selection[0]["ordinal"]) is int and selection[0]["ordinal"] >= 0, "explicit text ordinal"
    gold_raw = files[config["gold_file"]]
    exact(fingerprint(gold_raw)["sha256"], config["gold_sha256"], "gold input SHA")
    gold = gold_document(strict_json(gold_raw))
    tree(project)  # Reject linked/reparse source resources before actual_source resolves.
    source = actual_source(Path(project), config["import_id"], selection[0])
    exact(config["project_id"], source["binding"]["project_id"], "actual project binding")
    doc, unmatched, count = parse_source(source["units"][0]["text"].encode())
    snapshot = dict(schema_version=1, binding=source["binding"], units=source["units"])
    originals = {".case.json": files["process-review.json"], ".gold.json": gold_raw, ".source.json": encoded(snapshot)}
    return config, source, doc, unmatched, count, gold, originals


def verify_review(project, case, review, expected_path=None):
    config, source, doc, unmatched, count, gold, originals = source_inputs(project, case)
    files = tree(review)
    report = strict_json(files["review.json"])
    fields(report, REVIEW_FIELDS)
    uuid_string(report["request_id"])
    ready = not unmatched and doc == gold
    accepted = doc if not unmatched else None
    issues = ["PROCESS_UNPARSED_TEXT"] if unmatched else [] if ready else ["PROCESS_GOLD_MISMATCH"]
    expected_totals = semantic_totals(gold, len(unmatched))
    totals = semantic_totals(doc, len(unmatched)); totals["source_statements"] = count
    for key, value in dict(schema_version=1, producer="process_source_review", scope="bounded_process_text_v1",
            binding=source["binding"], selection=config["selection"], status="passed" if ready else "needs_review",
            **COMMON, checks=dict(source_parsing="needs_review" if unmatched else "passed",
                gold_match="not_run" if unmatched else "passed" if ready else "needs_review", saved_output="passed"),
            issue_codes=issues, unparsed_statements=len(unmatched), parsed_document=accepted, totals=totals,
            expected_totals=expected_totals, gold_sha256=config["gold_sha256"], inputs=pins(originals)).items():
        exact(report[key], value, "review " + key)
    for name, raw in originals.items():
        exact(files.get(name), raw, "actual source original " + name)
    owner = dict(schema_version=1, producer="process_source_review", request_id=report["request_id"], purpose="unpublished_process_source_review")
    exact(strict_json(files[".owner.json"]), owner, "review owner")
    artifact_names = set(originals) | {".owner.json"}
    if ready:
        artifact_names.add("process-ir.json")
        ir = dict(schema_version=1, producer="process_source_review", scope="bounded_process_text_v1",
            request_id=report["request_id"], binding=source["binding"], unit=source["units"][0], source_match="passed",
            gold_match="passed", knowledge_status="pending", scientific_quality="not_run", published=False,
            document=doc, evidence_fingerprints={e["object_id"]: fingerprint(e["quote"].encode()) for e in doc["evidence"]})
        exact(strict_json(files["process-ir.json"]), ir, "source-derived process IR")
        exact(files["process-ir.json"], encoded(ir), "canonical IR bytes")
    exact(report["ir_fingerprint"], fingerprint(files["process-ir.json"]) if ready else None, "review IR fingerprint")
    exact(set(files), artifact_names | {"review.json", "review.md"}, "review exact inventory")
    exact(report["artifacts"], pins({n: files[n] for n in artifact_names}), "review artifacts")
    markdown = "# 有界过程来源核对\n\n状态：" + report["status"] + "；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n" + files["review.json"].decode() + "\n```\n"
    exact(files["review.md"], markdown.encode(), "review Markdown full report")
    if expected_path:
        expected, author = load_expectation(expected_path)
        fixture = Path(expected_path).parent
        exact(source["units"][0]["text"].encode(), read(fixture / author["source_file"]), "actual selected author text")
        exact(files[".gold.json"], read(fixture / author["gold_file"]), "frozen original gold")
        exact(report["status"], expected["review_status"], "author review outcome")
    return report, files, doc


def node_label(step):
    return step["label"] + ("：" + step["criterion"] if step["kind"] == "decision" else "")


def relation_label(relation, steps):
    if relation["kind"] == "branch":
        return "判据为" + ("真" if relation["branch_outcome"] else "假") + "：" + steps[relation["left"]]["criterion"]
    return dict(sequence="随后", causes="导致", promotes="+：促进", inhibits="−：抑制",
                correlates="相关（无推定方向）", unknown="方向未说明")[relation["kind"]]


def array_body(label, values):
    return label + "：" + ("\n" + "\n".join(str(i+1) + ". " + value for i, value in enumerate(values))
                           if values else "选定来源未列出")


def semantic_bodies(doc):
    """A fixed field caption is formatting; every value comes from actual source."""
    steps = {s["id"]: s for s in doc["steps"]}
    p = doc["process"]
    bodies = {"process": "\n".join(["过程名称：" + p["title"], "入口：" + p["entry"],
               array_body("目标", p["goals"]), array_body("条件", p["conditions"]), array_body("例外", p["exceptions"])])}
    for s in doc["steps"]:
        lines = ["步骤：" + s["id"], "类型：" + ("判定" if s["kind"] == "decision" else "步骤"),
                 "名称：" + s["label"], "说明：" + s["description"], "判据：" + (s["criterion"] or "不适用（普通步骤）")]
        lines += [array_body(label, s[key]) for label, key in (("输入", "inputs"), ("输出", "outputs"), ("条件", "conditions"), ("例外", "exceptions"))]
        bodies[s["id"]] = "\n".join(lines)
    for r in doc["relations"]:
        bodies[r["id"]] = "\n".join(["关系：" + r["id"], "起点：" + r["left"], "起点名称：" + steps[r["left"]]["label"],
            "终点：" + r["right"], "终点名称：" + steps[r["right"]]["label"], "含义：" + relation_label(r, steps),
            array_body("条件", r["conditions"]), array_body("例外", r["exceptions"])])
    return bodies


def semantic_links(doc):
    result = {"process": dict(target_path="过程明细.md", target_heading="过程说明", display_text="查看完整过程说明", source_object_ids=["process"])}
    for category, label in (("steps", "步骤"), ("relations", "关系")):
        for item in doc[category]:
            result[item["id"]] = dict(target_path="过程明细.md", target_heading=label + "-" + item["id"],
                                     display_text="查看" + label + "明细", source_object_ids=[item["id"]])
    return result


def fallback_reasons(doc, codes):
    messages = {"FLOW_TARGET_UNVERIFIED": "所选目标版本或标签配置尚无本版实测依据，使用保留完整关系的文字表达。",
                "FLOW_LABEL_WHITESPACE_FALLBACK": "图标签含当前显示契约未确认可保真的空白，改用预格式文字保留原字符。"}
    return [dict(code=code, object_ids=([s["id"] for s in doc["steps"] if label_whitespace(s["label"]) or label_whitespace(s["criterion"] or "")]
                 if code == "FLOW_LABEL_WHITESPACE_FALLBACK" else []), message=messages[code]) for code in codes]


def verify_plan(plan, doc, profile, request_id):
    fields(plan, "schema_version producer scope representation_origin request_id revision knowledge_status scientific_quality research_status published document profile representation decision topological_order diagram notes")
    representation, codes, order = classify(doc, profile)
    assert representation, "blocked document cannot have plan"
    decision = dict(rule_id="verified_directed_acyclic_flow_v1" if representation == "flowchart" else "verified_relations_text_fallback_v1",
        reason="来源明确给出完整有向无环关系，且所选目标的当前图形及标签配置已有实测依据。" if representation == "flowchart" else "来源关系已通过流程语义检查；当前显示条件采用有据文字回退。",
        fallback_reasons=fallback_reasons(doc, codes))
    for key, value in dict(schema_version=1, producer="knowledge_flow", scope="active_flow_v1", representation_origin="deterministic_planner",
            request_id=request_id, revision=1, knowledge_status="pending", scientific_quality="not_run", research_status="not_requested", published=False,
            document=doc, profile=profile, representation=representation, decision=decision, topological_order=order).items():
        exact(plan[key], value, "source-derived plan " + key)
    nodes = {s["id"]: s for s in doc["steps"]}
    ids = {s["id"]: "n" + str(i) for i, s in enumerate(doc["steps"])}
    diagram = None
    if representation == "flowchart":
        diagram = dict(direction="TD",
            nodes=[dict(node_id=ids[s["id"]], step_id=s["id"], kind=s["kind"], text=node_label(s)) for s in doc["steps"]],
            edges=[dict(edge_id="e"+str(i), relation_id=r["id"], left_node_id=ids[r["left"]], right_node_id=ids[r["right"]],
                        kind=r["kind"], branch_outcome=r["branch_outcome"], text=relation_label(r, nodes)) for i, r in enumerate(doc["relations"])])
    exact(plan["diagram"], diagram, "source-derived graph semantics")
    bodies, links = semantic_bodies(doc), semantic_links(doc)
    provenance = {"process": ["process"], **{s["id"]: [s["id"]] for s in doc["steps"]},
                  **{r["id"]: sorted({r["id"], r["left"], r["right"]}) for r in doc["relations"]}}
    assert type(plan["notes"]) is list and len(plan["notes"]) == 2, "two fixed notes"
    overview, details = plan["notes"]
    def note(n, ident, path, title):
        fields(n, "note_id output_path title sections")
        exact([n["note_id"], n["output_path"], n["title"]], [ident, path, title], "fixed note")
    note(overview, "flow_overview", "流程总览.md", "流程总览")
    note(details, "process_details", "过程明细.md", "过程明细")
    def section(s, ident, heading):
        fields(s, "section_id heading blocks")
        exact([s["section_id"], s["heading"]], [ident, heading], "fixed section")
    def text_block(block, ident, body, source_ids):
        exact(block, dict(kind="text", content=dict(block_id=ident, text=body, source_object_ids=source_ids)), "complete source body " + ident)
    def link_block(block, ident, values):
        exact(block, dict(kind="links", block_id=ident, links=values), "complete navigation")
    assert len(overview["sections"]) == (1 if representation == "flowchart" else 3), "overview sections"
    front = overview["sections"][0]; section(front, "flow", "流程表达")
    assert len(front["blocks"]) == 4, "overview blocks"
    text_block(front["blocks"][0], "process_title", "过程名称：" + doc["process"]["title"], ["process"])
    text_block(front["blocks"][1], "reading_scope", "图和列表仅表达来源明确列出的关系。步骤的输入、输出、条件和例外，以及每条关系的条件和例外，见过程明细；这些限定仍然适用。文字编号用于定位和阅读，不另行主张步骤之间的执行顺序。", [])
    if representation == "flowchart":
        exact(front["blocks"][2], dict(kind="diagram", block_id="process_diagram"), "sole graph block")
    else:
        text_block(front["blocks"][2], "fallback_reason", "\n".join(r["message"] for r in decision["fallback_reasons"]), [])
        for index, (section_id, heading, block_id, item_ids) in enumerate([
                ("step_list", "步骤列表", "steps", order),
                ("relation_list", "明确关系列表", "relations", [r["id"] for r in doc["relations"]])], 1):
            s = overview["sections"][index]; section(s, section_id, heading)
            expected_items = [dict(item_id=ident, text=bodies[ident], source_object_ids=provenance[ident], links=[links[ident]]) for ident in item_ids]
            exact(s["blocks"], [dict(kind="ordered_list", block_id=block_id, items=expected_items)], "complete fallback list")
    link_block(front["blocks"][3], "detail_links", list(links.values()))
    layout = [("process_detail", "过程说明", "process")]
    layout += [("step_"+s["id"], "步骤-"+s["id"], s["id"]) for s in doc["steps"]]
    layout += [("relation_"+r["id"], "关系-"+r["id"], r["id"]) for r in doc["relations"]]
    assert len(details["sections"]) == len(layout) + 1, "all source objects have detail sections"
    for s, (ident, heading, object_id) in zip(details["sections"], layout):
        section(s, ident, heading)
        assert len(s["blocks"]) == 1, "single complete detail block"
        text_block(s["blocks"][0], object_id, bodies[object_id], provenance[object_id])
    back = details["sections"][-1]; section(back, "return", "返回总览")
    exact(back["blocks"], [dict(kind="links", block_id="return_link", links=[dict(target_path="流程总览.md", target_heading="流程表达",
        display_text="返回流程总览", source_object_ids=["process"])])], "back navigation")
    return plan


def decode_label(label):
    """Decode only declared ASCII-punctuation entities, with no raw directives."""
    pieces, at = [], 0
    punctuation = r"!\"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"
    while at < len(label):
        if label[at] == "#":
            m = re.match(r"#([1-9][0-9]*);", label[at:])
            assert m and len(m[1]) <= 3, "Mermaid entity grammar"
            value = int(m[1]); char = chr(value)
            assert value < 128 and char in punctuation, "only punctuation entity"
            pieces.append(char); at += len(m[0])
        else:
            char = label[at]
            assert char not in punctuation and unicodedata.category(char) != "Cc", "unencoded Mermaid label punctuation/control"
            pieces.append(char); at += 1
    return "".join(pieces)


class MarkdownReader:
    def __init__(self, raw):
        assert b"\r" not in raw and raw.endswith(b"\n"), "finite LF Markdown"
        self.raw = raw
        self.lines = []
        # Split only physical LF; Unicode line separators remain source data.
        at = 0
        for line in raw.split(b"\n")[:-1]:
            self.lines.append((line.decode("utf-8"), at, at+len(line)))
            at += len(line)+1
        self.index = 0

    def next(self):
        assert self.index < len(self.lines), "truncated Markdown"
        line = self.lines[self.index]; self.index += 1
        return line

    def take(self, text):
        line = self.next(); exact(line[0], text, "Markdown structure")
        return line

    def fence(self, indent=""):
        line = self.next()
        match = re.fullmatch(re.escape(indent) + r"(`{3,})text", line[0])
        assert match, "preformatted text fence"
        fence = match[1]; values = []; start = line[2]+1
        while True:
            line = self.next()
            assert line[0].startswith(indent), "list indentation preserved"
            decoded = line[0][len(indent):]
            if decoded == fence:
                end = line[1]-1
                break
            assert fence not in decoded, "source fence could close"
            values.append(decoded)
        assert values, "empty semantic body"
        self.take("")
        return "\n".join(values), start, end


def verify_rendered(plan, files, actual):
    """Read real file grammar, reconstruct offsets; never render expected bytes."""
    assert set(actual) == {n["output_path"] for n in plan["notes"]}, "exact rendered note inventory"
    assert type(files) is list and len(files) == 2, "two rendered file reports"
    declarations = {file["relative_path"]: file for file in files}
    assert len(declarations) == 2, "distinct rendered files"
    targets = {(n["output_path"], s["heading"]) for n in plan["notes"] for s in n["sections"]}
    relations = {r["id"]: r for r in plan["document"]["relations"]}
    for note in plan["notes"]:
        raw = actual[note["output_path"]]; reader = MarkdownReader(raw)
        mappings, links = [], []
        def content(section, block, kind, item, ids, start, end):
            mappings.append(dict(section_id=section["section_id"], block_id=block, kind=kind, item_id=item,
                                 source_object_ids=ids, start_byte=start, end_byte=end))
        def read_links(section, block, item, values, indent=""):
            for value in values:
                line, start, end = reader.next()
                assert line.startswith(indent), "link indentation"
                token = line[len(indent):]
                match = re.fullmatch(r"\[\[([^#|\[\]\n]+)#([^|\[\]\n]+)\|([^|\[\]\n]+)\]\]", token)
                assert match, "ordinary internal link token"
                path = match[1] + ".md"
                exact([path, match[2], match[3]], [value["target_path"], value["target_heading"], value["display_text"]], "actual navigation")
                assert (path, match[2]) in targets, "link destination heading"
                links.append(dict(section_id=section["section_id"], block_id=block, item_id=item,
                    **value, start_byte=start+len(indent.encode()), end_byte=end))
            reader.take("")
        reader.take("# " + note["title"]); reader.take("")
        for s in note["sections"]:
            reader.take("## " + s["heading"]); reader.take("")
            for b in s["blocks"]:
                if b["kind"] == "text":
                    c = b["content"]
                    text, start, end = reader.fence()
                    exact(text, c["text"], "actual complete semantic body")
                    content(s, c["block_id"], "text", None, c["source_object_ids"], start, end)
                elif b["kind"] == "links":
                    read_links(s, b["block_id"], None, b["links"])
                elif b["kind"] == "ordered_list":
                    for i, item in enumerate(b["items"]):
                        marker = str(i+1)+"."; reader.take(marker); indent = " " * (len(marker)+1)
                        text, start, end = reader.fence(indent)
                        exact(text, item["text"], "actual complete fallback item")
                        content(s, b["block_id"], "list_item", item["item_id"], item["source_object_ids"], start, end)
                        read_links(s, b["block_id"], item["item_id"], item["links"], indent)
                elif b["kind"] == "diagram":
                    reader.take("```mermaid"); reader.take("flowchart TD")
                    for node in plan["diagram"]["nodes"]:
                        line, start, end = reader.next()
                        match = re.fullmatch(r'  (n[0-9]+)([\[{])"([^"\n]*)"([\]}])', line)
                        assert match, "finite Mermaid node"
                        exact([match[1], match[2], match[4], decode_label(match[3])],
                              [node["node_id"], "{" if node["kind"] == "decision" else "[", "}" if node["kind"] == "decision" else "]", node["text"]], "actual Mermaid node meaning")
                        content(s, b["block_id"], "diagram_node", node["node_id"], [node["step_id"]], start, end)
                    for edge in plan["diagram"]["edges"]:
                        line, start, end = reader.next()
                        match = re.fullmatch(r'  (n[0-9]+) -->\|"([^"\n]*)"\| (n[0-9]+)', line)
                        assert match, "finite Mermaid edge"
                        exact([match[1], match[3], decode_label(match[2])], [edge["left_node_id"], edge["right_node_id"], edge["text"]], "actual Mermaid direction / sign / criterion")
                        ids = [edge["relation_id"]]
                        if edge["kind"] == "branch": ids.append(relations[edge["relation_id"]]["left"])
                        content(s, b["block_id"], "diagram_edge", edge["edge_id"], sorted(set(ids)), start, end)
                    reader.take("```"); reader.take("")
                else:
                    raise AssertionError("unknown flow block")
        assert reader.index == len(reader.lines), "unmapped extra Markdown"
        file = declarations[note["output_path"]]
        exact(file, dict(relative_path=note["output_path"], note_id=note["note_id"], plan_revision=plan["revision"],
            content_sha256=fingerprint(raw)["sha256"], byte_length=len(raw), content_mappings=mappings, links=links), "actual rendered mappings and bytes")
    return dict(rendered_notes=len(files), content_mappings=sum(len(f["content_mappings"]) for f in files), links=sum(len(f["links"]) for f in files))


def verify_flow(project, case, review, profile_path, vault, expected_path=None):
    reviewed, review_files, doc = verify_review(project, case, review, expected_path)
    profile_raw = read(profile_path); profile = strict_json(profile_raw); profile_schema(profile)
    files = tree(vault); report = strict_json(files["flow.json"]); fields(report, FLOW_FIELDS)
    uuid_string(report["request_id"])
    source_ready = reviewed["status"] == "passed"
    representation, codes, _ = classify(doc, profile) if source_ready else (None, ["FLOW_SOURCE_UNVERIFIED"], [])
    ready = representation is not None
    for key, value in dict(schema_version=1, producer="knowledge_flow", scope="active_flow_v1", binding=reviewed["binding"], **COMMON,
            status="passed" if ready else "needs_review", preview_status="rendered" if ready else "not_run",
            checks=dict(source_review=reviewed["status"], model_support="not_run" if not source_ready else "passed" if ready else "needs_review",
                note_plan="passed" if ready else "not_run", rendering="passed" if ready else "not_run", saved_output="passed"),
            issue_codes=codes, process_document=reviewed["parsed_document"], profile=profile, representation=representation,
            review_ir_fingerprint=reviewed["ir_fingerprint"]).items():
        exact(report[key], value, "flow " + key)
    inputs = {name: review_files[name] for name in (".case.json", ".gold.json", ".source.json")}
    inputs[".profile.json"] = profile_raw
    inputs.update({".review-"+name: raw for name, raw in review_files.items()})
    exact(report["inputs"], pins(inputs), "flow input fingerprints")
    for name, raw in inputs.items(): exact(files.get(name), raw, "flow saved original " + name)
    exact(strict_json(files[".owner.json"]), dict(schema_version=1, producer="knowledge_flow", request_id=report["request_id"], purpose="unpublished_flow"), "flow owner")
    artifacts = set(inputs) | {".owner.json"}
    totals = copy.deepcopy(reviewed["totals"])
    totals.update(flow_nodes=len(doc["steps"]) if ready else 0, flow_edges=len(doc["relations"]) if ready else 0)
    if ready:
        plan = strict_json(files["plan.json"])
        verify_plan(plan, doc, profile, report["request_id"])
        actual = {name: files[name] for name in ("流程总览.md", "过程明细.md")}
        totals.update(verify_rendered(plan, report["files"], actual))
        exact(report["decision"], plan["decision"], "flow decision")
        exact(report["plan_fingerprint"], fingerprint(files["plan.json"]), "flow plan fingerprint")
        exact(report["issues"], [], "passed flow has no semantic blockers")
        artifacts |= {"plan.json", *actual}
    else:
        exact(report["files"], [], "blocked render inventory")
        exact(report["decision"], None, "blocked decision")
        exact(report["plan_fingerprint"], None, "blocked plan")
        assert type(report["issues"]) is list and len(report["issues"]) == len(codes), "blocker details"
        all_ids = {e["object_id"] for e in doc["evidence"]}
        for issue, code in zip(report["issues"], codes):
            fields(issue, "code object_ids message"); exact(issue["code"], code, "blocker code")
            display_text(issue["message"])
            assert type(issue["object_ids"]) is list and set(issue["object_ids"]) <= all_ids, "blocker object IDs"
            exact(issue["object_ids"], sorted(set(issue["object_ids"])), "unique ordered blocker IDs")
        totals.update(rendered_notes=0, content_mappings=0, links=0)
    exact(report["totals"], totals, "flow complete denominators")
    exact(set(files), artifacts | {"flow.json", "flow.md"}, "flow exact inventory")
    exact(report["artifacts"], pins({n: files[n] for n in artifacts}), "flow artifact fingerprints")
    markdown = "# 有据流程表达预览\n\n状态：" + report["status"] + "；预览：" + report["preview_status"] + "；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n" + files["flow.json"].decode() + "\n```\n"
    exact(files["flow.md"], markdown.encode(), "flow full Markdown report")
    if expected_path:
        expected, _ = load_expectation(expected_path)
        exact(report["representation"], expected["representation"], "author expression")
        exact(report["checks"]["model_support"], {"passed": "passed", "blocked": "needs_review", "not_run": "not_run"}[expected["flow_status"]], "author flow status")
        for key, value in expected["totals"].items(): exact(totals[key], value, "author rendered denominator " + key)
        if source_ready: exact(codes, expected["issues"], "author flow issues")
    return report


def refresh_review(root):
    """Tamper support only: synchronize checksums, never used for verification."""
    files = tree(root); report = strict_json(files["review.json"])
    report["inputs"] = pins({n: files[n] for n in (".case.json", ".gold.json", ".source.json")})
    report["artifacts"] = pins({n: v for n, v in files.items() if n not in ("review.json", "review.md")})
    report["ir_fingerprint"] = fingerprint(files["process-ir.json"]) if "process-ir.json" in files else None
    raw = encoded(report); (root / "review.json").write_bytes(raw)
    (root / "review.md").write_bytes(("# 有界过程来源核对\n\n状态：" + report["status"] + "；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n" + raw.decode() + "\n```\n").encode())


def refresh_flow(root):
    """Tamper support: update all public fingerprints after changing output."""
    files = tree(root); report = strict_json(files["flow.json"])
    report["artifacts"] = pins({n: raw for n, raw in files.items() if n not in ("flow.json", "flow.md")})
    report["plan_fingerprint"] = fingerprint(files["plan.json"]) if "plan.json" in files else None
    for file in report["files"]:
        raw = files[file["relative_path"]]
        file["content_sha256"] = fingerprint(raw)["sha256"]; file["byte_length"] = len(raw)
    raw = encoded(report); (root / "flow.json").write_bytes(raw)
    (root / "flow.md").write_bytes(("# 有据流程表达预览\n\n状态：" + report["status"] + "；预览：" + report["preview_status"] + "；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n" + raw.decode() + "\n```\n").encode())


def audit(cli, root):
    cli, root = Path(cli), Path(root)
    assert cli.is_absolute(), "absolute ordinary CLI path required"
    guard_path(cli, directory=False); guard_path(root.parent, directory=True)
    assert not root.exists(), "audit requires new root"
    authors = verify_fixture()
    root.mkdir()
    shutil.copytree(FIXTURE, root / "frozen-authors")
    code = root / "source-pins"; code.mkdir()
    source_names = ["scripts/verify_process_flow.py", "tests/test_verify_process_flow.py",
        "scripts/verify_extraction_knowledge.py", "scripts/verify_knowledge_coordination.py",
        "scripts/verify_knowledge_conflicts.py", "doc/technical/PROCESS_FLOW_CONTRACT.md"]
    source_names += [str(p.relative_to(REPO)) for p in sorted((REPO / "crates/noetome-cli/src").glob("process*.rs"))]
    for name in source_names:
        target = code / name; target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes((REPO / name).read_bytes())
    baseline = dict(cli_path=str(cli), cli=fingerprint(cli.read_bytes()), python=sys.executable, platform=sys.platform,
                    fixture_manifest=fingerprint(read(FIXTURE / "frozen-sha256.json")), source_pins=pins(tree(code)))
    save(root / "before.json", baseline)
    commands, scenarios = [], []
    (root / "commands").mkdir(); (root / "cases").mkdir(); (root / "mutations").mkdir()
    def record():
        save(root / "audit.json", dict(status="running", commands=commands, scenarios=scenarios, before=baseline))
    def command(label, args, expected, protect=()):
        before = {str(p): pins(tree(p)) if Path(p).is_dir() else fingerprint(read(p)) for p in protect}
        argv = [str(cli), *map(str, args)]
        start = time.monotonic(); result = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
        prefix = root / "commands" / (str(len(commands)).zfill(3) + "-" + label)
        prefix.with_suffix(".stdout.bin").write_bytes(result.stdout); prefix.with_suffix(".stderr.bin").write_bytes(result.stderr)
        after = {str(p): pins(tree(p)) if Path(p).is_dir() else fingerprint(read(p)) for p in protect}
        receipt = dict(label=label, argv=argv, exit_code=result.returncode, elapsed_seconds=time.monotonic()-start,
                       stdout=fingerprint(result.stdout), stderr=fingerprint(result.stderr), input_before=before, input_after=after)
        save(prefix.with_suffix(".receipt.json"), receipt); commands.append(receipt); record()
        exact(before, after, "ordinary CLI readonly protected inputs")
        assert result.returncode in expected, label + " exit " + str(result.returncode) + ": " + result.stderr.decode(errors="replace")
        return result
    def rejected(label, fn):
        try:
            fn()
        except (AssertionError, ValueError, KeyError, OSError) as failure:
            scenarios.append(dict(name=label, status="rejected", reason=str(failure), exception=type(failure).__name__))
            record(); return
        raise AssertionError("independent checker accepted mutation: " + label)
    fixtures = root / "frozen-authors"
    for case in authors["cases"]:
        name = case["name"]; folder = root / "cases" / name; folder.mkdir()
        source_path = folder / "source.txt"; source_path.write_bytes(read(fixtures / case["source_file"]))
        profile = folder / "profile.json"; profile.write_bytes(read(fixtures / case["profile_file"]))
        expected_path = fixtures / case["expected_file"]
        (folder / "expected.json").write_bytes(read(expected_path))
        project = folder / "project"; case_path = folder / "case"; case_path.mkdir()
        command(name+"-project", ["project", "init", project], [0])
        imported = command(name+"-import", ["source", "import", project, source_path], [0])
        import_id = strict_json(imported.stdout)["import_id"]
        selected = actual_source(project, import_id, dict(kind="text", ordinal=0))
        exact(selected["units"][0]["text"].encode(), read(source_path), "ordinary import selected bytes")
        save(folder / "actual-source.json", selected)
        gold = read(fixtures / case["gold_file"]); (case_path / "gold.json").write_bytes(gold)
        save(case_path / "process-review.json", dict(schema_version=1, purpose="process_source_review",
            project_id=selected["binding"]["project_id"], import_id=import_id, selection=[dict(kind="text", ordinal=0)],
            gold_file="gold.json", gold_sha256=fingerprint(gold)["sha256"]))
        expected = strict_json(read(expected_path)); review = folder / "review"; vault = folder / "Vault"
        command(name+"-review", ["knowledge", "process-review", project, case_path, review],
                [0 if expected["review_status"] == "passed" else 3], [project, case_path])
        checked, _, _ = verify_review(project, case_path, review, expected_path)
        command(name+"-flow", ["knowledge", "flow", project, case_path, review, profile, vault],
                [0 if expected["flow_status"] == "passed" else 3], [project, case_path, review, profile])
        flow = verify_flow(project, case_path, review, profile, vault, expected_path)
        scenarios.append(dict(name=name, status="passed", review_status=checked["status"], flow_status=flow["status"],
            model_support=flow["checks"]["model_support"], representation=flow["representation"], issue_codes=flow["issue_codes"], totals=flow["totals"]))
        save(folder / "independent.json", scenarios[-1]); record()
    # Saved review and actual source are independent from output checksums.
    def setup(name, source_case):
        source = root / "cases" / source_case; mutated = root / "mutations" / name; mutated.mkdir()
        return source, mutated
    for name, source_case, kind in [("review-source-snapshot", "linear", "snapshot"),
            ("review-evidence-range", "linear", "evidence"), ("review-direction", "linear", "direction"),
            ("review-missing-exception", "linear", "exception"), ("review-truthy-published", "linear", "bool"),
            ("review-extra-file", "linear", "extra")]:
        source, mutated = setup(name, source_case); review = mutated / "review"; shutil.copytree(source / "review", review)
        if kind == "extra":
            (review / "unexpected.txt").write_text("unknown input", encoding="utf-8")
        elif kind == "snapshot":
            snapshot = strict_json(read(review / ".source.json")); snapshot["units"][0]["text"] += "虚构。"
            (review / ".source.json").write_bytes(encoded(snapshot))
        else:
            ir = strict_json(read(review / "process-ir.json")); report = strict_json(read(review / "review.json"))
            if kind == "bool": ir["published"] = 0
            elif kind == "evidence": ir["document"]["evidence"][0]["start_byte"] += 1
            elif kind == "direction":
                r = ir["document"]["relations"][0]; r["left"], r["right"] = r["right"], r["left"]
            elif kind == "exception": ir["document"]["relations"][0]["exceptions"] = []
            report["parsed_document"] = ir["document"]
            (review / "process-ir.json").write_bytes(encoded(ir)); (review / "review.json").write_bytes(encoded(report))
        refresh_review(review)
        rejected(name, lambda: verify_review(source / "project", source / "case", review))
        command(name, ["knowledge", "flow", source / "project", source / "case", review, source / "profile.json", mutated / "Vault"], [2, 3],
                [source / "project", source / "case", review, source / "profile.json"])
        assert not (mutated / "Vault").exists(), "rejected saved input created output"
    # A synchronized forged source/gold/IR/report still cannot replace SourceSession.
    source, mutated = setup("synchronized-source-gold-review", "linear")
    review, case_path = mutated / "review", mutated / "case"
    shutil.copytree(source / "review", review); shutil.copytree(source / "case", case_path)
    forged_raw = read(source / "source.txt").replace("登记卡片".encode(), "登记伪造卡片".encode())
    forged_doc, unmatched, _ = parse_source(forged_raw); assert not unmatched
    gold = strict_json(read(case_path / "gold.json")); gold.update(forged_doc); raw_gold = encoded(gold)
    (case_path / "gold.json").write_bytes(raw_gold); (review / ".gold.json").write_bytes(raw_gold)
    config = strict_json(read(case_path / "process-review.json")); config["gold_sha256"] = fingerprint(raw_gold)["sha256"]
    raw_config = encoded(config); (case_path / "process-review.json").write_bytes(raw_config); (review / ".case.json").write_bytes(raw_config)
    snapshot = strict_json(read(review / ".source.json")); snapshot["units"][0]["text"] = forged_raw.decode(); snapshot["units"][0]["end_byte"] = len(forged_raw)
    (review / ".source.json").write_bytes(encoded(snapshot))
    ir = strict_json(read(review / "process-ir.json")); ir["document"] = forged_doc; ir["unit"] = snapshot["units"][0]
    ir["evidence_fingerprints"] = {e["object_id"]: fingerprint(e["quote"].encode()) for e in forged_doc["evidence"]}
    (review / "process-ir.json").write_bytes(encoded(ir))
    report = strict_json(read(review / "review.json")); report["parsed_document"] = forged_doc; report["gold_sha256"] = config["gold_sha256"]
    (review / "review.json").write_bytes(encoded(report)); refresh_review(review)
    rejected("synchronized-source-gold-review", lambda: verify_review(source / "project", case_path, review))
    command("synchronized-source-gold-review", ["knowledge", "flow", source / "project", case_path, review, source / "profile.json", mutated / "Vault"], [2, 3],
            [source / "project", case_path, review, source / "profile.json"])
    assert not (mutated / "Vault").exists(), "forged source created output"
    # Read back modified final notes even when every public hash is refreshed.
    mutations = [("flow-reversed-arrow", "linear", "arrow"), ("flow-lost-inhibition-sign", "inhibits", "sign"),
        ("flow-false-branch", "branch", "branch"), ("flow-extra-mermaid-node", "linear", "extra_node"),
        ("flow-missing-condition", "linear", "condition"), ("flow-missing-exception", "linear", "exception"),
        ("flow-wrong-input", "linear", "input"), ("flow-flattened-whitespace", "multiline_labels", "whitespace"),
        ("flow-lost-mapping", "linear", "mapping"), ("flow-wrong-provenance", "branch", "provenance"),
        ("flow-link-target", "linear", "link"), ("flow-synchronized-plan-direction", "linear", "plan_direction"),
        ("flow-synchronized-cause-to-plus", "causes", "plan_cause"), ("flow-extra-output", "linear", "extra_file"),
        ("flow-knowledge-published", "linear", "published"), ("flow-false-total", "linear", "total")]
    for name, source_case, kind in mutations:
        source, mutated = setup(name, source_case); vault = mutated / "Vault"; shutil.copytree(source / "Vault", vault)
        overview = vault / "流程总览.md"; detail = vault / "过程明细.md"
        if kind in ("mapping", "provenance", "published", "total"):
            report = strict_json(read(vault / "flow.json"))
            if kind == "mapping": report["files"][0]["content_mappings"].pop()
            elif kind == "provenance":
                m = next(m for f in report["files"] for m in f["content_mappings"] if m["kind"] == "diagram_edge" and len(m["source_object_ids"]) == 2)
                m["source_object_ids"] = [m["source_object_ids"][-1]]
            elif kind == "published": report["published"] = True
            else: report["totals"]["flow_edges"] += 1
            (vault / "flow.json").write_bytes(encoded(report))
        elif kind == "extra_file": (vault / "unmapped.md").write_text("extra success", encoding="utf-8")
        elif kind in ("plan_direction", "plan_cause"):
            plan = strict_json(read(vault / "plan.json")); edge = plan["diagram"]["edges"][0]
            if kind == "plan_direction":
                edge["left_node_id"], edge["right_node_id"] = edge["right_node_id"], edge["left_node_id"]
                r = plan["document"]["relations"][0]; r["left"], r["right"] = r["right"], r["left"]
                raw = read(overview).decode(); raw = raw.replace('  n0 -->|"随后"| n1', '  n1 -->|"随后"| n0')
            else:
                edge["kind"] = "promotes"; edge["text"] = "+：促进"; plan["document"]["relations"][0]["kind"] = "promotes"
                raw = read(overview).decode().replace('"导致"', '"#43;：促进"')
            overview.write_bytes(raw.encode()); (vault / "plan.json").write_bytes(encoded(plan))
        elif kind in ("condition", "exception", "input"):
            doc = strict_json(read(vault / "plan.json"))["document"]
            field = {"condition": "conditions", "exception": "exceptions", "input": "inputs"}[kind]
            original = doc["steps"][0][field][0]
            raw = read(detail).decode(); assert original in raw
            detail.write_bytes(raw.replace(original, "被篡改", 1).encode())
        else:
            raw = read(overview).decode()
            if kind == "arrow": raw = raw.replace('  n0 -->|"随后"| n1', '  n1 -->|"随后"| n0')
            elif kind == "sign": raw = raw.replace("−：抑制", "#43;：促进")
            elif kind == "branch": raw = raw.replace("判据为真：", "判据为假：", 1)
            elif kind == "extra_node": raw = raw.replace("flowchart TD\n", 'flowchart TD\n  n99["无来源"]\n')
            elif kind == "whitespace": raw = raw.replace("\t", " ")
            elif kind == "link": raw = raw.replace("#步骤-s1|", "#步骤-s999|", 1)
            assert raw.encode() != read(overview), "mutation must change actual output bytes"
            overview.write_bytes(raw.encode())
        refresh_flow(vault)
        rejected(name, lambda: verify_flow(source / "project", source / "case", source / "review", source / "profile.json", vault))
    # Real ordinary CLI new-root / contract rejection checks, without guessing support.
    source = root / "cases" / "linear"
    for name, verb in (("existing-review-root", "process-review"), ("existing-flow-root", "flow")):
        _, mutated = setup(name, "linear"); existing = mutated / "occupied"; existing.mkdir(); (existing / "user.txt").write_bytes(b"retain existing user bytes")
        args = ["knowledge", verb, source / "project", source / "case"]
        if verb == "flow": args += [source / "review", source / "profile.json"]
        args += [existing]
        command(name, args, [2, 3], [source / "project", source / "case", source / "review", existing])
        scenarios.append(dict(name=name, status="rejected_readonly")); record()
    assert fingerprint(cli.read_bytes()) == baseline["cli"], "CLI changed during audit"
    result = dict(status="passed", scope="bounded_self_authored_engineering_only", scientific_quality="not_run",
                  root=str(root), commands=commands, scenarios=scenarios, before=baseline,
                  counts=dict(author_cases=len(authors["cases"]), ordinary_cli_commands=len(commands), scenarios=len(scenarios)),
                  saved_output_inventory=pins({n: raw for n, raw in tree(root).items() if n != "audit.json"}))
    save(root / "audit.json", result)
    return {k: result[k] for k in ("status", "scope", "scientific_quality", "root", "counts")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    review = sub.add_parser("check-review")
    for name in ("project", "case", "review", "expected"): review.add_argument(name, type=Path)
    check = sub.add_parser("check")
    for name in ("project", "case", "review", "profile", "vault", "expected"): check.add_argument(name, type=Path)
    run = sub.add_parser("audit"); run.add_argument("cli", type=Path); run.add_argument("root", type=Path)
    sub.add_parser("check-fixture")
    args = parser.parse_args()
    if args.command == "audit": result = audit(args.cli, args.root)
    elif args.command == "check-fixture": result = dict(status="passed", cases=len(verify_fixture()["cases"]))
    elif args.command == "check-review":
        report, _, _ = verify_review(args.project, args.case, args.review, args.expected)
        result = dict(status="passed", review_status=report["status"], totals=report["totals"])
    else:
        report = verify_flow(args.project, args.case, args.review, args.profile, args.vault, args.expected)
        result = dict(status="passed", flow_status=report["status"], representation=report["representation"], totals=report["totals"])
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
