"""Author deterministic, public PDF fixtures WITHOUT the parser under test.

The uncompressed PDF page tree and drawing commands are independently readable.
Expected text, page counts, baselines and Helvetica widths come from authoring,
not extraction. ReportLab is used only for font authoring metrics and the
encrypted boundary; Pillow only creates an image with no PDF text layer.
"""
from __future__ import annotations

import hashlib
import json
import zlib
from pathlib import Path

from PIL import Image, ImageDraw
from reportlab.lib.pdfencrypt import StandardEncryption
from reportlab.pdfbase.pdfmetrics import stringWidth
from reportlab.pdfgen import canvas

ROOT = Path(__file__).resolve().parent


def literal(text: str) -> bytes:
    data = text.encode("cp1252")
    return b"(" + data.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)") + b")"


def text_commands(lines: list[tuple[str, float, float, float]]) -> bytes:
    return b"\n".join(
        f"BT /F1 {size:g} Tf 1 0 0 1 {x:g} {baseline:g} Tm ".encode()
        + literal(text) + b" Tj ET" for text, x, baseline, size in lines
    ) + b"\n"


def make_pdf(pages: list[dict], image: bytes | None = None) -> bytes:
    objects: list[bytes] = [b"", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"]
    image_ref = None
    if image is not None:
        image_ref = len(objects) + 1
        compressed = zlib.compress(image)
        objects.append(b"<< /Type /XObject /Subtype /Image /Width 240 /Height 80 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length " + str(len(compressed)).encode() + b" >>\nstream\n" + compressed + b"\nendstream")
    page_refs = []
    for page in pages:
        page_ref = len(objects) + 1
        content_ref = page_ref + 1
        page_refs.append(page_ref)
        media = page.get("media", [0, 0, 600, 800])
        box = " ".join(str(v) for v in media)
        crop = page.get("crop", media)
        crop_str = " ".join(str(v) for v in crop)
        resource = b" /XObject << /Im1 " + f"{image_ref} 0 R >>".encode() if image_ref else b""
        unit_entry = f" /UserUnit {page['user_unit']:g}".encode() if "user_unit" in page else b""
        objects.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [{box}] /CropBox [{crop_str}] /Rotate {page.get('rotation', 0)} /Resources << /Font << /F1 3 0 R >>".encode()
            + resource + f" >> /Contents {content_ref} 0 R".encode() + unit_entry + b" >>"
        )
        content = page.get("content", text_commands(page.get("lines", [])))
        objects.append(b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"endstream")
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objects[1] = b"<< /Type /Pages /Count " + str(len(page_refs)).encode() + b" /Kids [" + b" ".join(f"{p} 0 R".encode() for p in page_refs) + b"] >>"
    header = b"%PDF-1.7\n" if any(page.get("user_unit", 1) != 1 for page in pages) else b"%PDF-1.4\n"
    result = bytearray(header + b"%\xe2\xe3\xcf\xd3\n")
    offsets = [0]
    for index, obj in enumerate(objects, 1):
        offsets.append(len(result))
        result.extend(f"{index} 0 obj\n".encode() + obj + b"\nendobj\n")
    start = len(result)
    result.extend(f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode())
    result.extend(b"".join(f"{offset:010d} 00000 n \n".encode() for offset in offsets[1:]))
    result.extend(f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n".encode())
    return bytes(result)


def expected_line(text: str, x: float, baseline: float, size: float, crop: list[float]) -> dict:
    # Helvetica authoring metrics, independently of pdfplumber/pdfminer.
    left, _, _, crop_top = crop
    return {
        "text": text,
        "bbox": [round(x-left, 6), round(crop_top-baseline-size*.793, 6),
                 round(x-left+stringWidth(text, "Helvetica", size), 6),
                 round(crop_top-baseline+size*.207, 6)],
        "tolerance_pt": 0.08,
        "authoring": {"font": "Helvetica", "size": size, "x": x, "baseline": baseline},
    }


def expected_geometry(crop: list[float], rotation: int, unit: float) -> dict:
    # Independently authored affine map: x' = ax+cy+e; y' = bx+dy+f.
    left, bottom, right, top = crop
    matrices = {
        0: [unit, 0, 0, -unit, -unit*left, unit*top],
        90: [0, unit, unit, 0, -unit*bottom, -unit*left],
        180: [-unit, 0, 0, unit, unit*right, -unit*bottom],
        270: [0, -unit, -unit, 0, unit*top, unit*right],
    }
    width, height = (right-left)*unit, (top-bottom)*unit
    if rotation in [90, 270]:
        width, height = height, width
    return {"pdf_to_display": matrices[rotation], "width_pt": width, "height_pt": height, "user_unit": unit}


def expected_glyphs(lines: list[tuple[str, float, float, float]], crop: list[float], rotation: int, unit: float) -> list[dict]:
    matrix = expected_geometry(crop, rotation, unit)["pdf_to_display"]
    a, b, c, d, e, f = matrix
    result = []
    for text, x, baseline, size in lines:
        for index, char in enumerate(text):
            raw_x = x + stringWidth(text[:index], "Helvetica", size)
            raw_right = raw_x + stringWidth(char, "Helvetica", size)
            raw_bottom, raw_top = baseline-size*.207, baseline+size*.793
            points = [(a*px+c*py+e, b*px+d*py+f) for px in [raw_x, raw_right] for py in [raw_bottom, raw_top]]
            result.append({"ordinal": len(result), "text": char,
                           "bbox": [round(min(p[0] for p in points), 6), round(min(p[1] for p in points), 6),
                                    round(max(p[0] for p in points), 6), round(max(p[1] for p in points), 6)],
                           "tolerance_pt": 0.08})
    return result


def main() -> None:
    cases = []

    def add(name: str, category: str, pages: list[dict], statuses: list[str], *, image: bytes | None = None, page_count_known: bool = True) -> None:
        data = make_pdf(pages, image)
        (ROOT / name).write_bytes(data)
        expected_pages = []
        for index, (page, status) in enumerate(zip(pages, statuses, strict=True)):
            media = page.get("media", [0, 0, 600, 800])
            crop = page.get("crop", media)
            rotation = page.get("rotation", 0)
            unit = page.get("user_unit", 1)
            lines = page.get("expected_lines", page.get("lines", []))
            glyph_lines = page.get("lines", page.get("expected_lines", []))
            valid_geometry = all(isinstance(value, (int, float)) for value in crop)
            expected_pages.append({
                "physical_page_index": index, "display_page_number": index+1,
                "printed_page_label": None, "status": status,
                "media_box": media, "crop_box": crop, "rotation_degrees": rotation,
                "geometry": expected_geometry(crop, rotation, unit) if valid_geometry else None,
                "glyph_positions": expected_glyphs(glyph_lines, crop, rotation, unit) if valid_geometry and status != "failed" else [],
                "lines": [expected_line(*line, crop) for line in lines] if rotation == 0 and status == "parsed" else [],
            })
        cases.append({"file": name, "category": category, "raw_sha256": hashlib.sha256(data).hexdigest(),
                      "byte_length": len(data), "authoring_page_count": len(pages),
                      "parse_page_count": len(pages) if page_count_known else None,
                      "status": "failed" if "failed" in statuses or not page_count_known else "needs_review" if "needs_review" in statuses else "parsed",
                      "pages": expected_pages})

    single = [("Noetome PDF fixture - physical page one", 72, 728, 12),
              ("Alpha keeps its unit: 12 mg at room temperature.", 72, 698, 12),
              ("Caf\u00e9 has UTF8 text; filename digits are not page labels.", 72, 668, 12)]
    add("single-page.pdf", "single_column", [{"lines": single}], ["parsed"])
    multi = [
        {"lines": [("Physical page one - introduction", 72, 728, 12), ("First: retain every source page.", 72, 698, 12)]},
        {"lines": [("Physical page two - middle", 72, 728, 12), ("Second: keep 25 mL under the stated condition.", 72, 698, 12)]},
        {"lines": [("Physical page three - last", 72, 728, 12), ("Third: the exception remains visible.", 72, 698, 12)]},
    ]
    add("multiple-pages.pdf", "multiple_pages", multi, ["parsed"]*3)
    add("crop-offset.pdf", "crop_media_offset", [{"media": [20, 30, 620, 830], "crop": [60, 90, 580, 790],
        "lines": [("CropBox text - the origin is the visible top left", 100, 730, 12), ("MediaBox offset is not a text byte offset.", 100, 700, 12)]}], ["parsed"])
    rotation_pages = [{"rotation": rotation, "lines": [(f"Rotate {rotation} - same native baseline", 72, 728, 12)]} for rotation in [0, 90, 180, 270]]
    rotation_pages.extend({"rotation": rotation, "media": [20, 30, 620, 830], "crop": [60, 90, 580, 790],
                           "lines": [(f"Rotated CropBox {rotation} - offset evidence", 100, 730, 12)]} for rotation in [90, 180, 270])
    rotation_pages.append({"user_unit": 2, "lines": [("UserUnit two - physical pt require scaling", 72, 728, 12)]})
    add("rotations.pdf", "rotation_crop_and_user_unit", rotation_pages, ["parsed"]+["needs_review"]*7)
    raster = Image.new("RGB", (240, 80), "white")
    ImageDraw.Draw(raster).text((20, 30), "RASTER ONLY - NO TEXT LAYER", fill="black")
    image_bytes = raster.tobytes()
    image_commands = b"q 360 0 0 120 72 600 cm /Im1 Do Q\n"
    add("scanned-page.pdf", "scan_no_text_layer", [{"content": image_commands}], ["needs_review"], image=image_bytes)
    add("blank-page.pdf", "unknown_blank", [{"content": b"\n"},
        {"lines": [("   ", 72, 728, 12)]}], ["needs_review"]*2)
    damaged = b"%PDF-1.4\nDeliberately damaged fixture; no page tree or xref.\n"
    (ROOT / "damaged.pdf").write_bytes(damaged)
    cases.append({"file": "damaged.pdf", "category": "damaged", "raw_sha256": hashlib.sha256(damaged).hexdigest(), "byte_length": len(damaged), "authoring_page_count": None, "parse_page_count": None, "status": "failed", "pages": []})
    encryption = StandardEncryption("fixture-only", ownerPassword="fixture-owner", canPrint=1, strength=128)
    encrypted_path = ROOT / "encrypted.pdf"
    encrypted = canvas.Canvas(str(encrypted_path), pagesize=(600, 800), invariant=1, pageCompression=0, encrypt=encryption)
    encrypted.setTitle("Noetome public encrypted boundary fixture")
    encrypted.drawString(72, 728, "Encrypted fixture - password is not supplied to the parser.")
    encrypted.showPage()
    encrypted.save()
    encrypted_bytes = encrypted_path.read_bytes()
    cases.append({"file": "encrypted.pdf", "category": "encrypted", "raw_sha256": hashlib.sha256(encrypted_bytes).hexdigest(), "byte_length": len(encrypted_bytes), "authoring_page_count": 1, "parse_page_count": None, "status": "failed", "pages": []})
    table_lines = [("Name", 80, 710, 12), ("Value", 300, 710, 12), ("Alpha", 80, 680, 12), ("12 mg", 300, 680, 12)]
    table_content = text_commands(table_lines) + b"72 660 420 72 re S\n282 660 m 282 732 l S\n72 696 m 492 696 l S\n"
    add("table-page.pdf", "table", [{"content": table_content, "expected_lines": table_lines}], ["needs_review"])
    formula_x = 72 + stringWidth("Formula: E = mc", "Helvetica", 12)
    formula = text_commands([("Formula: E = mc", 72, 728, 12), ("2", formula_x, 734, 8)])
    add("formula-page.pdf", "formula_superscript", [{"content": formula, "expected_lines": [("Formula: E = mc", 72, 728, 12), ("2", formula_x, 734, 8)]}], ["needs_review"])
    image_lines = [("Image association must be reviewed.", 72, 728, 12)]
    add("image-page.pdf", "image_with_text", [{"content": text_commands(image_lines) + image_commands, "expected_lines": image_lines}], ["needs_review"], image=image_bytes)
    columns = [("Left column first", 72, 728, 12), ("Left column second", 72, 698, 12), ("Right column first", 340, 728, 12), ("Right column second", 340, 698, 12)]
    staggered_columns = [("Left first paragraph.", 72, 728, 12), ("Left continuation.", 72, 698, 12),
                         ("Right first paragraph.", 340, 716, 12), ("Right continuation.", 340, 686, 12)]
    add("columns-page.pdf", "multiple_columns", [{"lines": columns}, {"lines": staggered_columns}], ["needs_review"]*2)
    partial = [{"lines": [("Confirmed first page survives a later page error.", 72, 728, 12)]},
               {"media": [0, 0, "(broken)", 800], "lines": [("This page has a deliberately malformed MediaBox.", 72, 728, 12)]}]
    add("partial-page-failure.pdf", "partial_page_failure", partial, ["parsed", "failed"])
    ordered = [("First visible line", 72, 728, 12), ("Second visible line", 72, 698, 12)]
    add("layout-order.pdf", "drawing_order_differs", [{"lines": list(reversed(ordered)), "expected_lines": ordered}], ["parsed"])
    expected = {"schema_version": 1, "producer": "authoring_fixture", "review_basis": "uncompressed page tree, literal drawing text, authored baselines/Helvetica widths; initial Poppler review before adapter and explicit review regressions before accepting fixes", "cases": cases}
    (ROOT / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2)+"\n", encoding="utf8", newline="\n")
    print(f"Authored {len(cases)} fixed fixtures; expected.json does not use parser output.")


if __name__ == "__main__":
    main()
