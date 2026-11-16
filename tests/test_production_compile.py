"""Harness checks plus opt-in real-CLI synthetic HTTP regression."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import urllib.request

MODULE = Path(__file__).resolve().parents[1] / "scripts/check_production_compile.py"
spec = importlib.util.spec_from_file_location("check_production_compile", MODULE)
import sys
checker = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = checker
spec.loader.exec_module(checker)


def request_body():
    source = "Authored café text"
    return {"messages": [{"role": "system", "content": "explicit test"}, {"role": "user", "content": json.dumps({"request": {"request_id": "request-test", "binding": {"source_id": "source-test"}, "unit": {"unit_id": "unit-test", "text": source, "evidence_anchor": {"excerpt_start_byte": 7, "excerpt_end_byte": 7 + len(source.encode("utf8")), "excerpt_sha256": checker.sha(source.encode("utf8"))}}, "existing_concepts": []}})}]}


class FakeHttpTests(unittest.TestCase):
    def test_loopback_server_records_exact_request_bytes_and_utf8_anchor(self):
        with tempfile.TemporaryDirectory() as temporary:
            with checker.FakeChat(Path(temporary) / "http") as server:
                body = json.dumps(request_body(), ensure_ascii=False).encode("utf8")
                request = urllib.request.Request(server.endpoint, body, {"Content-Type": "application/json", "Authorization": "Bearer " + checker.FAKE_KEY})
                with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=5) as response:
                    envelope = json.load(response)
                candidate = json.loads(envelope["choices"][0]["message"]["content"])
                self.assertEqual(candidate["request_id"], "request-test")
                self.assertEqual(candidate["evidence"][0]["excerpt_start_byte"], 7)
                self.assertEqual(candidate["evidence"][0]["excerpt_end_byte"], 26)
                self.assertEqual(candidate["evidence"][0]["excerpt_sha256"], checker.sha("Authored café text".encode("utf8")))
                self.assertEqual(candidate["evidence"][0]["quote"], "Authored café text")
                self.assertEqual(server.records[0]["body_sha256"], checker.sha(body))
                self.assertTrue(server.records[0]["auth_valid"])
                self.assertNotIn("Authorization", server.records[0])

    def test_empty_candidates_keep_binding_and_known_usage(self):
        envelope = json.loads(checker.chat_response(request_body(), empty=True))
        candidate = json.loads(envelope["choices"][0]["message"]["content"])
        self.assertEqual(candidate["binding"], {"source_id": "source-test"})
        self.assertEqual((candidate["concepts"], candidate["evidence"], candidate["atoms"]), ([], [], []))
        self.assertEqual(envelope["usage"]["total_tokens"], 150)

    def test_auth_failure_is_real_http_401_on_loopback(self):
        with tempfile.TemporaryDirectory() as temporary:
            with checker.FakeChat(Path(temporary) / "http", mode="auth", fault_at=1) as server:
                request = urllib.request.Request(server.endpoint, json.dumps(request_body()).encode(), {"Authorization": "Bearer " + checker.FAKE_KEY})
                with self.assertRaises(urllib.error.HTTPError) as error:
                    urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=5)
                self.assertEqual(error.exception.code, 401)
                self.assertEqual(len(server.records), 1)


class AuthoredPdfTests(unittest.TestCase):
    def test_independent_pdf_engine_reads_all_pages_and_accented_source(self):
        try:
            import pypdfium2
        except ImportError:
            self.skipTest("pypdfium2 is not installed in selected test interpreter")
        document = pypdfium2.PdfDocument(checker.authored_pdf(pages=4, lines=10, blank_last=True))
        try:
            self.assertEqual(len(document), 5)
            for index in range(4):
                page = document[index]
                text = page.get_textpage()
                try:
                    extracted = text.get_text_range()
                    self.assertIn(f"Page {index+1} line 10", extracted)
                    self.assertIn("Café", extracted)
                finally:
                    text.close()
                    page.close()
        finally:
            document.close()


class ProductionCompileBlackboxTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("NOETOME_TEST_COMPILE_BINARY") and os.environ.get("NOETOME_TEST_COMPILE_PDF_PYTHON"), "Select explicit CLI and pinned PDF Python to run synthetic HTTP/CLI blackbox")
    def test_real_cli_entire_synthetic_matrix(self):
        with tempfile.TemporaryDirectory() as temporary:
            harness = checker.Blackbox(Path(os.environ["NOETOME_TEST_COMPILE_BINARY"]), Path(os.environ["NOETOME_TEST_COMPILE_PDF_PYTHON"]), Path(temporary) / "evidence")
            try:
                result = harness.run()
                self.assertTrue(result["all_passed"], json.dumps(result["scenarios"], ensure_ascii=False))
            finally:
                harness.cleanup()


if __name__ == "__main__":
    unittest.main()
