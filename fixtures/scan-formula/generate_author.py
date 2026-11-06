#!/usr/bin/env python3
"""Create the self-authored scan fixture in a brand-new directory, never from OCR."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import zlib

from PIL import Image, ImageDraw, ImageFont


PAGE = (800, 1000)
BOUNDS = (120, 175, 560, 200)
FORMULA = r"x_i = -\frac{a_i}{b_i}"
TOKENS = ["x", "_", "i", "=", "-", "\\frac", "{", "a", "_", "i", "}", "{", "b", "_", "i", "}"]
SERIF_SHA = "3731a19f2b50d86d81dd1a1effcd81492ed1fa44f6ea904792dd98fdae9b3dfe"
SANS_SHA = "690243adfefe0ce154b547db6205794bd30ac4277275179517a90994f4980648"
SYMBOLS = [
    {"symbol": "x", "definition": "Reported displacement.", "unit": "mg"},
    {"symbol": "a", "definition": "Input amount.", "unit": "mg"},
    {"symbol": "b", "definition": "Positive scale.", "unit": "dimensionless"},
    {"symbol": "i", "definition": "Sample index.", "unit": "dimensionless"},
]
CONDITIONS = ["a_i >= 0.", "b_i > 0."]
RANGES = ["i in {1, 2}.", "x_i <= 0."]
EXCEPTIONS = ["If b_i = 0, the expression is undefined."]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def pdf_from_image(image: Image.Image) -> bytes:
    """A one-page PDF with one lossless RGB image and no text/font objects."""
    payload = zlib.compress(image.convert("RGB").tobytes(), level=9)
    content = b"q\n800 0 0 1000 0 0 cm\n/ScanPage Do\nQ\n"
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 800 1000] /Resources << /XObject << /ScanPage 4 0 R >> >> /Contents 5 0 R >>",
        (b"<< /Type /XObject /Subtype /Image /Width 800 /Height 1000 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length "
         + str(len(payload)).encode() + b" >>\nstream\n" + payload + b"\nendstream"),
        b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"endstream",
        b"<< /Title (Self-authored bounded scalar scan) /Author (Noetome independent fixture author) /Producer (Pillow raster author; CC0 content) >>",
    ]
    data = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = [0]
    for number, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(str(number).encode() + b" 0 obj\n" + obj + b"\nendobj\n")
    xref = len(data)
    data.extend(b"xref\n0 7\n0000000000 65535 f \n")
    for offset in offsets[1:]:
        data.extend(f"{offset:010d} 00000 n \n".encode())
    data.extend(b"trailer\n<< /Size 7 /Root 1 0 R /Info 6 0 R >>\nstartxref\n" + str(xref).encode() + b"\n%%EOF\n")
    return bytes(data)


def make_region(serif: Path) -> tuple[Image.Image, dict[str, list[int]]]:
    image = Image.new("RGB", BOUNDS[2:], "white")
    draw = ImageDraw.Draw(image)
    main = ImageFont.truetype(str(serif), 68)
    sub = ImageFont.truetype(str(serif), 37)

    def indexed(letter: str, x: int, baseline: int) -> list[int]:
        draw.text((x, baseline), letter, font=main, fill="black", anchor="ls")
        offset = round(draw.textlength(letter, font=main)) + 2
        subbox = [x + offset, baseline - 14, 21, 38]
        draw.text((x + offset, baseline + 20), "i", font=sub, fill="black", anchor="ls")
        return subbox

    left_sub = indexed("x", 80, 122)
    draw.text((185, 122), "=", font=main, fill="black", anchor="ls")
    minus = [270, 92, 38, 12]
    draw.line((272, 98, 305, 98), fill="black", width=4)
    numerator_sub = indexed("a", 341, 74)
    draw.line((323, 106, 442, 106), fill="black", width=4)
    denominator_sub = indexed("b", 341, 163)
    return image, {"left_subscript": left_sub, "minus_before_fraction": minus,
                   "numerator_subscript": numerator_sub, "denominator_subscript": denominator_sub}


def occlude(image: Image.Image, bounds: list[int]) -> None:
    """Opaque paper loss deliberately contains no recoverable original glyph."""
    draw = ImageDraw.Draw(image)
    x, y, width, height = bounds
    draw.rectangle((x - 3, y - 3, x + width + 3, y + height + 3), fill=(225, 225, 225))
    draw.line((x - 3, y - 3, x + width + 3, y + height + 3), fill=(180, 180, 180), width=2)
    draw.line((x + width + 3, y - 3, x - 3, y + height + 3), fill=(180, 180, 180), width=2)


def make_page(region: Image.Image, sans: Path, state: str) -> tuple[Image.Image, list[dict[str, object]]]:
    page = Image.new("RGB", PAGE, "white")
    draw = ImageDraw.Draw(page)
    regular = ImageFont.truetype(str(sans), 18)
    small = ImageFont.truetype(str(sans), 15)
    title = ImageFont.truetype(str(sans), 26)
    draw.text((55, 45), "A bounded signed scalar expression", font=title, fill="black")
    draw.text((55, 90), "Self-authored scan-style engineering example; public CC0 content.", font=small, fill="black")
    draw.text((55, 118), "One selected formula; this page contains no hidden PDF text.", font=small, fill="black")
    page.paste(region, BOUNDS[:2])
    lines: list[dict[str, object]] = []
    y = 415
    for header, values in [
        ("Symbols and units", [f"{s['symbol']}: {s['definition']} Unit: {s['unit']}." for s in SYMBOLS]),
        ("Conditions", CONDITIONS), ("Ranges", RANGES), ("Exception", EXCEPTIONS),
    ]:
        draw.text((65, y), header, font=regular, fill="black")
        y += 32
        for value in values:
            box = draw.textbbox((85, y), value, font=regular)
            draw.text((85, y), value, font=regular, fill="black")
            lines.append({"text": value, "bounds": [box[0], box[1], box[2]-box[0], box[3]-box[1]], "section": header})
            y += 29
        y += 18
    if state != "clear":
        draw.text((55, 927), "A grey crossed patch marks missing scan material in the selected formula.", font=small, fill="black")
        draw.text((55, 950), "The covered symbol is unknown; recover it only from a separately verified source.", font=small, fill="black")
    else:
        draw.text((55, 950), "All selected formula marks are clear in this author original.", font=small, fill="black")
    return page, lines


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-root", type=Path, required=True, help="must not exist")
    parser.add_argument("--serif-font", type=Path, required=True)
    parser.add_argument("--sans-font", type=Path, required=True)
    parser.add_argument("--font-license", type=Path, required=True)
    args = parser.parse_args()
    if args.output_root.exists():
        parser.error("output root already exists; frozen originals are never overwritten")
    for path, expected in [(args.serif_font, SERIF_SHA), (args.sans_font, SANS_SHA)]:
        if sha(path.read_bytes()) != expected:
            parser.error(f"font fingerprint mismatch: {path.name}")
    args.output_root.mkdir()
    shutil.copyfile(args.font_license, args.output_root / "author-font-license.txt")
    clear, positions = make_region(args.serif_font)
    facts = {"schema_version": 1, "producer": "author_scan_source_facts", "license": "CC0-1.0",
             "symbols": SYMBOLS, "conditions": CONDITIONS, "ranges": RANGES, "exceptions": EXCEPTIONS,
             "statement": "Manually authored page facts; not extracted by OCR and not an independent verification result."}
    write_json(args.output_root / "author-source-facts.json", facts)
    layouts = {}
    for state, missing in [("clear", None), ("unreadable_sign", "minus_before_fraction"),
                           ("unreadable_subscript", "denominator_subscript")]:
        root = args.output_root / "originals" / state
        root.mkdir(parents=True)
        region = clear.copy()
        if missing is not None:
            occlude(region, positions[missing])
        page, lines = make_page(region, args.sans_font, state)
        page.save(root / "page.png", format="PNG", optimize=False, compress_level=9)
        region.save(root / "region.png", format="PNG", optimize=False, compress_level=9)
        (root / "page.pdf").write_bytes(pdf_from_image(page))
        layouts[state] = {"canvas": list(PAGE), "page": 1, "bounds": list(BOUNDS), "pdf_media_box": [0,0,*PAGE],
                          "formula_mark_bounds_in_region": positions, "context_lines": lines,
                          "missing_mark": missing, "occlusion_bounds_in_region": None if missing is None else positions[missing]}
        expected = {"schema_version": 1, "producer": "independent_author_expected", "author_case": state,
                    "author_formula_count": 1, "physical_page": 1, "canvas": list(PAGE), "bounds": list(BOUNDS),
                    "originals": {p.name: {"sha256": sha(p.read_bytes()), "byte_length": p.stat().st_size} for p in sorted(root.iterdir())},
                    "expression": FORMULA if missing is None else None, "tokens": TOKENS if missing is None else None,
                    "symbols": SYMBOLS, "conditions": CONDITIONS, "ranges": RANGES, "exceptions": EXCEPTIONS,
                    "legibility": "clear" if missing is None else "unreadable", "unknown_positions": [] if missing is None else [missing],
                    "native_representation": "native_math" if missing is None else None,
                    "no_native_representation": "formula_image" if missing is None else None,
                    "success_allowed": missing is None,
                    "meaning": "One signed scalar expression plus required page context; unknown covered glyphs cannot be inferred from the clear counterpart."}
        write_json(args.output_root / f"expected-{state}.json", expected)
    write_json(args.output_root / "author-layout.json", {"schema_version": 1, "states": layouts,
               "fonts": {"DejaVuSerif.ttf": SERIF_SHA, "DejaVuSans.ttf": SANS_SHA},
               "pdf_structure": "one physical page; one lossless RGB image; no font objects or text layer"})
    files = {p.relative_to(args.output_root).as_posix(): {"sha256": sha(p.read_bytes()), "byte_length": p.stat().st_size}
             for p in sorted(args.output_root.rglob("*")) if p.is_file()}
    write_json(args.output_root / "frozen-sha256.json", {"schema_version": 1, "producer": "independent_author_freeze",
               "license": "CC0-1.0", "author_formula_count": 1, "scan_states": 3,
               "freeze_stage": "before any recognition and before first normal scan CLI RED", "files": files})
    print(json.dumps({"output_root": str(args.output_root), "files": len(files),
                      "freeze_sha256": sha((args.output_root / "frozen-sha256.json").read_bytes())}))


if __name__ == "__main__":
    main()
