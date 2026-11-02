"""Independent Poppler review of fixed authoring fixtures, BEFORE extraction.

Requires Pillow for annotated/contact PNGs; does not import Noetome/pdfplumber.
All inputs are the repository's public fixtures. Output must be a new directory.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


def run(command: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, timeout=30)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--pdfinfo", required=True)
    parser.add_argument("--pdftoppm", required=True)
    parser.add_argument("--pdftotext")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    fixtures = root / "fixtures/pdf-text-v1"
    expected = json.loads((fixtures / "expected.json").read_text(encoding="utf8"))
    args.output.mkdir(exist_ok=False)
    records, pictures = [], []
    for case in expected["cases"]:
        selected = fixtures / case["file"]
        data = selected.read_bytes()
        assert hashlib.sha256(data).hexdigest() == case["raw_sha256"]
        assert len(data) == case["byte_length"]
        password = ["-upw", "fixture-only"] if case["category"] == "encrypted" else []
        info = run([args.pdfinfo, *password, "-box", str(selected)])
        info_text = info.stdout.decode("utf8", errors="replace")
        count_match = re.search(r"^Pages:\s*(\d+)", info_text, re.MULTILINE)
        count = int(count_match.group(1)) if count_match else None
        if case["category"] != "damaged":
            assert count == case["authoring_page_count"], (case["file"], info_text, info.stderr)
        else:
            assert info.returncode != 0 and count is None
        text = run([args.pdftotext, *password, "-layout", str(selected), "-"]) if args.pdftotext else None
        extracted = text.stdout.decode("utf8", errors="replace") if text else None
        expected_lines = [line["text"] for page in case["pages"] for line in page["lines"]]
        prior = -1
        if extracted is not None:
            for line in expected_lines:
                found = extracted.find(line, prior+1)
                assert found > prior, (case["file"], line, extracted)
                prior = found
        record = {"file": case["file"], "raw_sha256": case["raw_sha256"], "pdfinfo_exit": info.returncode,
                  "actual_pages": count, "pdfinfo": info_text,
                  "diagnostics": info.stderr.decode("utf8", errors="replace"),
                  "pdftotext_exit": text.returncode if text else None,
                  "authoring_body_order_checked": bool(expected_lines) and text is not None,
                  "text_tool_status": "run" if text else "not_run_unavailable",
                  "pdftotext": extracted, "renders": []}
        for page_index in range(count or 0):
            prefix = args.output / f"{selected.stem}-p{page_index+1:03}"
            rendered = run([args.pdftoppm, *password, "-cropbox", "-r", "96", "-f", str(page_index+1), "-l", str(page_index+1), "-singlefile", "-png", str(selected), str(prefix)])
            png = prefix.with_suffix(".png")
            record["renders"].append({"physical_page_index": page_index, "exit": rendered.returncode,
                                       "path": png.name if png.exists() else None,
                                       "diagnostics": rendered.stderr.decode("utf8", errors="replace")})
            assert rendered.returncode == 0 and png.exists(), (selected.name, page_index, rendered.stderr)
            picture = Image.open(png).convert("RGB")
            if page_index < len(case["pages"]):
                want_page = case["pages"][page_index]
                crop = want_page["crop_box"]
                if isinstance(crop[2], (int, float)):
                    geometry = want_page["geometry"]
                    x_scale = picture.width / geometry["width_pt"]
                    y_scale = picture.height / geometry["height_pt"]
                    draw = ImageDraw.Draw(picture)
                    regions = want_page["lines"] if want_page["lines"] else want_page["glyph_positions"]
                    for line in regions:
                        l, t, r, b = line["bbox"]
                        draw.rectangle((l*x_scale, t*y_scale, r*x_scale, b*y_scale), outline="#c12832", width=2)
            annotated = args.output / f"{selected.stem}-p{page_index+1:03}-authoring.png"
            picture.save(annotated)
            pictures.append((f"{selected.name} / physical {page_index+1}", picture))
        records.append(record)
    font = ImageFont.load_default(size=18)
    columns, tile_w, tile_h = 4, 400, 570
    rows = (len(pictures)+columns-1)//columns
    sheet = Image.new("RGB", (columns*tile_w, rows*tile_h), "#dedede")
    draw = ImageDraw.Draw(sheet)
    for index, (label, picture) in enumerate(pictures):
        column, row = index % columns, index // columns
        picture.thumbnail((tile_w-16, tile_h-48))
        sheet.paste(picture, (column*tile_w+8, row*tile_h+38))
        draw.text((column*tile_w+8, row*tile_h+8), label, font=font, fill="black")
    sheet.save(args.output / "authoring-contact.png")
    assert len(pictures) == sum(record["actual_pages"] or 0 for record in records)
    report = {"schema_version": 1, "reviewer": "independent Poppler tool + agent visual review, not human scientific gold standard", "fixture_count": len(records), "render_count": len(pictures), "records": records}
    (args.output / "authoring-review.json").write_text(json.dumps(report, ensure_ascii=False, indent=2)+"\n", encoding="utf8", newline="\n")
    print(f"Authoring SHA/page checks: {len(records)} fixtures; {len(pictures)} actual page PNGs. Text tool: {'run' if args.pdftotext else 'not_run_unavailable'}; contact and individual authoring regions saved.")


if __name__ == "__main__":
    main()
