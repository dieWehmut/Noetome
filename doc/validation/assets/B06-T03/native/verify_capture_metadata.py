"""Independently inspect recorded native DOM for heading/label/value association."""
import argparse
import json
from html.parser import HTMLParser
from pathlib import Path


class Events(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.depth = 0
        self.active = []
        self.events = []

    def handle_starttag(self, tag, attrs):
        # These recorded Obsidian nodes only need text; void tags do not
        # contribute depth because HTMLParser emits no corresponding end tag.
        if tag in {"br", "hr", "img", "input", "meta", "link", "source"}:
            return
        self.depth += 1
        if tag in {"h1", "h2", "h3", "p", "code"}:
            self.active.append({"tag": tag, "depth": self.depth, "text": ""})

    def handle_data(self, data):
        for e in self.active:
            e["text"] += data

    def handle_endtag(self, tag):
        remaining = []
        for e in self.active:
            if e["tag"] == tag and e["depth"] == self.depth:
                self.events.append((tag, e["text"]))
            else:
                remaining.append(e)
        self.active = remaining
        self.depth -= 1


def contiguous(events, expected):
    for i in range(len(events) - len(expected) + 1):
        if events[i:i + len(expected)] == expected:
            return True
    return False


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("capture_roots", type=Path)
parser.add_argument("new_output", type=Path)
args = parser.parse_args()
roots = json.loads(args.capture_roots.read_bytes())["successful_cases"]
records = []
for name, root_value in roots.items():
    root = Path(root_value)
    result = json.loads((root / "result.json").read_bytes())
    spec = json.loads((root / "visual-input.json").read_bytes())["spec"]
    assert result["status"] == "passed"
    if spec["representation"] is None:
        continue
    semantics = spec["semantic_source"]
    snapshots = []
    for sample in result["notes"][0]["actual"]["samples"]:
        events = Events()
        events.feed(sample["dom"])
        snapshots.append(events.events)
    checked = []
    for i, s in enumerate(semantics["symbols"]):
        expected = [("h3", "符号 " + str(i + 1)),
                    ("p", "符号："), ("code", s["symbol"] + "\n"),
                    ("p", "定义："), ("code", s["definition"] + "\n"),
                    ("p", "单位："), ("code", (s["unit"] or "来源未给出单位") + "\n")]
        assert any(contiguous(events, expected) for events in snapshots), (name, "native label association", expected)
        checked.append({"symbol_index": i, "symbol": s["symbol"], "heading": "符号 " + str(i + 1), "definition": s["definition"], "unit": s["unit"], "actual_native_contiguous_events_match": True})
    context = []
    for heading, field in [("适用前提", "conditions"), ("范围", "ranges"), ("例外", "exceptions")]:
        expected = [("h2", heading)] + [("code", s + "\n") for s in semantics[field]]
        if not semantics[field]:
            expected.append(("p", "选定来源未列出。"))
        assert any(contiguous(events, expected) for events in snapshots), (name, "native context association", expected)
        context.append({"heading": heading, "field": field, "values": semantics[field], "actual_native_contiguous_events_match": True})
    records.append({"case": name, "capture_root": str(root), "symbols": checked, "context": context})
with args.new_output.open("x", encoding="utf8") as f:
    json.dump({"status": "passed", "cases": records, "method": "parse actual recorded scoped Obsidian DOM events; require a contiguous observed heading/label/value group rather than flattening all visible strings"}, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(json.dumps({"status": "passed", "cases": len(records), "symbol_groups": sum(len(r["symbols"]) for r in records)}))
