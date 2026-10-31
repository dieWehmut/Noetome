import json
from contextlib import redirect_stdout
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import autotest


class AutotestTests(unittest.TestCase):
    def setUp(self):
        capture = redirect_stdout(io.StringIO())
        capture.__enter__()
        self.addCleanup(capture.__exit__, None, None, None)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_failure_stops_before_task_and_is_retained(self):
        with patch.object(autotest.subprocess, 'run', side_effect=[Mock(returncode=0), Mock(returncode=1)]) as child:
            report = autotest.run(self.root, 'B01-T01')
        self.assertEqual(child.call_count, 2)
        self.assertFalse(report['passed'])
        self.assertEqual(json.loads((self.root / '.artifacts/autotest/latest.json').read_text(encoding='utf-8'))['checks'][-1]['exit_code'], 1)

    def test_default_scope_does_not_claim_product_verified(self):
        with patch.object(autotest.subprocess, 'run', return_value=Mock(returncode=0)):
            report = autotest.run(self.root)
        self.assertTrue(report['passed'])
        self.assertEqual(report['scope'], 'repository_only')
        self.assertFalse(any('scripts/verify.py' in r['argv'] for r in report['checks']))

    def test_task_checks_are_run_after_repository_checks(self):
        with patch.object(autotest.subprocess, 'run', return_value=Mock(returncode=0)) as child:
            report = autotest.run(self.root, 'B01-T01')
        self.assertTrue(report['passed'])
        self.assertEqual(child.call_args.args[0][-2:], ['scripts/verify.py', 'B01-T01'])


if __name__ == '__main__':
    unittest.main()
