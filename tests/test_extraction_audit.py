"""The independent audit must release SQLite before subsequent CLI writes."""

import gc
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest

from scripts.validate_extraction_gateway import sql_snapshot


class ExtractionAuditTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "Windows file sharing boundary")
    def test_sql_snapshot_releases_database_without_waiting_for_gc(self):
        parent = Path(__file__).resolve().parents[1] / "target" / "compiler-temp"
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as temporary:
            project = Path(temporary)
            database = project / "project.sqlite"
            connection = sqlite3.connect(database)
            try:
                connection.execute("CREATE TABLE evidence (id INTEGER, body TEXT)")
                connection.execute("INSERT INTO evidence VALUES (1, 'author row')")
                connection.commit()
            finally:
                connection.close()
            enabled = gc.isenabled()
            gc.disable()
            try:
                snapshot = sql_snapshot(project)
                self.assertIn('INSERT INTO "evidence" VALUES(1,\'author row\');', snapshot)
                # A retained SQLite handle prevents the next CLI publication
                # from replacing its database on Windows.
                moved = database.with_name("captured.sqlite")
                try:
                    database.rename(moved)
                except PermissionError:
                    self.fail("SQLite snapshot retained a handle after returning its rows")
                self.assertTrue(moved.is_file())
            finally:
                if enabled:
                    gc.enable()
                gc.collect()
