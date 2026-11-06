"""Author a new public self-owned SVG capability probe, not a product result."""
import argparse
import json
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("new_root", type=Path)
args = parser.parse_args()
root = args.new_root
root.mkdir()
selected = root / "selected"
selected.mkdir()
svg = '''<svg xmlns="http://www.w3.org/2000/svg" width="760" height="430" viewBox="0 0 760 430">
<rect width="760" height="430" fill="white"/>
<defs><marker id="arrow" markerWidth="12" markerHeight="12" refX="10" refY="6" orient="auto"><path d="M 0 0 L 10 6 L 0 12 Z" fill="#222"/></marker></defs>
<rect id="inlet" x="50" y="80" width="180" height="100" fill="#dceef9" stroke="#222"/>
<rect id="outlet" x="490" y="80" width="180" height="100" fill="#fde4d5" stroke="#222"/>
<text id="inlet-label" x="80" y="135" font-family="Arial" font-size="22">Inlet (+)</text>
<text id="outlet-label" x="520" y="135" font-family="Arial" font-size="22">Outlet (-)</text>
<line id="flow" x1="230" y1="130" x2="490" y2="130" stroke="#222" stroke-width="3" marker-end="url(#arrow)"/>
<text id="flow-label" x="290" y="105" font-family="Arial" font-size="22">2 mL/min</text>
<circle id="probe" cx="360" cy="275" r="35" fill="#ddf0df" stroke="#222"/>
<text id="probe-label" x="320" y="340" font-family="Arial" font-size="22">Probe</text>
<rect id="legend-color" x="50" y="355" width="25" height="25" fill="#ddf0df" stroke="#222"/>
<text id="legend" x="90" y="376" font-family="Arial" font-size="20">Green = sensor</text>
</svg>'''
(selected / "probe.svg").write_text(svg, encoding="utf8")
image = Image.new("RGB", (760, 430), "white")
draw = ImageDraw.Draw(image)
font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 22)
draw.rectangle((50, 80, 230, 180), fill="#dceef9", outline="#222", width=2)
draw.rectangle((490, 80, 670, 180), fill="#fde4d5", outline="#222", width=2)
draw.text((80, 114), "Inlet (+)", fill="#222", font=font)
draw.text((520, 114), "Outlet (-)", fill="#222", font=font)
draw.line((230, 130, 480, 130), fill="#222", width=3)
draw.polygon(((480, 118), (490, 130), (480, 142)), fill="#222")
draw.text((290, 84), "2 mL/min", fill="#222", font=font)
draw.ellipse((325, 240, 395, 310), fill="#ddf0df", outline="#222", width=2)
draw.text((320, 319), "Probe", fill="#222", font=font)
draw.rectangle((50, 355, 75, 380), fill="#ddf0df", outline="#222", width=2)
draw.text((90, 355), "Green = sensor", fill="#222", font=font)
image.save(selected / "probe-original.png")
(selected / "Probe.md").write_text("# Public self-owned SVG capability probe\n\nThis authored probe does not prove final CLI image output.\n\n![[probe.svg]]\n\n![[probe-original.png]]\n\n[[probe-original.png|Open selected local source PNG]]\n", encoding="utf8")
spec = {"case": "svg-capability-probe", "scope": "self-owned capability probe; actual installed Obsidian; not final product output", "require_mathjax_runtime": False,
"notes": [{"file": "Probe.md", "math_trees": [], "required_texts": ["This authored probe does not prove final CLI image output."],
"images": [{"file": "probe.svg", "width": 760, "height": 430}, {"file": "probe-original.png", "width": 760, "height": 430}],
"svg_semantics": {"file": "probe.svg", "labels": ["Inlet (+)", "Outlet (-)", "2 mL/min", "Probe", "Green = sensor"],
"relations": [{"kind": "left_of", "a": "inlet", "b": "outlet"}, {"kind": "above", "a": "inlet", "b": "probe"},
{"kind": "arrow_right", "a": "flow", "marker": "arrow"}]},
"link_destinations": ["probe-original.png"], "click_attachments": ["probe-original.png"]}]}
(root / "capture-spec.json").write_text(json.dumps(spec, indent=2) + "\n", encoding="utf8")
print(json.dumps({"root": str(root), "selected": str(selected)}))
