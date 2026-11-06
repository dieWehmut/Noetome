"""Read-only author audit; fixed spans are not produced by the Rust parser."""

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent
FIXED_SPANS = {
    "S4m": [("P1a", 0, 24, "heading"), ("P2a", 24, 25, "formatting"),
            ("P3a", 25, 65, "text"), ("P4a", 65, 66, "formatting"),
            ("P5a", 66, 170, "text")],
    "S9q": [("P1b", 0, 24, "heading"), ("P2b", 24, 25, "formatting"),
            ("P3b", 25, 65, "text"), ("P4b", 65, 66, "formatting"),
            ("P5b", 66, 167, "text"), ("P6b", 167, 168, "formatting"),
            ("P7b", 168, 208, "text")],
}
FIXED_SOURCE_SHA = {
    "S4m": "4f5c914504aaab35408dfcc88813b838b9b5ff99efd7023af60888e258425562",
    "S9q": "ed8491e5b6935d46ba4708da6d969593f1c79c06739294bde7376435bbfa6535",
}
FIXED_ATOMS = {
    "A3d": ("甲容器是虚构的蓝砂收纳盒。", [], ["P3a", "P7b"]),
    "A7e": ("乙容器是虚构的红砂收纳盒。", [], ["P3b"]),
    "A5p": ("甲容器的虚构刻度为5 cm。", ["仅用于室温演示", "装满后停止读数", "破损时不适用"], ["P5a"]),
    "A9w": ("乙容器的虚构刻度为8 cm。", ["仅用于室温演示", "空盒时不读数", "浸水后不适用"], ["P5b"]),
}
FIXED_EVIDENCE = {
    "E3a": ("P3a", 0, 40, "1494edf3abcd6cc4b91d64c2974a22de422c00686be1be8c06af244d70934821"),
    "E5a": ("P5a", 0, 104, "eb2a2eb1f537be777839c6f1bb8146c1bd7ab2db82e39a06744261f51c0dd782"),
    "E3b": ("P3b", 0, 40, "da6274343ab5816281a39d505287bc814db7b6ac9573feb2535c94078e3e484d"),
    "E5b": ("P5b", 0, 101, "a87c6fd67f5c4c1469d0bef308e17f1f1a121e2fa38f2489607cd62f9fdd3f8f"),
    "E7b": ("P7b", 0, 40, "1494edf3abcd6cc4b91d64c2974a22de422c00686be1be8c06af244d70934821"),
}
FIXED_CANONICAL = [
    {"concept_id": "C2f", "note_id": "N4u", "section_id": "H2j", "atom_ids": ["A3d"]},
    {"concept_id": "C6h", "note_id": "N8v", "section_id": "H6n", "atom_ids": ["A7e"]},
]
REVIEW_EXPECTED_SHA = "0477e6dc10ee3e5be654a3bbce4c22cb214972bf658a58e9f8a0cf16db39eadd"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_json(name):
    return json.loads((ROOT / name).read_text(encoding="utf-8"))


def audit():
    manifest = read_json("case.json")
    ir = read_json("ir.json")
    plan = read_json("note-plan.json")
    expected = read_json("expected.json")
    assert sha((ROOT / "ir.json").read_bytes()) == "3dd1e99ef340d1f603c9514be9e5a3b2b2b2225135f0397d20bcffbe7577fc50"
    assert sha((ROOT / "expected.json").read_bytes()) == REVIEW_EXPECTED_SHA
    assert plan["knowledge_sha256"] == sha((ROOT / "ir.json").read_bytes())
    assert all(package["producer"] == "fixture" for package in [manifest, ir, plan, expected])
    actual = {}
    for source in manifest["sources"]:
        data = (ROOT / source["relative_path"]).read_bytes()
        assert b"\r" not in data and not data.startswith(b"\xef\xbb\xbf")
        assert sha(data) == source["content_sha256"] == source["normalized_sha256"] == FIXED_SOURCE_SHA[source["source_id"]]
        fixed = FIXED_SPANS[source["source_id"]]
        assert len(source["spans"]) == len(fixed)
        cursor = 0
        for ordinal, (declared, (pid, start, end, kind)) in enumerate(zip(source["spans"], fixed)):
            assert start == cursor
            assert declared["span_id"] == pid and declared["ordinal"] == ordinal
            assert declared["start_byte"] == start and declared["end_byte"] == end
            assert declared["kind"] == kind and declared["parse_status"] == "parsed"
            assert declared["start_line"] == declared["end_line"] == ordinal + 1
            raw = data[start:end]
            raw.decode("utf-8")  # Fixed interval must start/end at character boundaries.
            assert raw.count(b"\n") == 1 and raw.endswith(b"\n")
            assert sha(raw) == declared["text_sha256"]
            actual[pid] = raw
            cursor = end
        assert cursor == len(data)
    assert actual["P3a"] == actual["P7b"]
    for package in [ir, expected]:
        assert len(package["evidence"]) == len(FIXED_EVIDENCE)
        assert {e["evidence_id"] for e in package["evidence"]} == set(FIXED_EVIDENCE)
        for evidence in package["evidence"]:
            fixed = FIXED_EVIDENCE[evidence["evidence_id"]]
            assert (evidence["source_span_id"], evidence["excerpt_start_byte"],
                    evidence["excerpt_end_byte"], evidence["excerpt_sha256"]) == fixed
            raw = actual[evidence["source_span_id"]]
            assert evidence["excerpt_start_byte"] == 0 and evidence["excerpt_end_byte"] == len(raw)
            assert evidence["excerpt_sha256"] == sha(raw)
    assert expected["canonical_placements"] == FIXED_CANONICAL
    assert {value["atom_id"] for value in expected["required_atoms"]} == set(FIXED_ATOMS)
    for value in expected["required_atoms"]:
        text, qualifiers, positions = FIXED_ATOMS[value["atom_id"]]
        assert (value["content"], value["qualifiers"], value["evidence_span_ids"]) == (text, qualifiers, positions)
    print("Author audit passed: 2 fixed sources, 12 fixed spans, 5 full-position evidence slices in IR and independent expected, 4 fixed required atoms, 2 canonical explanation-atom declarations; scientific quality not evaluated.")
    print(f"Reviewed independent expected SHA256: {REVIEW_EXPECTED_SHA}")


if __name__ == "__main__":
    audit()
