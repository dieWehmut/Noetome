"""Run the embedded PDF adapter as an actual isolated Python subprocess.

Expected PDF text and positions come from the fixed authoring fixtures, never
from pdfplumber or adapter helpers. The generic fake sidecar tests are separate.
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zlib


ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "sidecars" / "pdf_parser" / "adapter.py"
FIXTURES = ROOT / "fixtures" / "pdf-text-v1"
EXPECTED = {
    case["file"]: case
    for case in json.loads((FIXTURES / "expected.json").read_text(encoding="utf-8"))["cases"]
}
REQUEST_ID = "12345678-1234-4234-8234-123456789abc"
CAPABILITY_ID = "abcdef01-2345-4678-9abc-def012345678"
LIMITS = {
    "request_bytes": 64 * 1024,
    "response_bytes": 4 * 1024 * 1024,
    "diagnostic_bytes": 64 * 1024,
    "input_bytes": 64 * 1024 * 1024,
    "resource_bytes": 16 * 1024 * 1024,
    "total_resource_bytes": 64 * 1024 * 1024,
    "resources": 256,
    "units": 10000,
    "issues": 1000,
    "timeout_ms": 30000,
}
PARSER_IDENTITY = {
    "engine": "pdfplumber",
    "engine_version": "0.11.9",
    "engine_dependency": "pdfminer.six",
    "engine_dependency_version": "20251230",
    "adapter_version": "noetome-pdf-text-v1",
    "options_version": "crop-pt-lines-v1",
    "python_version": "3.12.14",
}


def fingerprint(data: bytes) -> dict:
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def boundary_pdf(contents: list[bytes], *, box: bytes = b"0 0 600 800",
                 media_objects=None, extra: bytes = b"", to_unicode: bytes | None = None) -> bytes:
    """Author only temporary policy boundaries, without calling the parser."""
    objects = [b"", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
    pages = []
    for index, content in enumerate(contents):
        page_number = len(objects) + 1
        pages.append(page_number)
        media = media_objects[index] if media_objects is not None else b"[" + box + b"]"
        objects.extend([
            b"<< /Type /Page /Parent 2 0 R /MediaBox " + media
            + b" /Resources << /Font << /F1 3 0 R >> >> /Contents "
            + str(page_number + 1).encode() + b" 0 R " + extra + b" >>",
            b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n"
            + content + b"\nendstream",
        ])
    if to_unicode is not None:
        objects[2] = (b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
                      b"/Encoding /WinAnsiEncoding /ToUnicode "
                      + str(len(objects) + 1).encode() + b" 0 R >>")
        objects.append(b"<< /Length " + str(len(to_unicode)).encode() + b" >>\nstream\n"
                       + to_unicode + b"\nendstream")
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objects[1] = b"<< /Type /Pages /Count " + str(len(pages)).encode() + b" /Kids ["
    objects[1] += b" ".join(f"{index} 0 R".encode() for index in pages) + b"] >>"
    data = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for index, value in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f"{index} 0 obj\n".encode() + value + b"\nendobj\n")
    xref = len(data)
    data.extend(f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode())
    data.extend(b"".join(f"{offset:010d} 00000 n \n".encode() for offset in offsets[1:]))
    data.extend(f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
    return bytes(data)


class PdfAdapterTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="noetome-pdf-adapter-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.sentinel = self.root / "outside sentinel.txt"
        self.sentinel_bytes = b"Human bytes outside the parser capability.\r\n"
        self.sentinel.write_bytes(self.sentinel_bytes)
        self.sequence = 0

    def run_adapter(self, data: bytes, *, limits=None, edit=None, wire=None,
                    prefix="", preexisting=None):
        self.sequence += 1
        selected = self.root / f"selected {self.sequence} caf\u00e9 $().pdf"
        selected.write_bytes(data)
        workspace = self.root / f"parser-{self.sequence}"
        workspace.mkdir()
        if preexisting is not None:
            (workspace / "raw.pdf").write_bytes(preexisting)
        request = {
            "protocol_version": 1,
            "request_id": REQUEST_ID,
            "input": {"path": str(selected), "access": "read_only_single_file",
                      "fingerprint": fingerprint(data)},
            "workspace": {"path": str(workspace), "capability_id": CAPABILITY_ID,
                          "access": "exclusive_new_directory"},
            "limits": dict(LIMITS, **(limits or {})),
        }
        if edit is not None:
            edit(request)
        request_bytes = json.dumps(request, ensure_ascii=False).encode("utf-8")
        if wire is not None:
            request_bytes = wire(request_bytes)
        # Before the first implementation, still start the real process and
        # obtain an assertion failure for the explicitly missing entrypoint.
        source = ADAPTER.read_text(encoding="utf-8") if ADAPTER.is_file() else (
            "raise SystemExit('PDF adapter is not implemented')\n"
        )
        if prefix:
            declaration = "from __future__ import annotations\n"
            source = source.replace(declaration, declaration + prefix, 1)
        source_bytes = source.encode("utf-8")
        payload = zlib.compress(source_bytes, 9)
        self.assertEqual(zlib.decompress(payload), source_bytes)
        code = ("import zlib;exec(compile(zlib.decompress(bytes.fromhex('"
                + payload.hex() + "')),'<noetome-pdf-adapter-test>','exec'))")
        process = subprocess.run(
            [sys.executable, "-I", "-B", "-X", "utf8", "-c", code],
            input=request_bytes, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            cwd=self.root, timeout=45,
        )
        self.assertEqual(selected.read_bytes(), data, "selected PDF was modified")
        self.assertEqual(self.sentinel.read_bytes(), self.sentinel_bytes)
        return process, workspace, request

    def run_fixture(self, filename: str):
        data = (FIXTURES / filename).read_bytes()
        case = EXPECTED[filename]
        self.assertEqual(fingerprint(data), {
            "sha256": case["raw_sha256"], "byte_length": case["byte_length"],
        }, "fixed fixture bytes changed without authoring review")
        result, workspace, request = self.run_adapter(data)
        response, manifest, resources = self.valid_response(result, workspace, request)
        return case, response, manifest, resources

    def valid_response(self, result, workspace, request):
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertTrue(result.stdout, "adapter did not return its PDF protocol response")
        self.assertLessEqual(len(result.stdout), request["limits"]["response_bytes"])
        self.assertLessEqual(len(result.stderr), request["limits"]["diagnostic_bytes"])
        response = json.loads(result.stdout.decode("utf-8"))
        self.assertEqual(set(response), {
            "protocol_version", "request_id", "input_fingerprint", "workspace_capability_id",
            "producer", "parser", "unit_count", "units", "resources", "issues",
        })
        self.assertEqual(response["protocol_version"], 1)
        self.assertEqual(response["request_id"], REQUEST_ID)
        self.assertEqual(response["workspace_capability_id"], CAPABILITY_ID)
        self.assertEqual(response["input_fingerprint"], request["input"]["fingerprint"])
        self.assertEqual(response["producer"], "parser")
        self.assertEqual(response["parser"], {
            "name": "pdfplumber", "version": "0.11.9", "capability_version": 1,
            "capabilities": ["source_units_v1"],
        })
        self.assertEqual(response["unit_count"], len(response["units"]))
        self.assertGreater(response["unit_count"], 0)
        self.assertEqual([unit["ordinal"] for unit in response["units"]],
                         list(range(response["unit_count"])))
        resources = {}
        for resource in response["resources"]:
            self.assertEqual(set(resource), {"resource_id", "relative_path", "kind", "fingerprint"})
            self.assertNotIn(resource["resource_id"], resources)
            data = (workspace / resource["relative_path"]).read_bytes()
            self.assertEqual(fingerprint(data), resource["fingerprint"])
            if resource["kind"] == "utf8_text":
                data.decode("utf-8")
            resources[resource["resource_id"]] = (resource, data)
        self.assertEqual({path.relative_to(workspace).as_posix()
                          for path in workspace.rglob("*") if path.is_file()},
                         {value[0]["relative_path"] for value in resources.values()})
        declared_directories = {str(Path(value[0]["relative_path"]).parent).replace("\\", "/")
                                for value in resources.values()}
        declared_directories.discard(".")
        self.assertEqual({path.relative_to(workspace).as_posix()
                          for path in workspace.rglob("*") if path.is_dir()}, declared_directories)
        self.assertEqual(resources["raw-pdf"][0]["relative_path"], "raw.pdf")
        self.assertEqual(resources["raw-pdf"][0]["kind"], "binary")
        self.assertEqual(resources["raw-pdf"][0]["fingerprint"], request["input"]["fingerprint"])
        self.assertEqual(resources["pdf-manifest"][0]["relative_path"], "pdf-manifest.json")
        manifest = json.loads(resources["pdf-manifest"][1].decode("utf-8"))
        self.assertEqual(set(manifest), {
            "schema_version", "format", "input_fingerprint", "parser", "raw_resource_id",
            "page_count", "status", "pages", "resources", "issues",
        })
        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["format"], "pdf-page-regions-v1")
        self.assertEqual(manifest["input_fingerprint"], request["input"]["fingerprint"])
        self.assertEqual(manifest["parser"], PARSER_IDENTITY)
        self.assertEqual(manifest["raw_resource_id"], "raw-pdf")
        self.assertEqual(manifest["resources"], [resource for resource in response["resources"]
                                                if resource["resource_id"] != "pdf-manifest"])
        self.assertEqual(manifest["issues"], response["issues"])
        for issue in response["issues"]:
            self.assertEqual(set(issue), {"code", "severity", "message", "unit_ids", "resource_ids"})
            self.assertTrue(issue["message"])
            self.assertTrue(set(issue["unit_ids"]).issubset(
                {unit["unit_id"] for unit in response["units"]}))
            self.assertTrue(set(issue["resource_ids"]).issubset(resources))
        for page in manifest["pages"]:
            self.assertEqual(set(page), {
                "physical_page_index", "display_page_number", "printed_page_label", "status",
                "geometry", "text", "glyphs", "regions", "visuals",
            })
            page_index = page["physical_page_index"]
            unit = response["units"][page_index]
            self.assertEqual(set(unit), {"unit_id", "ordinal", "kind", "status", "text", "resource_ids"})
            self.assertEqual(unit["unit_id"], f"page-{page_index:06d}")
            self.assertEqual(unit["status"], page["status"])
            self.assertIn("raw-pdf", unit["resource_ids"])
            self.assertIn("pdf-manifest", unit["resource_ids"])
            self.assertEqual(page["display_page_number"], page_index + 1)
            self.assertIsNone(page["printed_page_label"])
            if page["text"] is not None:
                self.assertEqual(set(page["text"]), {"resource_id", "sha256", "byte_length"})
                ident = f"page-text-{page_index:06d}"
                self.assertEqual(page["text"]["resource_id"], ident)
                resource, text_bytes = resources[ident]
                self.assertEqual(resource["relative_path"], f"pages/{page_index:06d}.txt")
                self.assertEqual(page["text"]["sha256"], resource["fingerprint"]["sha256"])
                self.assertEqual(page["text"]["byte_length"], len(text_bytes))
                if text_bytes:
                    self.assertEqual(unit["kind"], "text")
                    self.assertEqual(unit["text"], {"resource_id": ident, "start_byte": 0,
                                                    "end_byte": len(text_bytes)})
                else:
                    self.assertEqual(unit["kind"], "unknown")
                    self.assertIsNone(unit["text"])
                self.assertIn(ident, unit["resource_ids"])
                assigned = []
                for region_index, region in enumerate(page["regions"]):
                    self.assertEqual(set(region), {"region_id", "locator_kind", "physical_page_index",
                                                  "bbox", "text", "glyph_indices"})
                    self.assertEqual(region["region_id"], f"region-{page_index:06d}-{region_index:06d}")
                    self.assertEqual(region["locator_kind"], "pdf_page_region")
                    self.assertEqual(region["physical_page_index"], page_index)
                    reference = region["text"]
                    self.assertEqual(set(reference), {"resource_id", "resource_sha256", "start_byte", "end_byte"})
                    self.assertEqual(reference["resource_id"], ident)
                    self.assertEqual(reference["resource_sha256"], page["text"]["sha256"])
                    fragment = text_bytes[reference["start_byte"]:reference["end_byte"]]
                    self.assertEqual(text_bytes[reference["end_byte"]:reference["end_byte"] + 1], b"\n")
                    fragment.decode("utf-8")
                    assigned.extend(region["glyph_indices"])
                self.assertEqual(sorted(assigned), list(range(len(page["glyphs"]))))
                self.assertEqual([glyph["ordinal"] for glyph in page["glyphs"]],
                                 list(range(len(page["glyphs"]))))
                for glyph in page["glyphs"]:
                    self.assertEqual(set(glyph), {"ordinal", "text", "bbox", "font_name",
                                                 "font_size", "upright"})
                    self.assertTrue(all(math.isfinite(value) for value in glyph["bbox"]))
                for visual in page["visuals"]:
                    self.assertEqual(set(visual), {"visual_id", "kind", "physical_page_index",
                                                  "bbox", "evidence_resource_id"})
                    self.assertEqual(visual["physical_page_index"], page_index)
                    self.assertEqual(visual["evidence_resource_id"], "raw-pdf")
            else:
                self.assertIsNone(unit["text"])
                self.assertEqual(page["glyphs"], [])
                self.assertEqual(page["regions"], [])
                self.assertEqual(page["visuals"], [])
            if page["status"] != "parsed":
                self.assertTrue(any(unit["unit_id"] in issue["unit_ids"]
                                    and issue["severity"] == "blocking"
                                    for issue in response["issues"]))
        for unit in response["units"]:
            if unit["text"] is not None:
                self.assertLess(unit["text"]["start_byte"], unit["text"]["end_byte"],
                                "protocol v1 forbids empty generic text ranges")
        return response, manifest, resources

    def assert_bbox(self, actual, expected, tolerance=0.08):
        self.assertEqual(len(actual), 4)
        for value, want in zip(actual, expected, strict=True):
            self.assertAlmostEqual(value, want, delta=tolerance)

    def assert_authored(self, case, manifest, resources):
        self.assertEqual(manifest["page_count"], case["parse_page_count"])
        self.assertEqual(manifest["status"], case["status"])
        self.assertEqual(len(manifest["pages"]), len(case["pages"]))
        for page, want in zip(manifest["pages"], case["pages"], strict=True):
            self.assertEqual(page["physical_page_index"], want["physical_page_index"])
            self.assertEqual(page["status"], want["status"])
            if want["status"] == "failed":
                continue
            geometry = page["geometry"]
            self.assertEqual(set(geometry), {"unit", "origin", "y_axis", "media_box", "crop_box",
                                             "rotation_degrees", "user_unit", "width_pt", "height_pt",
                                             "pdf_to_display"})
            self.assertEqual(geometry["unit"], "pt")
            self.assertEqual(geometry["origin"], "crop_top_left")
            self.assertEqual(geometry["y_axis"], "down")
            self.assertEqual(geometry["media_box"], want["media_box"])
            self.assertEqual(geometry["crop_box"], want["crop_box"])
            self.assertEqual(geometry["rotation_degrees"], want["rotation_degrees"])
            self.assertEqual(geometry["user_unit"], want.get("geometry", {}).get("user_unit", 1))
            if "geometry" in want:
                for field, expected_value in want["geometry"].items():
                    self.assertEqual(geometry[field], expected_value)
            if "glyph_positions" in want:
                self.assertEqual(len(page["glyphs"]), len(want["glyph_positions"]))
            for position in want.get("glyph_positions", []):
                glyph = page["glyphs"][position["ordinal"]]
                self.assertEqual(glyph["text"], position["text"])
                self.assert_bbox(glyph["bbox"], position["bbox"], position.get("tolerance_pt", 0.08))
            if want["status"] == "parsed":
                ident = page["text"]["resource_id"]
                lines = want["lines"]
                self.assertEqual(resources[ident][1].decode("utf-8"),
                                 "".join(line["text"] + "\n" for line in lines))
                self.assertEqual(len(page["regions"]), len(lines))
                for region, line in zip(page["regions"], lines, strict=True):
                    self.assert_bbox(region["bbox"], line["bbox"], line["tolerance_pt"])
                    reference = region["text"]
                    self.assertEqual(resources[ident][1][reference["start_byte"]:reference["end_byte"]],
                                     line["text"].encode("utf-8"))

    def assert_rejected(self, result, request):
        self.assertNotEqual(result.returncode, 0, "invalid request or overflow was accepted")
        self.assertEqual(result.stdout, b"", "failed adapter delivered a protocol success")
        self.assertTrue(result.stderr, "failure has no bounded diagnostic")
        limit = request.get("limits", {}).get("diagnostic_bytes", LIMITS["diagnostic_bytes"])
        if isinstance(limit, int) and limit > 0:
            self.assertLessEqual(len(result.stderr), limit)

    def test_real_single_page_matches_full_authoring_text_and_utf8_ranges(self):
        case, response, manifest, resources = self.run_fixture("single-page.pdf")
        self.assert_authored(case, manifest, resources)
        self.assertEqual(response["issues"], [])
        page = manifest["pages"][0]
        self.assertEqual(len(page["glyphs"]), sum(len(line["text"]) for line in case["pages"][0]["lines"]))
        self.assertIn(b"Caf\xc3\xa9", resources["page-text-000000"][1])

    def test_region_slice_is_exact_authoring_line_excluding_appended_lf(self):
        case, _, manifest, resources = self.run_fixture("single-page.pdf")
        page = manifest["pages"][0]
        text_bytes = resources[page["text"]["resource_id"]][1]
        cursor = 0
        for region, line in zip(page["regions"], case["pages"][0]["lines"], strict=True):
            reference = region["text"]
            self.assertEqual(reference["start_byte"], cursor)
            self.assertEqual(text_bytes[reference["start_byte"]:reference["end_byte"]],
                             line["text"].encode("utf-8"))
            self.assertEqual(text_bytes[reference["end_byte"]:reference["end_byte"] + 1], b"\n")
            cursor = reference["end_byte"] + 1
        self.assertEqual(cursor, len(text_bytes))

    def test_real_multiple_pages_preserves_every_physical_page(self):
        case, _, manifest, resources = self.run_fixture("multiple-pages.pdf")
        self.assert_authored(case, manifest, resources)
        self.assertEqual([page["physical_page_index"] for page in manifest["pages"]], [0, 1, 2])

    def test_real_cropbox_offset_matches_visible_origin(self):
        case, _, manifest, resources = self.run_fixture("crop-offset.pdf")
        self.assert_authored(case, manifest, resources)
        geometry = manifest["pages"][0]["geometry"]
        self.assertEqual(geometry["pdf_to_display"], [1, 0, 0, -1, -60, 790])
        self.assertEqual((geometry["width_pt"], geometry["height_pt"]), (520, 700))

    def test_visual_reading_order_preserves_actual_drawing_inventory(self):
        case, _, manifest, resources = self.run_fixture("layout-order.pdf")
        self.assert_authored(case, manifest, resources)
        page = manifest["pages"][0]
        self.assertEqual("".join(glyph["text"] for glyph in page["glyphs"]),
                         "Second visible lineFirst visible line")
        self.assertGreater(page["regions"][0]["glyph_indices"][0],
                           page["regions"][1]["glyph_indices"][0])

    def test_staggered_columns_retain_all_text_and_require_review(self):
        content = (
            b"BT /F1 12 Tf 1 0 0 1 72 728 Tm (Left first paragraph.) Tj ET\n"
            b"BT /F1 12 Tf 1 0 0 1 72 704 Tm (Left continuation.) Tj ET\n"
            b"BT /F1 12 Tf 1 0 0 1 330 716 Tm (Right first paragraph.) Tj ET\n"
            b"BT /F1 12 Tf 1 0 0 1 330 692 Tm (Right continuation.) Tj ET\n"
        )
        result, workspace, request = self.run_adapter(boundary_pdf([content]))
        _, manifest, resources = self.valid_response(result, workspace, request)
        page = manifest["pages"][0]
        self.assertEqual(manifest["page_count"], 1)
        self.assertEqual("".join(glyph["text"] for glyph in page["glyphs"]),
                         "Left first paragraph.Left continuation."
                         "Right first paragraph.Right continuation.")
        self.assertEqual(resources["page-text-000000"][1],
                         b"Left first paragraph.\nRight first paragraph.\n"
                         b"Left continuation.\nRight continuation.\n")
        self.assertEqual(len(page["regions"]), 4)
        self.assertEqual(page["visuals"], [])
        self.assertEqual(page["status"], "needs_review")
        self.assertEqual(manifest["status"], "needs_review")
        self.assertIn("PDF_POSSIBLE_COLUMNS", {issue["code"] for issue in manifest["issues"]})

    def test_partial_page_failure_retains_confirmed_page_and_true_total(self):
        case, _, manifest, resources = self.run_fixture("partial-page-failure.pdf")
        self.assert_authored(case, manifest, resources)
        self.assertEqual(manifest["page_count"], 2)
        failed = manifest["pages"][1]
        self.assertIsNone(failed["geometry"])
        self.assertIsNone(failed["text"])
        self.assertNotIn("page-text-000001", resources)
        self.assertIn(b"Confirmed first page survives", resources["page-text-000000"][1])

    def test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit(self):
        text = b"BT /F1 12 Tf 1 0 0 1 72 728 Tm (Confirmed prefix.) Tj ET"
        data = boundary_pdf([text, b"\n"], media_objects=[b"[0 0 600 800]", b"42"])
        result, workspace, request = self.run_adapter(data)
        response, manifest, resources = self.valid_response(result, workspace, request)
        self.assertIsNone(manifest["page_count"])
        self.assertEqual(manifest["status"], "failed")
        self.assertEqual([page["physical_page_index"] for page in manifest["pages"]], [0])
        self.assertEqual(manifest["pages"][0]["status"], "parsed")
        self.assertEqual(resources["page-text-000000"][1], b"Confirmed prefix.\n")
        self.assertEqual([unit["unit_id"] for unit in response["units"]], ["page-000000", "pdf-document"])
        self.assertEqual(response["units"][1]["ordinal"], 1)

    def test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping(self):
        content = b"BT /F1 12 Tf 1 0 0 1 590 728 Tm (Outside text) Tj ET"
        result, workspace, request = self.run_adapter(boundary_pdf([content]))
        _, manifest, resources = self.valid_response(result, workspace, request)
        self.assertEqual(manifest["page_count"], 1)
        self.assertEqual(manifest["status"], "failed")
        self.assertEqual(manifest["pages"][0]["status"], "failed")
        self.assertIsNone(manifest["pages"][0]["text"])
        self.assertNotIn("page-text-000000", resources)
        self.assertTrue(any(issue["code"] == "PDF_PAGE_BBOX" for issue in manifest["issues"]))

    def test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review(self):
        # PDF real syntax uses decimal notation; both values are finite and
        # positive, while their multiplication underflows to a zero pt width.
        tiny = b"0." + b"0" * 199 + b"1"
        box = b"0 0 " + tiny + b" " + tiny
        result, workspace, request = self.run_adapter(
            boundary_pdf([b"\n"], box=box, extra=b"/UserUnit " + tiny))
        _, manifest, _ = self.valid_response(result, workspace, request)
        self.assertEqual(manifest["page_count"], 1)
        self.assertEqual(manifest["status"], "failed")
        self.assertIsNone(manifest["pages"][0]["geometry"])

    def test_overlapping_glyphs_are_preserved_and_require_review(self):
        content = (b"BT /F1 12 Tf 1 0 0 1 72 728 Tm (Alpha) Tj ET\n"
                   b"BT /F1 12 Tf 1 0 0 1 72 728 Tm (Bravo) Tj ET\n")
        result, workspace, request = self.run_adapter(boundary_pdf([content]))
        _, manifest, _ = self.valid_response(result, workspace, request)
        self.assertEqual(manifest["status"], "needs_review")
        self.assertEqual("".join(glyph["text"] for glyph in manifest["pages"][0]["glyphs"]), "AlphaBravo")
        self.assertTrue(any(issue["code"] == "PDF_GLYPH_OVERLAP" for issue in manifest["issues"]))

    def test_annotation_uses_raw_rotation_transform_without_following_action(self):
        content = b"BT /F1 12 Tf 1 0 0 1 72 728 Tm (Annotation evidence) Tj ET"
        extra = (b"/Rotate 90 /CropBox [20 30 580 790] /Annots ["
                 b"<< /Type /Annot /Subtype /Link /Rect [100 700 200 740] "
                 b"/A << /S /URI /URI (file:///unselected-private-path) >> >>]")
        result, workspace, request = self.run_adapter(boundary_pdf([content], extra=extra))
        _, manifest, _ = self.valid_response(result, workspace, request)
        self.assertEqual(manifest["status"], "needs_review")
        annotations = [visual for visual in manifest["pages"][0]["visuals"] if visual["kind"] == "annotation"]
        self.assertEqual(len(annotations), 1)
        self.assert_bbox(annotations[0]["bbox"], [670, 80, 710, 180], 0.000001)

    def test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues(self):
        for filename, has_visual in [("scanned-page.pdf", True), ("blank-page.pdf", False)]:
            with self.subTest(filename=filename):
                case, response, manifest, resources = self.run_fixture(filename)
                self.assert_authored(case, manifest, resources)
                self.assertEqual(resources["page-text-000000"][1], b"")
                self.assertIsNotNone(manifest["pages"][0]["text"])
                self.assertEqual(manifest["pages"][0]["text"]["byte_length"], 0)
                self.assertEqual(response["units"][0]["kind"], "unknown")
                self.assertIsNone(response["units"][0]["text"])
                self.assertIn("page-text-000000", response["units"][0]["resource_ids"])
                self.assertEqual(bool(manifest["pages"][0]["visuals"]), has_visual)

    def test_whitespace_only_page_retains_nonempty_artifact_and_requires_review(self):
        content = b"BT /F1 12 Tf 1 0 0 1 72 728 Tm (   ) Tj ET"
        result, workspace, request = self.run_adapter(boundary_pdf([content]))
        response, manifest, resources = self.valid_response(result, workspace, request)
        page = manifest["pages"][0]
        self.assertEqual(manifest["page_count"], 1)
        self.assertEqual([glyph["text"] for glyph in page["glyphs"]], [" ", " ", " "])
        self.assertEqual(resources["page-text-000000"][1], b"   \n")
        self.assertEqual(page["text"]["byte_length"], 4)
        self.assertEqual(len(page["regions"]), 1)
        reference = page["regions"][0]["text"]
        self.assertEqual((reference["start_byte"], reference["end_byte"]), (0, 3))
        self.assertEqual(response["units"][0]["kind"], "text")
        self.assertEqual(response["units"][0]["text"], {
            "resource_id": "page-text-000000", "start_byte": 0, "end_byte": 4,
        })
        self.assertEqual(page["status"], "needs_review")
        self.assertEqual(manifest["status"], "needs_review")
        self.assertIn("PDF_UNKNOWN_BLANK", {issue["code"] for issue in manifest["issues"]})

    def test_tounicode_glyph_boundary_spaces_and_geometry_require_review(self):
        # These literal decoded strings author the CMap. They are not position
        # or font-width oracles for this unsupported multi-codepoint encoding.
        cases = [
            ("trailing_space", "A ", "B", 95, b"A B\n", {"PDF_MULTICODEPOINT_GLYPH"}),
            ("leading_space", "A", " B", 95, b"A B\n", {"PDF_MULTICODEPOINT_GLYPH"}),
            ("both_boundary_spaces", "A ", " B", 95, b"A  B\n", {"PDF_MULTICODEPOINT_GLYPH"}),
            ("large_gap_after_encoded_space", "A ", "B", 130, b"A B\n",
             {"PDF_MULTICODEPOINT_GLYPH", "PDF_LAYOUT_GAP"}),
            ("large_gap_before_whitespace_glyph", "A", " ", 130, b"A \n", {"PDF_LAYOUT_GAP"}),
            ("overlap_before_whitespace_glyph", "A", " ", 73, b"A \n", {"PDF_GLYPH_OVERLAP"}),
        ]
        for name, first_text, second_text, second_x, expected, required_issues in cases:
            with self.subTest(case=name):
                cmap = (
                    b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n"
                    b"/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n"
                    b"/CMapName /ReviewMap def\n/CMapType 2 def\n"
                    b"1 begincodespacerange\n<00> <FF>\nendcodespacerange\n2 beginbfchar\n<41> <"
                    + first_text.encode("utf-16-be").hex().upper().encode("ascii") + b">\n<42> <"
                    + second_text.encode("utf-16-be").hex().upper().encode("ascii")
                    + b">\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
                )
                content = (f"BT /F1 12 Tf 1 0 0 1 72 728 Tm (A) Tj "
                           f"1 0 0 1 {second_x} 728 Tm (B) Tj ET").encode("ascii")
                result, workspace, request = self.run_adapter(boundary_pdf([content], to_unicode=cmap))
                _, manifest, resources = self.valid_response(result, workspace, request)
                page = manifest["pages"][0]
                self.assertEqual(manifest["page_count"], 1)
                self.assertEqual([glyph["text"] for glyph in page["glyphs"]], [first_text, second_text])
                text_bytes = resources["page-text-000000"][1]
                self.assertEqual(text_bytes, expected)
                self.assertEqual(len(page["regions"]), 1)
                reference = page["regions"][0]["text"]
                self.assertEqual(text_bytes[reference["start_byte"]:reference["end_byte"]], expected[:-1])
                self.assertEqual(page["status"], "needs_review")
                self.assertEqual(manifest["status"], "needs_review")
                self.assertTrue(required_issues <= {issue["code"] for issue in manifest["issues"]})

    def test_table_formula_image_and_columns_remain_honestly_needs_review(self):
        for filename in ["table-page.pdf", "formula-page.pdf", "image-page.pdf", "columns-page.pdf"]:
            with self.subTest(filename=filename):
                case, _, manifest, resources = self.run_fixture(filename)
                self.assert_authored(case, manifest, resources)
                self.assertTrue(manifest["issues"])
                self.assertTrue(manifest["pages"][0]["glyphs"])
                if filename in {"table-page.pdf", "image-page.pdf"}:
                    self.assertTrue(manifest["pages"][0]["visuals"])

    def test_rotations_and_userunit_keep_true_transforms_and_review_status(self):
        case, _, manifest, resources = self.run_fixture("rotations.pdf")
        self.assert_authored(case, manifest, resources)
        for page, want in zip(manifest["pages"], case["pages"], strict=True):
            left, bottom, right, top = want["crop_box"]
            scale = want.get("geometry", {}).get("user_unit", 1)
            rotate = want["rotation_degrees"]
            matrices = {
                0: [scale, 0, 0, -scale, -scale * left, scale * top],
                90: [0, scale, scale, 0, -scale * bottom, -scale * left],
                180: [-scale, 0, 0, scale, scale * right, -scale * bottom],
                270: [0, -scale, -scale, 0, scale * top, scale * right],
            }
            geometry = page["geometry"]
            self.assertEqual(geometry["pdf_to_display"], matrices[rotate])
            dimensions = ((right - left) * scale, (top - bottom) * scale)
            if rotate in {90, 270}:
                dimensions = dimensions[::-1]
            self.assertEqual((geometry["width_pt"], geometry["height_pt"]), dimensions)
            self.assertTrue(page["glyphs"])

    def test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit(self):
        for filename in ["damaged.pdf", "encrypted.pdf"]:
            with self.subTest(filename=filename):
                case, response, manifest, resources = self.run_fixture(filename)
                self.assert_authored(case, manifest, resources)
                self.assertEqual(response["unit_count"], 1)
                self.assertEqual(response["units"][0], {
                    "unit_id": "pdf-document", "ordinal": 0, "kind": "unknown", "status": "failed",
                    "text": None, "resource_ids": ["raw-pdf", "pdf-manifest"],
                })
                self.assertTrue(any("pdf-document" in issue["unit_ids"] for issue in manifest["issues"]))

    def test_unknown_format_preserves_complete_raw_without_inventing_pages(self):
        data = b"This selected file is not a PDF.\n"
        result, workspace, request = self.run_adapter(data)
        response, manifest, resources = self.valid_response(result, workspace, request)
        self.assertIsNone(manifest["page_count"])
        self.assertEqual(manifest["status"], "failed")
        self.assertEqual(manifest["pages"], [])
        self.assertEqual(response["units"][0]["unit_id"], "pdf-document")
        self.assertEqual(resources["raw-pdf"][1], data)

    def test_empty_document_gets_failed_document_unit(self):
        result, workspace, request = self.run_adapter(boundary_pdf([]))
        response, manifest, _ = self.valid_response(result, workspace, request)
        self.assertEqual(manifest["page_count"], 0)
        self.assertEqual(manifest["status"], "failed")
        self.assertEqual(manifest["pages"], [])
        self.assertEqual(response["units"][0]["unit_id"], "pdf-document")

    def test_wrong_input_fingerprint_is_rejected_before_creating_resources(self):
        for field, value in [("sha256", "0" * 64), ("byte_length", 1)]:
            with self.subTest(field=field):
                def edit(request):
                    request["input"]["fingerprint"][field] = value
                result, workspace, request = self.run_adapter((FIXTURES / "single-page.pdf").read_bytes(), edit=edit)
                self.assert_rejected(result, request)
                self.assertEqual(list(workspace.iterdir()), [])

    def test_existing_resource_collision_never_overwrites_human_bytes(self):
        human = b"Existing human raw.pdf must stay byte-identical.\n"
        result, workspace, request = self.run_adapter((FIXTURES / "single-page.pdf").read_bytes(), preexisting=human)
        self.assert_rejected(result, request)
        self.assertEqual((workspace / "raw.pdf").read_bytes(), human)
        self.assertEqual([path.name for path in workspace.iterdir()], ["raw.pdf"])

    def test_unknown_and_missing_fields_and_bad_enums_are_rejected(self):
        changes = []
        for path in [(), ("input",), ("workspace",), ("limits",), ("input", "fingerprint")]:
            changes.append((f"unknown at {path}", path, "unknown", "unexpected"))
            target = {
                (): ["protocol_version", "request_id", "input", "workspace", "limits"],
                ("input",): ["path", "access", "fingerprint"],
                ("workspace",): ["path", "access", "capability_id"],
                ("limits",): list(LIMITS),
                ("input", "fingerprint"): ["sha256", "byte_length"],
            }[path]
            changes.extend((f"missing {path}/{key}", path, key, None) for key in target)
        changes.extend([
            ("protocol", (), "protocol_version", 2),
            ("request UUID", (), "request_id", "not-a-uuid"),
            ("workspace UUID", ("workspace",), "capability_id", REQUEST_ID.upper()),
            ("input access", ("input",), "access", "read_write"),
            ("workspace access", ("workspace",), "access", "reuse_directory"),
            ("relative input", ("input",), "path", "relative.pdf"),
            ("relative workspace", ("workspace",), "path", "../outside"),
            ("uppercase SHA", ("input", "fingerprint"), "sha256", "A" * 64),
            ("bool policy", ("limits",), "resources", True),
            ("zero policy", ("limits",), "issues", 0),
        ])
        for label, path, key, value in changes:
            with self.subTest(change=label):
                def edit(request):
                    target = request
                    for component in path:
                        target = target[component]
                    if value is None:
                        del target[key]
                    else:
                        target[key] = value
                result, workspace, request = self.run_adapter((FIXTURES / "single-page.pdf").read_bytes(), edit=edit)
                self.assert_rejected(result, request)
                self.assertEqual(list(workspace.iterdir()), [])

    def test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected(self):
        wires = [
            lambda data: data.replace(b'"protocol_version": 1', b'"protocol_version": 1, "protocol_version": 1'),
            lambda data: data + b"\n{}",
            lambda data: data + b"\xff",
            lambda data: b" " * (64 * 1024 + 1) + data,
        ]
        for index, wire in enumerate(wires):
            with self.subTest(wire=index):
                result, workspace, request = self.run_adapter((FIXTURES / "single-page.pdf").read_bytes(), wire=wire)
                self.assert_rejected(result, request)
                self.assertEqual(list(workspace.iterdir()), [])

    def test_invalid_request_diagnostic_respects_small_explicit_byte_cap(self):
        result, _, request = self.run_adapter(
            (FIXTURES / "single-page.pdf").read_bytes(), limits={"diagnostic_bytes": 7},
            edit=lambda request: request["input"].update(unknown="field"),
        )
        self.assert_rejected(result, request)

    def test_actual_small_byte_count_and_runtime_limits_never_claim_success(self):
        cases = [
            {"request_bytes": 1}, {"response_bytes": 1}, {"input_bytes": 1},
            {"resource_bytes": 1}, {"total_resource_bytes": 1}, {"resources": 1},
            {"units": 1}, {"issues": 1}, {"timeout_ms": 1},
        ]
        for limit in cases:
            with self.subTest(limit=limit):
                filename = "multiple-pages.pdf" if "units" in limit else (
                    "rotations.pdf" if "issues" in limit else "single-page.pdf")
                result, _, request = self.run_adapter((FIXTURES / filename).read_bytes(), limits=limit)
                self.assert_rejected(result, request)

    def test_pdf_page_policy_overflow_does_not_truncate_page_manifest(self):
        result, _, request = self.run_adapter(boundary_pdf([b"\n"] * 129))
        self.assert_rejected(result, request)
        self.assertIn(b"128", result.stderr)

    def test_glyph_policy_overflow_does_not_publish_truncated_inventory(self):
        content = b"BT /F1 .01 Tf 1 0 0 1 72 728 Tm (" + b"A" * 50001 + b") Tj ET"
        result, _, request = self.run_adapter(boundary_pdf([content]))
        self.assert_rejected(result, request)
        self.assertIn(b"50000", result.stderr)

    def test_total_glyph_policy_overflow_never_publishes_incomplete_manifest(self):
        content = b"BT /F1 .01 Tf 1 0 0 1 72 728 Tm (" + b"A" * 40001 + b") Tj ET"
        result, _, request = self.run_adapter(boundary_pdf([content] * 5))
        self.assert_rejected(result, request)
        self.assertIn(b"200000", result.stderr)

    def test_raw_pdf_policy_overflow_fails_before_any_resource_is_written(self):
        data = b"%PDF-1.4\n" + b" " * (16 * 1024 * 1024)
        result, workspace, request = self.run_adapter(data)
        self.assert_rejected(result, request)
        self.assertEqual(list(workspace.iterdir()), [])

    def test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import(self):
        # A scoped metadata failure avoids mutating the shared Python install.
        # The real adapter executes; the assertion is on its refused output.
        for name in ["pdfplumber", "pdfminer.six", "pypdfium2", "Pillow", "charset-normalizer",
                     "cryptography", "cffi", "pycparser"]:
            with self.subTest(dependency=name):
                prefix = (
                    "import importlib.metadata as _metadata\n"
                    "_real_version = _metadata.version\n"
                    f"_metadata.version = lambda name: '0.0.0' if name == {name!r} else _real_version(name)\n"
                )
                result, workspace, request = self.run_adapter((FIXTURES / "single-page.pdf").read_bytes(), prefix=prefix)
                self.assert_rejected(result, request)
                self.assertIn(name.encode("utf-8"), result.stderr)
                self.assertEqual(list(workspace.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
