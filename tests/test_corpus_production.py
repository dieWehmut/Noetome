"""Black-box production repository checks; authored PDF content is synthetic.

Set NOETOME_CORPUS_CLI to the built executable and NOETOME_PDF_PYTHON to the
locked PDF runtime. No selected/private textbook is loaded by these regressions.
The kill test uses a POSIX process group or the Windows host Job lifetime.
"""
import hashlib
from contextlib import closing
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / "sidecars/corpus_pdf/worker.py"
CLI = os.environ.get("NOETOME_CORPUS_CLI")
RUNTIME = os.environ.get("NOETOME_PDF_PYTHON")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_pdf(path, operators):
    """An explicit page tree, Helvetica font, image and xref with literal text."""
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"",
               b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
               b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n\x00\nendstream"]
    ids = []
    for operators, extra in operators:
        number = len(objects) + 1
        ids.append(number)
        objects.extend([
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /F1 3 0 R >> /XObject << /Im1 4 0 R >> >> "
            + f"/Contents {number+1} 0 R ".encode() + extra + b" >>",
            b"<< /Length " + str(len(operators)).encode() + b" >>\nstream\n" + operators + b"\nendstream",
        ])
    objects[1] = b"<< /Type /Pages /Count " + str(len(ids)).encode() + b" /Kids [" + b" ".join(f"{i} 0 R".encode() for i in ids) + b"] >>"
    with path.open("wb") as stream:
        stream.write(b"%PDF-1.7\n")
        offsets = []
        for number, obj in enumerate(objects, 1):
            offsets.append(stream.tell())
            stream.write(f"{number} 0 obj\n".encode() + obj + b"\nendobj\n")
        xref = stream.tell()
        stream.write(f"xref\n0 {len(objects)+1}\n0000000000 65535 f \n".encode())
        for offset in offsets:
            stream.write(f"{offset:010d} 00000 n \n".encode())
        stream.write(f"trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())


def text_page(text):
    return f"BT /F1 12 Tf 20 220 Td ({text}) Tj ET".encode(), b""


class WindowsWorkerTree:
    """Retain native identities of this test's worker tree before killing its CLI."""

    def __init__(self, case, process):
        import ctypes
        from ctypes import wintypes

        class Entry(ctypes.Structure):
            _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD),
                        ("th32ProcessID", wintypes.DWORD), ("th32DefaultHeapID", ctypes.c_size_t),
                        ("th32ModuleID", wintypes.DWORD), ("cntThreads", wintypes.DWORD),
                        ("th32ParentProcessID", wintypes.DWORD), ("pcPriClassBase", wintypes.LONG),
                        ("dwFlags", wintypes.DWORD), ("szExeFile", wintypes.WCHAR * 260)]

        self.case, self.process = case, process
        self.ctypes, self.Entry = ctypes, Entry
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
        self.kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
        self.kernel.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
        self.kernel.Process32FirstW.restype = wintypes.BOOL
        self.kernel.Process32NextW.argtypes = self.kernel.Process32FirstW.argtypes
        self.kernel.Process32NextW.restype = wintypes.BOOL
        self.kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        self.kernel.OpenProcess.restype = wintypes.HANDLE
        self.kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
        self.kernel.GetProcessTimes.restype = wintypes.BOOL
        self.kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        self.kernel.WaitForSingleObject.restype = wintypes.DWORD
        self.kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
        self.kernel.TerminateProcess.restype = wintypes.BOOL
        self.kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        self.kernel.CloseHandle.restype = wintypes.BOOL
        self.handles = {}
        self.names = {}
        self.FILETIME = wintypes.FILETIME
        case.addCleanup(self.cleanup)
        root_created = self.creation_time(process._handle)
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            case.assertIsNone(process.poll(), "CLI exited before its real worker was observed")
            snapshot = self.snapshot()
            owned = {process.pid}
            while True:
                children = {pid for pid, (parent, _) in snapshot.items() if parent in owned}
                if children <= owned:
                    break
                owned |= children
            owned.discard(process.pid)
            if not any(snapshot[pid][1].casefold() == Path(RUNTIME).name.casefold() for pid in owned):
                time.sleep(0.01)
                continue
            created = {process.pid: root_created}
            pending = set(owned)
            while pending:
                children = {pid for pid in pending if snapshot[pid][0] in created}
                if not children:
                    break
                for pid in children:
                    pending.remove(pid)
                    # Original process handles avoid treating a later reused PID
                    # as this worker. TERMINATE is only for failed-test cleanup.
                    handle = self.kernel.OpenProcess(0x00101001, False, pid)
                    if not handle:
                        raise ctypes.WinError(ctypes.get_last_error())
                    child_created = self.creation_time(handle)
                    # Toolhelp parent PIDs survive their parents. Each child
                    # must have been created after its retained original parent.
                    if child_created < created[snapshot[pid][0]]:
                        self.kernel.CloseHandle(handle)
                        continue
                    if pid in self.handles:
                        self.kernel.CloseHandle(handle)
                    else:
                        self.handles[pid] = handle
                        self.names[pid] = snapshot[pid][1]
                    created[pid] = child_created
            if any(name.casefold() == Path(RUNTIME).name.casefold() for name in self.names.values()):
                return
        case.fail("real native PDF worker not observed within 5 seconds")

    def creation_time(self, handle):
        times = [self.FILETIME() for _ in range(4)]
        if not self.kernel.GetProcessTimes(handle, *(self.ctypes.byref(value) for value in times)):
            raise self.ctypes.WinError(self.ctypes.get_last_error())
        return (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime

    def snapshot(self):
        handle = self.kernel.CreateToolhelp32Snapshot(2, 0)
        if handle == self.ctypes.c_void_p(-1).value:
            raise self.ctypes.WinError(self.ctypes.get_last_error())
        try:
            entry = self.Entry()
            entry.dwSize = self.ctypes.sizeof(entry)
            result = {}
            more = self.kernel.Process32FirstW(handle, self.ctypes.byref(entry))
            while more:
                result[entry.th32ProcessID] = (entry.th32ParentProcessID, entry.szExeFile)
                more = self.kernel.Process32NextW(handle, self.ctypes.byref(entry))
            if self.ctypes.get_last_error() != 18:  # ERROR_NO_MORE_FILES
                raise self.ctypes.WinError(self.ctypes.get_last_error())
            return result
        finally:
            self.kernel.CloseHandle(handle)

    def alive(self):
        live = []
        for pid, handle in self.handles.items():
            state = self.kernel.WaitForSingleObject(handle, 0)
            if state == 258:  # WAIT_TIMEOUT
                live.append(pid)
            elif state != 0:  # WAIT_OBJECT_0
                raise self.ctypes.WinError(self.ctypes.get_last_error())
        return live

    def wait_exit(self, timeout=10):
        deadline = time.monotonic() + timeout
        while self.alive() and time.monotonic() < deadline:
            time.sleep(0.01)
        self.case.assertEqual(self.alive(), [], "worker tree survived a root-only CLI kill")

    def cleanup(self):
        try:
            stop_error = None
            try:
                self.case.stop_process(self.process)
            except BaseException as error:
                stop_error = error
            for pid in self.alive():
                if not self.kernel.TerminateProcess(self.handles[pid], 1):
                    raise self.ctypes.WinError(self.ctypes.get_last_error())
            self.wait_exit()
            if stop_error:
                raise stop_error
        finally:
            for handle in self.handles.values():
                self.kernel.CloseHandle(handle)
            self.handles.clear()


@unittest.skipUnless(CLI and RUNTIME, "explicit production CLI/PDF runtime required")
class CorpusProductionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="noetome-corpus-production-")
        # unittest runs cleanups after tearDown, in reverse registration order.
        # Stop a spawned CLI and its workers before deleting its source files.
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.project = self.root / "project 中文"
        self.source = self.root / "authored source café.pdf"
        self.sentinel = self.root / "outside-sentinel.txt"
        self.sentinel.write_bytes(b"unselected outside content\n")
        self.sentinel_sha = digest(self.sentinel)
        self.env = dict(os.environ, NOETOME_PDF_PYTHON=RUNTIME,
                        NOETOME_CORPUS_OCR_PROFILE=str(self.root / "no-ocr-profile.json"))
        initialized = subprocess.run([CLI, "project", "init", str(self.project)], env=self.env, capture_output=True, timeout=30)
        self.assertEqual(initialized.returncode, 0, initialized.stderr.decode())
        self.assertTrue((self.project / "project.json").is_file())
        self.source_sha = None

    def tearDown(self):
        self.assertEqual(digest(self.sentinel), self.sentinel_sha)
        if self.source_sha:
            self.assertEqual(digest(self.source), self.source_sha)

    @property
    def db(self):
        return self.project / ".noetome-corpus/corpus.sqlite"

    def call(self, *args, expected=0, timeout=90):
        result = subprocess.run([CLI, *map(str, args)], env=self.env, capture_output=True, timeout=timeout)
        self.assertEqual(result.returncode, expected, result.stderr.decode() + result.stdout.decode())
        if expected:
            return result.stderr.decode()
        return json.loads(result.stdout)

    def import_pdf(self, pages):
        write_pdf(self.source, pages)
        self.source_sha = digest(self.source)
        return self.call("corpus", "import", self.project, self.source)

    def page_rows(self, source_id):
        with closing(sqlite3.connect(f"file:{self.db}?mode=ro", uri=True)) as db:
            return db.execute("SELECT page_number,status,page_sha256 FROM pages WHERE source_id=? ORDER BY page_number", (source_id,)).fetchall()

    def wait_progress(self, source_id, minimum, process):
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            rows = self.page_rows(source_id)
            if sum(status != "pending" for _, status, _ in rows) >= minimum:
                return rows
            self.assertIsNone(process.poll(), "process exited before observed progress")
            time.sleep(0.025)
        self.fail("durable page progress not observed in 45 seconds")

    def start_process(self, source_id):
        return subprocess.Popen([CLI, "corpus", "process", str(self.project), source_id], env=self.env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=os.name == "posix")

    def stop_process(self, process):
        if process.poll() is None:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            else:
                stopped = subprocess.run(["taskkill.exe", "/PID", str(process.pid), "/T", "/F"],
                                         capture_output=True, timeout=15)
                self.assertEqual(stopped.returncode, 0, stopped.stderr.decode(errors="replace"))
        process.communicate(timeout=15)

    def slow_book(self):
        # Literal glyph extraction takes long enough to interrupt after a real
        # batch; a small file still represents all physical pages independently.
        operators = b"\n".join(f"BT /F1 8 Tf 20 {20+n%250} Td (Authored line {n}) Tj ET".encode() for n in range(300))
        return [(operators, b"")] * 64

    def test_full_inventory_paginated_content_and_same_hash_reuse(self):
        imported = self.import_pdf([text_page(f"Page {n}") for n in range(1, 162)])
        source_id = imported["source_id"]
        self.assertEqual((imported["page_count"], imported["processed_pages"], imported["status"]), (161, 0, "queued"))
        duplicate = self.call("corpus", "import", self.project, self.source)
        self.assertEqual(duplicate["source_id"], source_id)
        result = self.call("corpus", "process", self.project, source_id)
        self.assertEqual((result["status"], result["processed_pages"], result["text_pages"]), ("ready", 161, 161))
        tail = self.call("corpus", "pages", self.project, source_id, 150, 50)
        self.assertEqual([page["page_number"] for page in tail["pages"]], list(range(151, 162)))
        self.assertEqual([page["text"] for page in tail["pages"]], [f"Page {n}" for n in range(151, 162)])
        last = tail["pages"][-1]
        self.assertEqual(last["source_id"], source_id)
        self.assertEqual(len(last["page_revision"]), 64)
        self.assertEqual(last["regions"][0]["region_id"],
                         f"{source_id}:p161:{last['page_revision'][:16]}:r0")
        reopened = self.call("corpus", "pages", self.project, source_id, 160, 1)["pages"][0]
        self.assertEqual(reopened["page_revision"], last["page_revision"])
        self.assertEqual(reopened["regions"][0]["region_id"], last["regions"][0]["region_id"])
        self.assertEqual(len(self.call("corpus", "list", self.project)["sources"]), 1)

    def test_blank_scan_failed_page_keep_exact_physical_coverage(self):
        image = b"q 200 0 0 300 0 0 cm /Im1 Do Q"
        imported = self.import_pdf([text_page("Before"), (b"", b""), (image, b""), (b"", b"/UserUnit 0"), text_page("After")])
        source_id = imported["source_id"]
        result = self.call("corpus", "process", self.project, source_id)
        self.assertEqual((result["status"], result["processed_pages"], result["text_pages"], result["review_pages"], result["failed_pages"], result["blank_pages"]), ("needs_review", 5, 2, 1, 1, 1))
        page_list = self.call("corpus", "pages", self.project, source_id, 0, 50)["pages"]
        self.assertEqual([page["status"] for page in page_list], ["text_ready", "blank", "needs_ocr", "failed", "text_ready"])
        self.assertEqual(page_list[3]["issues"][0]["code"], "PDF_PAGE_GEOMETRY")
        self.assertEqual(page_list[-1]["text"], "After")

    def test_pause_reopen_reuses_confirmed_pages_without_duplicate_counts(self):
        source_id = self.import_pdf(self.slow_book())["source_id"]
        process = self.start_process(source_id)
        self.addCleanup(self.stop_process, process)
        self.wait_progress(source_id, 8, process)
        self.call("corpus", "pause", self.project, source_id)
        stdout, stderr = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, stderr.decode())
        paused = json.loads(stdout)
        self.assertEqual(paused["status"], "paused")
        self.assertGreaterEqual(paused["processed_pages"], 8)
        self.assertLess(paused["processed_pages"], 64)
        prior = {n: sha for n, status, sha in self.page_rows(source_id) if status != "pending"}
        resumed = self.call("corpus", "process", self.project, source_id)
        self.assertEqual(resumed["processed_pages"], 64)
        current = dict((n, sha) for n, _, sha in self.page_rows(source_id))
        self.assertEqual({n: current[n] for n in prior}, prior)

    def test_killed_cli_recovers_without_losing_or_rewriting_confirmed_pages(self):
        source_id = self.import_pdf(self.slow_book())["source_id"]
        process = self.start_process(source_id)
        self.addCleanup(self.stop_process, process)
        self.wait_progress(source_id, 8, process)
        self.assertIsNone(process.poll())
        worker_tree = WindowsWorkerTree(self, process) if os.name == "nt" else None
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            # Kill only the CLI. A test-side taskkill /T would conceal a broken
            # Windows host Job lifetime guard.
            process.kill()
        process.communicate(timeout=15)
        if worker_tree:
            worker_tree.wait_exit()
        if os.name == "posix":
            self.assertEqual(process.returncode, -signal.SIGKILL)
        else:
            self.assertNotEqual(process.returncode, 0)
        prior = {n: sha for n, status, sha in self.page_rows(source_id) if status != "pending"}
        self.assertGreaterEqual(len(prior), 8)
        self.assertLess(len(prior), 64)
        raw = self.project / ".noetome-corpus" / source_id / "raw.pdf"
        self.assertEqual(digest(raw), self.source_sha)
        resumed = self.call("corpus", "process", self.project, source_id)
        self.assertEqual(resumed["processed_pages"], 64)
        current = dict((n, sha) for n, _, sha in self.page_rows(source_id))
        self.assertEqual({n: current[n] for n in prior}, prior)
        with closing(sqlite3.connect(self.db)) as db:
            self.assertEqual(db.execute("PRAGMA integrity_check").fetchone(), ("ok",))

    def test_changed_selected_version_does_not_replace_prior_version(self):
        first = self.import_pdf([text_page("Old version")])
        first_raw = self.project / ".noetome-corpus" / first["source_id"] / "raw.pdf"
        original_sha = digest(first_raw)
        second = self.import_pdf([text_page("New version")])
        self.assertNotEqual(first["source_id"], second["source_id"])
        self.assertNotEqual(first["content_sha256"], second["content_sha256"])
        self.assertEqual(digest(first_raw), original_sha)
        self.assertEqual(len(self.call("corpus", "list", self.project)["sources"]), 2)

    def test_changed_saved_raw_is_rejected_before_progress(self):
        imported = self.import_pdf([text_page("Immutable")])
        source_id = imported["source_id"]
        raw = self.project / ".noetome-corpus" / source_id / "raw.pdf"
        with raw.open("ab") as stream:
            stream.write(b"\nchanged bytes\n")
        result = self.call("corpus", "process", self.project, source_id, expected=3)
        self.assertIn("CORPUS_SOURCE_CHANGED", result)
        self.assertEqual(self.call("corpus", "status", self.project, source_id)["processed_pages"], 0)

    def test_missing_saved_raw_persists_failed_state_and_can_resume(self):
        imported = self.import_pdf([text_page("Restorable")])
        source_id = imported["source_id"]
        raw = self.project / ".noetome-corpus" / source_id / "raw.pdf"
        saved = raw.read_bytes()
        raw.unlink()
        self.call("corpus", "process", self.project, source_id, expected=3)
        failure = self.call("corpus", "status", self.project, source_id)
        self.assertEqual(failure["status"], "failed")
        self.assertIsNotNone(failure["error"])
        self.assertEqual(failure["processed_pages"], 0)
        raw.write_bytes(saved)
        resumed = self.call("corpus", "process", self.project, source_id)
        self.assertEqual((resumed["status"], resumed["processed_pages"]), ("ready", 1))
        self.assertIsNone(resumed["error"])

    def test_explicit_retry_preserves_failed_revision_and_confirmed_neighbour(self):
        source_id = self.import_pdf([text_page("Confirmed"), (b"", b"/UserUnit 0")])["source_id"]
        original = self.call("corpus", "process", self.project, source_id)
        self.assertEqual((original["processed_pages"], original["failed_pages"]), (2, 1))
        prior = self.page_rows(source_id)
        retried = self.call("corpus", "retry", self.project, source_id)
        self.assertEqual((retried["processed_pages"], retried["failed_pages"]), (2, 1))
        self.assertEqual(self.page_rows(source_id)[0], prior[0])
        with closing(sqlite3.connect(self.db)) as db:
            archived = db.execute("SELECT page_sha256,page_json FROM page_revisions WHERE source_id=? AND page_number=2", (source_id,)).fetchall()
        self.assertIn(prior[1][2], [sha for sha, _ in archived])
        self.assertTrue(all(hashlib.sha256(raw.encode()).hexdigest() == sha for sha, raw in archived))

    def test_source_package_preserves_raw_partial_statuses_and_existing_destinations(self):
        image = b"q 200 0 0 300 0 0 cm /Im1 Do Q"
        source_id = self.import_pdf([text_page("Confirmed"), (image, b""), (b"", b"/UserUnit 0")])["source_id"]
        pending = self.root / "pending-package"
        pending_manifest = self.call("corpus", "export-source", self.project, source_id, pending)
        self.assertEqual(pending_manifest["page_status_counts"], {"pending": 3})
        self.assertEqual(pending_manifest["knowledge_status"], "pending")
        self.assertEqual(digest(pending / "raw.pdf"), self.source_sha)
        self.call("corpus", "process", self.project, source_id)
        destination = self.root / "source-package"
        manifest = self.call("corpus", "export-source", self.project, source_id, destination)
        self.assertEqual(manifest["page_status_counts"], {"text_ready": 1, "needs_ocr": 1, "failed": 1})
        self.assertEqual(manifest["source"]["needs_ocr_pages"], 1)
        self.assertEqual(manifest["exported_page_records"], 3)
        self.assertEqual(digest(destination / "raw.pdf"), self.source_sha)
        for name, pin in manifest["files"].items():
            self.assertEqual(digest(destination / name), pin["sha256"])
            self.assertEqual((destination / name).stat().st_size, pin["byte_length"])
        self.assertFalse((destination / "source-package.incomplete.json").exists())
        before = {str(p.relative_to(destination)): digest(p) for p in destination.rglob("*") if p.is_file()}
        self.assertIn("CORPUS_EXPORT_EXISTS", self.call("corpus", "export-source", self.project, source_id, destination, expected=3))
        self.assertEqual({str(p.relative_to(destination)): digest(p) for p in destination.rglob("*") if p.is_file()}, before)
        inside = self.project / "source-package"
        self.assertIn("CORPUS_EXPORT_PATH", self.call("corpus", "export-source", self.project, source_id, inside, expected=3))
        self.assertFalse(inside.exists())

    def test_changed_original_prevents_source_package_creation(self):
        imported = self.import_pdf([text_page("Immutable export")])
        raw = self.project / ".noetome-corpus" / imported["source_id"] / "raw.pdf"
        with raw.open("ab") as stream:
            stream.write(b"modified")
        destination = self.root / "invalid-package"
        self.assertIn("CORPUS_SOURCE_CHANGED", self.call("corpus", "export-source", self.project, imported["source_id"], destination, expected=3))
        self.assertFalse(destination.exists())

    def test_running_source_cannot_be_exported_as_a_stable_package(self):
        source_id = self.import_pdf(self.slow_book())["source_id"]
        process = self.start_process(source_id)
        self.addCleanup(self.stop_process, process)
        self.wait_progress(source_id, 8, process)
        destination = self.root / "running-package"
        self.assertIn("CORPUS_BUSY", self.call("corpus", "export-source", self.project, source_id, destination, expected=3))
        self.assertFalse(destination.exists())
        self.call("corpus", "pause", self.project, source_id)
        process.communicate(timeout=20)

    def test_saved_page_hash_rejects_text_tampering(self):
        source_id = self.import_pdf([text_page("Saved content")])["source_id"]
        self.call("corpus", "process", self.project, source_id)
        with closing(sqlite3.connect(self.db)) as db, db:
            db.execute("UPDATE pages SET page_json='{}' WHERE source_id=?", (source_id,))
        result = self.call("corpus", "pages", self.project, source_id, 0, 1, expected=3)
        self.assertIn("CORPUS_PAGE_CHANGED", result)

    def test_filesystem_aliases_cannot_import_internal_raw_or_export_inside_project(self):
        source_id = self.import_pdf([text_page("Selected alias boundary")])["source_id"]
        repository = self.project / ".noetome-corpus"
        raw = repository / source_id / "raw.pdf"
        original_sources = self.call("corpus", "list", self.project)
        original_directories = {p.name for p in repository.iterdir() if p.is_dir()}

        def aliases(path):
            choices = [path]
            different_case = Path(str(path).swapcase())
            if different_case.exists() and os.path.samefile(path, different_case):
                choices.append(different_case)
            if os.name == "nt":
                # Namespace prefixes also change the spelling of the same NTFS
                # directory. Never infer aliases by lowercasing path strings.
                namespaced = Path("\\\\?\\" + str(path))
                self.assertTrue(os.path.samefile(path, namespaced))
                choices.append(namespaced)
            return choices

        for parent in aliases(repository):
            with self.subTest(operation="import", parent=str(parent)):
                self.assertIn("CORPUS_SOURCE_PATH", self.call(
                    "corpus", "import", self.project, parent / source_id / "raw.pdf", expected=3))
                self.assertEqual(self.call("corpus", "list", self.project), original_sources)
                self.assertEqual({p.name for p in repository.iterdir() if p.is_dir()}, original_directories)
                self.assertEqual(digest(raw), self.source_sha)
        for parent in aliases(self.project):
            destination = parent / "blocked-package"
            with self.subTest(operation="export", parent=str(parent)):
                self.assertIn("CORPUS_EXPORT_PATH", self.call(
                    "corpus", "export-source", self.project, source_id, destination, expected=3))
                self.assertFalse(destination.exists())

        if os.name == "posix":
            link = self.root / "project-link"
            link.symlink_to(self.project, target_is_directory=True)
            self.assertIn("PROJECT_UNSAFE_PATH", self.call(
                "corpus", "export-source", self.project, source_id, link / "blocked-package", expected=2))
            self.assertIn("PROJECT_UNSAFE_PATH", self.call(
                "corpus", "import", self.project, link / ".noetome-corpus" / source_id / "raw.pdf", expected=2))
            link.unlink()

        # A distinct case-sensitive sibling must remain valid; a similarly
        # spelled Windows sibling also proves this is not a string prefix rule.
        name = self.project.name.swapcase() + ("-sibling" if os.name == "nt" else "")
        sibling = self.root / name
        manifest = self.call("corpus", "export-source", self.project, source_id, sibling)
        self.assertEqual(manifest["exported_page_records"], 1)
        self.assertEqual(digest(sibling / "raw.pdf"), self.source_sha)
        self.assertEqual(self.call("corpus", "list", self.project), original_sources)


