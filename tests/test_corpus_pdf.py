"""Independently authored PDF operators exercise the real resumable worker."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / "sidecars/corpus_pdf/worker.py"
spec = importlib.util.spec_from_file_location("corpus_pdf_worker", WORKER)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


def author_pdf(path, pages, *, outline=False, padding=0):
    """Write explicit PDF objects/xref; expected values never come from parser.

    Each page is (content operators, extra page dictionary fields). Optional
    sparse padding gives a large file without allocating its entire contents.
    """
    objects = [b"", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
               b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n\x00\nendstream"]
    page_ids = []
    for content, extra in pages:
        page_id = len(objects) + 1
        page_ids.append(page_id)
        objects.extend([
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] "
            b"/Resources << /Font << /F1 3 0 R >> /XObject << /Im1 4 0 R >> >> "
            + f"/Contents {page_id + 1} 0 R ".encode() + extra + b" >>",
            b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"\nendstream",
        ])
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R"
    if outline:
        oid = len(objects) + 1
        objects[0] += f" /Outlines {oid} 0 R".encode()
        objects.extend([
            f"<< /Type /Outlines /First {oid+1} 0 R /Last {oid+1} 0 R /Count 1 >>".encode(),
            f"<< /Title (Authored Chapter) /Parent {oid} 0 R /Dest [{page_ids[-1]} 0 R /Fit] >>".encode(),
        ])
    objects[0] += b" >>"
    objects[1] = b"<< /Type /Pages /Count " + str(len(pages)).encode() + b" /Kids ["
    objects[1] += b" ".join(f"{n} 0 R".encode() for n in page_ids) + b"] >>"
    with path.open("wb") as output:
        output.write(b"%PDF-1.7\n")
        if padding:
            output.seek(padding)
            output.write(b"\n")
        offsets = [0]
        for n, obj in enumerate(objects, 1):
            offsets.append(output.tell())
            output.write(f"{n} 0 obj\n".encode() + obj + b"\nendobj\n")
        xref = output.tell()
        output.write(f"xref\n0 {len(objects)+1}\n0000000000 65535 f \n".encode())
        for offset in offsets[1:]:
            output.write(f"{offset:010d} 00000 n \n".encode())
        output.write(f"trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())


def text_page(text):
    return f"BT /F1 12 Tf 30 220 Td ({text}) Tj ET".encode(), b""


class CorpusPdfTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="noetome-corpus-pdf-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.source = self.root / "selected 中文 café $().pdf"
        self.output = self.root / "output"

    def invoke(self, action, *args, expected=0, source=None):
        selected = source or self.source
        before = selected.stat()
        result = subprocess.run([sys.executable, "-B", "-X", "utf8", str(WORKER), action,
                                 "--input", str(selected), "--output-dir", str(self.output), *map(str, args)],
                                capture_output=True, timeout=60)
        self.assertEqual(result.returncode, expected, result.stderr.decode() + result.stdout.decode())
        after = selected.stat()
        self.assertEqual((before.st_size, before.st_mtime_ns), (after.st_size, after.st_mtime_ns))
        return [json.loads(line) for line in result.stdout.splitlines()]

    def page(self, number):
        return json.loads((self.output / "pages" / f"{number:06d}.json").read_bytes())

    def test_readable_text_source_protection_and_outline(self):
        author_pdf(self.source, [text_page("Alpha beta"), text_page("Second chapter")], outline=True)
        digest = hashlib.sha256(self.source.read_bytes()).hexdigest()
        self.invoke("preflight")
        report = json.loads((self.output / "preflight.json").read_bytes())
        self.assertEqual(report["status"], "ready")
        self.assertEqual(report["page_count"], 2)
        self.assertEqual(report["outline"], [{"level": 1, "title": "Authored Chapter", "page_number": 2}])
        self.assertEqual(report["text_sample"][0]["status"], "text_ready")
        events = self.invoke("pages", "--start-page", 1, "--end-page", 1)
        page = self.page(1)
        self.assertEqual(page["text"], "Alpha beta")
        self.assertEqual(page["status"], "text_ready")
        self.assertEqual(page["regions"][0]["ordinal"], 0)
        self.assertEqual(page["regions"][0]["kind"], "text")
        self.assertEqual((page["width_pt"], page["height_pt"]), (200, 300))
        self.assertEqual(events[-1]["processed_pages"], 1)
        self.assertEqual(hashlib.sha256(self.source.read_bytes()).hexdigest(), digest)

    def test_unlimited_file_and_page_inventory_and_late_range(self):
        author_pdf(self.source, [text_page(f"Page {n}") for n in range(1, 161)], padding=65 * 1024 * 1024)
        self.invoke("preflight")
        report = json.loads((self.output / "preflight.json").read_bytes())
        self.assertEqual(report["page_count"], 160)
        self.assertGreater(report["byte_length"], 64 * 1024 * 1024)
        self.assertIsNone(report["limits"]["max_file_bytes"])
        self.assertIsNone(report["limits"]["max_pages"])
        self.assertEqual(len(report["text_sample"]), 3)
        self.invoke("pages", "--start-page", 160)
        self.assertEqual(self.page(160)["text"], "Page 160")
        self.assertEqual(len(list((self.output / "pages").iterdir())), 1)

    def test_default_eight_page_batch_and_resumable_stable_outputs(self):
        author_pdf(self.source, [text_page(f"Page {n}") for n in range(1, 12)])
        events = self.invoke("pages")
        self.assertEqual(events[-1]["processed_pages"], 8)
        first = (self.output / "pages/000001.json").read_bytes()
        self.invoke("pages", "--start-page", 9)
        self.assertEqual(len(list((self.output / "pages").iterdir())), 11)
        self.assertEqual((self.output / "pages/000001.json").read_bytes(), first)
        original = (self.output / "pages/000009.json").read_bytes()
        self.invoke("pages", "--start-page", 9, "--end-page", 9)
        self.assertEqual((self.output / "pages/000009.json").read_bytes(), original)
        self.assertEqual(self.page(11)["text"], "Page 11")

    def test_blank_scan_and_mixed_visual_statuses(self):
        image = b"q 100 0 0 100 30 100 cm /Im1 Do Q"
        author_pdf(self.source, [(b"", b""), (image, b""), (text_page("Caption")[0] + b"\n" + image, b"")])
        self.invoke("pages")
        self.assertEqual([self.page(n)["status"] for n in (1, 2, 3)], ["blank", "needs_ocr", "needs_review"])
        self.assertEqual(self.page(2)["regions"][0]["kind"], "image")
        self.assertEqual(self.page(3)["text"], "Caption")

    def test_scan_with_only_page_number_still_requires_ocr(self):
        image = b"q 200 0 0 300 0 0 cm /Im1 Do Q"
        author_pdf(self.source, [(image + b"\n" + text_page("12")[0], b"")])
        self.invoke("pages")
        self.assertEqual(self.page(1)["status"], "needs_ocr")
        self.assertEqual(self.page(1)["text"], "12")
        self.assertIn("PDF_SPARSE_TEXT_LAYER", {i["code"] for i in self.page(1)["issues"]})

    def test_one_bad_page_does_not_abort_following_page(self):
        author_pdf(self.source, [text_page("Before"), (b"", b"/UserUnit 0"), text_page("After")])
        self.invoke("preflight")
        self.assertEqual(json.loads((self.output / "preflight.json").read_bytes())["page_count"], 3)
        events = self.invoke("pages")
        self.assertEqual([self.page(n)["status"] for n in (1, 2, 3)], ["text_ready", "failed", "text_ready"])
        self.assertEqual(self.page(2)["issues"][0]["code"], "PDF_PAGE_GEOMETRY")
        self.assertEqual(self.page(3)["text"], "After")
        self.assertEqual(events[-1]["counts"]["failed"], 1)

    def test_crop_rotation_and_userunit_have_authored_vector_coordinates(self):
        author_pdf(self.source, [(b"40 60 40 40 re S", f"/CropBox [20 30 180 270] /UserUnit 2 /Rotate {r}".encode())
                                 for r in (0, 90, 180, 270)])
        self.invoke("pages")
        # The PDF rectangle is [40,60,80,100] in bottom-left source coordinates.
        expected = [(40, 340, 120, 420), (60, 40, 140, 120),
                    (200, 60, 280, 140), (340, 200, 420, 280)]
        for number, bounds in enumerate(expected, 1):
            page = self.page(number)
            self.assertEqual(page["regions"][0]["bbox"], list(bounds))
            self.assertEqual((page["width_pt"], page["height_pt"]), (320, 480) if number % 2 else (480, 320))
            self.assertEqual(page["geometry"]["crop_box"], [20, 30, 180, 270])
            self.assertEqual(page["geometry"]["user_unit"], 2)
            self.assertEqual(page["status"], "needs_review")

    def test_columns_keep_segments_and_explicit_review(self):
        content = b"BT /F1 10 Tf 10 220 Td (Left) Tj 130 0 Td (Right) Tj ET"
        author_pdf(self.source, [(content, b"")])
        self.invoke("pages")
        page = self.page(1)
        self.assertEqual([r["text"] for r in page["regions"]], ["Left", "Right"])
        self.assertEqual(page["status"], "needs_review")
        self.assertIn("PDF_COMPLEX_LAYOUT", {i["code"] for i in page["issues"]})

    def test_damaged_password_and_out_of_range_are_explicit(self):
        self.source.write_bytes(b"not a PDF")
        self.invoke("preflight", expected=2)
        self.assertEqual(json.loads((self.output / "preflight.json").read_bytes())["status"], "failed")
        self.invoke("preflight", expected=2, source=ROOT / "fixtures/pdf-text-v1/encrypted.pdf")
        report = json.loads((self.output / "preflight.json").read_bytes())
        self.assertTrue(report["password_required"])
        self.assertEqual(report["status"], "password_required")
        author_pdf(self.source, [text_page("Only")])
        events = self.invoke("pages", "--start-page", 2, expected=2)
        self.assertEqual(events[-1]["code"], "PDF_PAGE_RANGE")

    def test_atomic_limit_retains_prior_file_and_page_overflow_does_not_abort(self):
        path = self.root / "stable.json"
        path.write_bytes(b"previous\n")
        with self.assertRaisesRegex(worker.WorkerError, "16 MiB"):
            worker.atomic_json(path, {"text": "x" * 1000}, maximum=50)
        self.assertEqual(path.read_bytes(), b"previous\n")
        self.assertFalse(list(self.root.glob(".*.tmp")))
        author_pdf(self.source, [text_page("oversized"), text_page("After")])
        actual_extract = worker.extract_page
        def oversized(pdf, number, object_id, attrs):
            page = actual_extract(pdf, number, object_id, attrs)
            if number == 1:
                page["text"] = "x" * 10000
            return page
        with patch.object(worker, "MAX_PAGE_JSON_BYTES", 2048), patch.object(worker, "extract_page", side_effect=oversized), patch("sys.stdout", new=io.StringIO()):
            self.assertEqual(worker.run("pages", self.source, self.output), 0)
        self.assertEqual(self.page(1)["status"], "failed")
        self.assertEqual(self.page(1)["issues"][0]["code"], "PDF_PAGE_JSON_LIMIT")
        self.assertEqual(self.page(2)["text"], "After")

    def test_never_builds_all_pages_cache_or_reads_whole_input(self):
        import pdfplumber
        author_pdf(self.source, [text_page(f"Page {n}") for n in range(12)])
        original_open = Path.open
        read_sizes = []
        class GuardedFile:
            def __init__(self, wrapped): self.wrapped = wrapped
            def read(self, size=-1):
                self_test.assertGreaterEqual(size, 0, "whole-file read attempted")
                read_sizes.append(size)
                return self.wrapped.read(size)
            def __getattr__(self, name): return getattr(self.wrapped, name)
            def __enter__(self): return self
            def __exit__(self, *args): self.wrapped.close()
        self_test = self
        def guarded_open(path, *args, **kwargs):
            opened = original_open(path, *args, **kwargs)
            return GuardedFile(opened) if path == self.source else opened
        def no_all_pages(_):
            raise AssertionError("all-pages cache used")
        with patch.object(Path, "open", guarded_open), patch.object(pdfplumber.PDF, "pages", property(no_all_pages)), patch("sys.stdout", new=io.StringIO()):
            self.assertEqual(worker.run("pages", self.source, self.output, 2, 3), 0)
        self.assertTrue(read_sizes)
        self.assertEqual(self.page(3)["text"], "Page 2")


if __name__ == "__main__":
    unittest.main()
