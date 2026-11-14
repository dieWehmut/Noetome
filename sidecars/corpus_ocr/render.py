"""Render one original physical PDF page, retaining the text worker's geometry.

PDFium reads a file path, not whole-book bytes. Its bitmap uses the visible,
already rotated CropBox. UserUnit is explicitly included in the DPI scale.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import math
from pathlib import Path


def sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def geometry_for_pdf(path: Path, page_number: int) -> dict:
    # Reuse the production text adapter's inherited page tree and transform.
    from pdfminer.pdfdocument import PDFDocument
    from pdfminer.pdfparser import PDFParser
    from pdfminer.pdfpage import PDFPage
    from pdfminer.pdftypes import resolve1
    spec = importlib.util.spec_from_file_location(
        "noetome_pdf_geometry", Path(__file__).parents[1] / "corpus_pdf" / "worker.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with path.open("rb") as source:
        document = PDFDocument(PDFParser(source))
        for number, (object_id, attrs) in enumerate(module.page_records(document), 1):
            if number == page_number:
                return module.geometry_for(PDFPage(document, object_id, attrs, None), resolve1)
    raise ValueError("Physical page number is outside the original PDF")


def render_page(path: Path, page_number: int, target: Path, dpi: int = 144,
                geometry: dict | None = None, document=None) -> dict:
    import pypdfium2 as pdfium
    if type(page_number) is not int or page_number < 1:
        raise ValueError("page_number must be a positive physical page number")
    if type(dpi) is not int or not 72 <= dpi <= 600:
        raise ValueError("render_dpi must be an integer from 72 to 600")
    geometry = geometry or geometry_for_pdf(path, page_number)
    owned_document = document is None
    document = document or pdfium.PdfDocument(str(path))
    page = bitmap = None
    try:
        page = document[page_number - 1]
        width, height = page.get_size()
        unit = geometry["user_unit"]
        if page.get_rotation() != geometry["rotation"] or any(
                abs(a * unit - b) > .1 for a, b in
                zip((width, height), (geometry["width_pt"], geometry["height_pt"]))):
            raise ValueError("PDFium rendered geometry differs from original CropBox")
        pixel_width = math.ceil(width * unit * dpi / 72)
        pixel_height = math.ceil(height * unit * dpi / 72)
        if pixel_width * pixel_height > 80_000_000:
            raise ValueError("Page render exceeds 80 million pixels; lower the explicit DPI")
        bitmap = page.render(scale=unit * dpi / 72, draw_annots=True,
                             limit_image_cache=True)
        target.parent.mkdir(parents=True, exist_ok=True)
        image = bitmap.to_pil()
        image.save(target, format="PNG", dpi=(dpi, dpi))
        return {"schema_version": 1, "page_number": page_number,
                "render_dpi": dpi, "width_px": image.width, "height_px": image.height,
                "geometry": geometry, "render_sha256": sha256(target),
                "pixel_to_display": [geometry["width_pt"] / image.width, 0, 0,
                                     geometry["height_pt"] / image.height, 0, 0]}
    finally:
        if bitmap is not None:
            bitmap.close()
        if page is not None:
            page.close()
        if owned_document:
            document.close()


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selection", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    selection = json.loads(args.selection.read_text(encoding="utf-8"))
    args.output.mkdir(parents=True, exist_ok=False)
    records, digests = [], {}
    for item in selection["items"]:
        path = args.source_root / item["file"]
        if str(path) not in digests:
            digests[str(path)] = sha256(path)
        if digests[str(path)] != item["source_sha256"]:
            raise ValueError("Source changed after page selection was frozen")
        target = args.output / (item["case_id"] + ".png")
        record = render_page(path, item["page_number"], target, selection["render_dpi"])
        record.update(case_id=item["case_id"], source_sha256=item["source_sha256"])
        records.append(record)
        (args.output / (item["case_id"] + ".json")).write_text(
            json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print("RENDERED " + item["case_id"], flush=True)
    (args.output / "manifest.json").write_text(json.dumps({
        "schema_version": 1, "selection_sha256": sha256(args.selection),
        "items": records}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