@unittest.skipUnless(RUNTIME, "explicit locked PDF runtime required")
class IndependentReadingOrderTests(unittest.TestCase):
    def test_staggered_columns_cannot_claim_confident_reading_order(self):
        with tempfile.TemporaryDirectory(prefix="noetome-corpus-column-") as directory:
            root = Path(directory)
            source = root / "authored.pdf"
            # Authors place complete columns at staggered heights; row sorting
            # interleaves paragraphs even though no two segments share a row.
            operators = b"BT /F1 10 Tf 10 220 Td (Left one) Tj 130 -10 Td (Right one) Tj -130 -10 Td (Left two) Tj 130 -10 Td (Right two) Tj ET"
            write_pdf(source, [(operators, b"")])
            result = subprocess.run([RUNTIME, "-B", "-X", "utf8", str(WORKER), "pages", "--input", str(source), "--output-dir", str(root / "output")], capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            page = json.loads((root / "output/pages/000001.json").read_bytes())
            self.assertEqual(page["status"], "needs_review", page)
            self.assertIn("PDF_COMPLEX_LAYOUT", {issue["code"] for issue in page["issues"]})
            self.assertEqual({region["text"] for region in page["regions"]}, {"Left one", "Left two", "Right one", "Right two"})


if __name__ == "__main__":
    unittest.main()
