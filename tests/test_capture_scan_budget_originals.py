"""Byte capture tests use artificial receipts; no process or API is invoked."""
import hashlib
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import capture_scan_budget_originals as C
import verify_scan_formula as V
from test_verify_scan_formula import synthetic_budget_originals, synthetic_vault


class ObservedFile:
    """Instrument real file I/O to schedule an actual append deterministically."""

    def __init__(self, handle, *, after_read=None, after_write=None):
        self.handle = handle
        self.after_read = after_read
        self.after_write = after_write

    def __getattr__(self, name):
        return getattr(self.handle, name)

    def __enter__(self):
        self.handle.__enter__()
        return self

    def __exit__(self, *args):
        return self.handle.__exit__(*args)

    def read(self, size=-1):
        data = self.handle.read(size)
        if self.after_read is not None:
            self.after_read(size, data)
        return data

    def write(self, data):
        count = self.handle.write(data)
        if self.after_write is not None:
            self.after_write(data)
        return count


class ScanBudgetCaptureTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def sample(self):
        vault = self.base / "artificial-vault"
        args = synthetic_vault(vault)
        report, raw, _ = synthetic_budget_originals(vault, args)
        binding = V.strict_json(raw)
        directory = self.base / "synthetic-budget-only"
        prefix = str(directory / "tangmao-deepseek.usage.json") + ".scan-" + binding["invocation_id"]
        originals = {}
        for phase in C.PHASES:
            receipt = V.strict_json(V.read(binding["receipts"][phase]["file"]))
            receipt["usage_ledger_reference"] = str(directory / "tangmao-deepseek.usage.json")
            ledger = V.read(receipt["ledger_snapshot"]["path"])
            ledger_path = Path(prefix + "." + phase + "-ledger.json")
            ledger_path.write_bytes(ledger)
            receipt["ledger_snapshot"] = dict(path=str(ledger_path), **V.fingerprint(ledger))
            if phase == "settled":
                for stream in ("stdout", "stderr"):
                    entry = receipt["result"][stream]
                    content = V.read(entry["saved_copy"]["path"])
                    stream_path = Path(prefix + "." + stream + ".txt")
                    stream_path.write_bytes(content)
                    entry["saved_copy"] = dict(path=str(stream_path), **V.fingerprint(content))
            receipt_path = Path(prefix + "." + phase + ".json")
            receipt_path.write_bytes(C.encoded(receipt))
            originals[phase] = receipt_path
        binary = Path(V.strict_json(originals["before"].read_bytes())["selected_binary"]["path"])
        binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
        return report, originals, binary_sha

    def test_captures_actual_bytes_and_passes_independent_four_phase_budget_check(self):
        report, sources, binary_sha = self.sample()
        before = {phase: path.read_bytes() for phase, path in sources.items()}
        summary = C.capture(sources["settled"], self.base / "capture", expected_binary_sha256=binary_sha)
        binding_raw = Path(summary["budget_binding"]).read_bytes()
        checked = V.verify_budget_binding(report, binding_raw, summary["budget_binding_sha256"])
        self.assertEqual(checked["actual_total_tokens"], 130)
        self.assertEqual(summary["copied_originals"], 10)
        self.assertEqual(summary["network_calls_from_capture"], 0)
        self.assertEqual(before, {phase: path.read_bytes() for phase, path in sources.items()})
        binding = V.strict_json(binding_raw)
        for ref in binding["copies"].values():
            self.assertEqual(V.file_pin(ref["file"]), {key: ref[key] for key in ("sha256", "byte_length")})
        invocation = V.strict_json(Path(summary["invocation_receipt"]).read_bytes())
        receipt = V.strict_json(sources["settled"].read_bytes())
        self.assertEqual(invocation["command"], [receipt["selected_binary"]["path"], "knowledge", "scan-formula", receipt["case_directory"], receipt["selected_config"]["path"], receipt["profile"], receipt["output_root"]])
        self.assertEqual(invocation["exit_code"], receipt["result"]["cli_exit_code"])

    def test_changed_binary_rejected_before_archive_is_created(self):
        _, sources, binary_sha = self.sample()
        binary = Path(V.strict_json(sources["before"].read_bytes())["selected_binary"]["path"])
        binary.write_bytes(b"changed artificial bytes")
        output = self.base / "capture"
        with self.assertRaisesRegex(AssertionError, "reference fingerprint"):
            C.capture(sources["settled"], output, expected_binary_sha256=binary_sha)
        self.assertFalse(output.exists())

    def test_bad_ledger_arithmetic_rejected_before_archive_is_created(self):
        _, sources, binary_sha = self.sample()
        receipt = V.strict_json(sources["before"].read_bytes())
        ledger_file = Path(receipt["ledger_snapshot"]["path"])
        ledger = V.strict_json(ledger_file.read_bytes())
        ledger["accounted_tokens"] = 0
        ledger["remaining_tokens"] = 10000000
        ledger_file.write_bytes(C.encoded(ledger))
        receipt["ledger_snapshot"].update(V.file_pin(ledger_file))
        sources["before"].write_bytes(C.encoded(receipt))
        output = self.base / "capture"
        with self.assertRaisesRegex(AssertionError, "confirmed arithmetic"):
            C.capture(sources["settled"], output, expected_binary_sha256=binary_sha)
        self.assertFalse(output.exists())

    def test_existing_output_is_preserved(self):
        _, sources, binary_sha = self.sample()
        output = self.base / "capture"
        output.mkdir()
        sentinel = output / "human.txt"
        sentinel.write_bytes(b"human owned")
        with self.assertRaisesRegex(AssertionError, "brand-new"):
            C.capture(sources["settled"], output, expected_binary_sha256=binary_sha)
        self.assertEqual(sentinel.read_bytes(), b"human owned")
        self.assertEqual(list(output.iterdir()), [sentinel])

    def test_copy_rejects_actual_source_growth_before_writing_extra_bytes(self):
        source = self.base / "growing-original.bin"
        original = b"a" * (1024 * 1024)
        source.write_bytes(original)
        expected = V.file_pin(source)
        destination = self.base / "incomplete-copy.bin"
        real_open = Path.open
        appended = False

        def append_after_first_write(data):
            nonlocal appended
            if not appended:
                with real_open(source, "ab") as writer:
                    writer.write(b"b" * (1024 * 1024))
                appended = True

        def observed_open(path, mode="r", *args, **kwargs):
            handle = real_open(path, mode, *args, **kwargs)
            if path == destination and mode == "xb":
                return ObservedFile(handle, after_write=append_after_first_write)
            return handle

        with mock.patch.object(Path, "open", new=observed_open):
            with self.assertRaises(AssertionError):
                C.copy_unchanged(source, destination, expected)
        self.assertTrue(appended)
        self.assertGreater(source.stat().st_size, expected["byte_length"])
        self.assertEqual(destination.read_bytes(), original)

    def test_checked_read_limits_actual_growth_after_initial_pin(self):
        source = self.base / "growing-json.bin"
        original = b"small"
        source.write_bytes(original)
        real_open, real_pin = Path.open, C.V.file_pin
        appended = False
        reads = []

        def pin_then_append(path):
            nonlocal appended
            pin = real_pin(path)
            if path == source and not appended:
                with real_open(source, "ab") as writer:
                    writer.write(b"extra" * 100)
                appended = True
            return pin

        def observe_read(size, data):
            if appended:
                reads.append((size, len(data)))

        def observed_open(path, mode="r", *args, **kwargs):
            handle = real_open(path, mode, *args, **kwargs)
            if path == source and mode == "rb":
                return ObservedFile(handle, after_read=observe_read)
            return handle

        with mock.patch.object(Path, "open", new=observed_open):
            with mock.patch.object(C.V, "file_pin", side_effect=pin_then_append):
                with self.assertRaises(AssertionError):
                    C.checked_read(source, maximum=16)
        self.assertTrue(appended)
        self.assertGreater(source.stat().st_size, 16)
        self.assertTrue(reads)
        self.assertTrue(all(0 <= size <= len(original) + 1 for size, _ in reads))
        self.assertLessEqual(sum(length for _, length in reads), len(original) + 1)

    def test_file_pin_stops_actual_growth_at_original_length_plus_one(self):
        source = self.base / "growing-runtime.bin"
        original_length = 1024 * 1024
        source.write_bytes(b"a" * original_length)
        real_open = Path.open
        appended = False
        reads = []

        def append_after_first_read(size, data):
            nonlocal appended
            reads.append((size, len(data)))
            if data and not appended:
                with real_open(source, "ab") as writer:
                    writer.write(b"b" * original_length)
                appended = True

        def observed_open(path, mode="r", *args, **kwargs):
            handle = real_open(path, mode, *args, **kwargs)
            if path == source and mode == "rb":
                return ObservedFile(handle, after_read=append_after_first_read)
            return handle

        with mock.patch.object(Path, "open", new=observed_open):
            with self.assertRaises(AssertionError):
                V.file_pin(source)
        self.assertTrue(appended)
        self.assertEqual(source.stat().st_size, 2 * original_length)
        self.assertLessEqual(sum(length for _, length in reads), original_length + 1)

    def test_failed_or_foreign_phase_is_not_accepted_as_settlement(self):
        _, sources, binary_sha = self.sample()
        receipt = V.strict_json(sources["settled"].read_bytes())
        receipt["phase"] = "failed"
        sources["settled"].write_bytes(C.encoded(receipt))
        with self.assertRaisesRegex(AssertionError, "completed helper phase"):
            C.capture(sources["settled"], self.base / "capture", expected_binary_sha256=binary_sha)

    def test_explicit_windows_mapping_preserves_recorded_source_spelling(self):
        paths = C.SourcePaths([("D:/Noetome", str(self.base))])
        self.assertEqual(paths.local("d:\\Noetome\\new\\request.json"), self.base / "new" / "request.json")
        if os.name != "nt":
            with self.assertRaisesRegex(AssertionError, "requires --path-map"):
                paths.local("C:/not-selected/file.json")

    def test_longest_mapping_is_specific_and_ambiguous_mapping_is_rejected(self):
        nested = self.base / "nested"
        nested.mkdir()
        paths = C.SourcePaths([("D:/", str(self.base)), ("D:/Noetome", str(nested))])
        self.assertEqual(paths.local("D:/Noetome/request.json"), nested / "request.json")
        ambiguous = C.SourcePaths([("D:/", str(self.base)), ("D:/", str(nested))])
        with self.assertRaisesRegex(AssertionError, "ambiguous"):
            ambiguous.local("D:/request.json")

    @unittest.skipIf(os.name == "nt", "symlink setup needs separate Windows privilege")
    def test_linked_original_receipt_is_refused(self):
        _, sources, binary_sha = self.sample()
        target = sources["before"].with_suffix(".saved")
        sources["before"].rename(target)
        sources["before"].symlink_to(target)
        with self.assertRaisesRegex(AssertionError, "link/reparse"):
            C.capture(sources["settled"], self.base / "capture", expected_binary_sha256=binary_sha)


if __name__ == "__main__":
    unittest.main()
