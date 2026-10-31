"""Regression tests for bounded selection and evidence bound to actual sources."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import devlog
import plan
import verify


def task(ident='B01-T01', deps=(), batch='B01'):
    return dict(id=ident, title=ident, batch=batch, status='todo', depends_on=list(deps), requirements=[],
                acceptance=['observable behavior'], history=[])


class PlanVerifyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        (self.root / '.gitignore').write_text('.artifacts/\ndev-log/.devlog.lock\n', encoding='utf-8')
        (self.root / 'dev-log').mkdir()
        self.task = task()
        self.task.update(plan_version=1, specs=['spec.md'], steps=['implement behavior'], outputs=['source.py'],
                         non_goals=['scope limit'], failure_policy=['retain failure evidence'], scenarios=[], quality=[],
                         checks=[dict(name='smoke', argv=['{python}', '-c', 'print("Ran 1 test in 0.001s")'], timeout_s=5, min_tests=1)])
        self.data = dict(schema_version=1, current_batch='B01', tasks=[self.task])
        (self.root / 'spec.md').write_text('spec', encoding='utf-8')
        (self.root / 'source.py').write_text('answer = 42\n', encoding='utf-8')
        self.save()

    def save(self):
        (self.root / 'dev-log/tasks.json').write_text(json.dumps(self.data), encoding='utf-8')
        devlog.render(self.root)

    def test_next_reads_only_and_respects_dependency_batch(self):
        self.data['tasks'] += [task('B01-T02', ['B01-T01']), task('B02-T01', ['B01-T02'], 'B02')]
        self.save()
        before = (self.root / 'dev-log/tasks.json').read_bytes()
        self.assertEqual(plan.next_action(self.data), {'action': 'start', 'task': 'B01-T01'})
        self.assertEqual(before, (self.root / 'dev-log/tasks.json').read_bytes())
        devlog.change_task(self.root, 'start', 'B01-T01', 'start')
        current, _ = devlog.load(self.root)
        self.assertEqual(plan.next_action(current)['action'], 'resume')
        devlog.change_task(self.root, 'block', 'B01-T01', 'missing capability')
        current, _ = devlog.load(self.root)
        self.assertEqual(plan.next_action(current)['action'], 'blocked')

    def test_blocked_task_does_not_hide_independent_ready_task(self):
        self.data['tasks'].append(task('B01-T02'))
        self.save()
        devlog.change_task(self.root, 'block', 'B01-T01', 'missing capability')
        current, _ = devlog.load(self.root)
        self.assertEqual(plan.next_action(current), {'action': 'start', 'task': 'B01-T02'})

    def test_actual_check_report_allows_completion_and_survives_log_update(self):
        devlog.change_task(self.root, 'start', 'B01-T01', 'start')
        report = verify.verify(self.root, 'B01-T01')
        self.assertTrue(report['passed'])
        devlog.change_task(self.root, 'complete', 'B01-T01', 'done', ['ran behavior test'], ['source.py'],
                           '.artifacts/verification/B01-T01.json')
        current, _ = devlog.load(self.root)
        self.assertEqual(plan.next_action(current), {'action': 'complete'})
        self.assertTrue(current['tasks'][0]['history'][-1]['verification']['passed'])
        devlog.check(self.root)

    def test_source_change_or_task_change_invalidates_evidence(self):
        verify.verify(self.root, 'B01-T01')
        supplied = '.artifacts/verification/B01-T01.json'
        (self.root / 'source.py').write_text('answer = 43\n', encoding='utf-8')
        with self.assertRaisesRegex(devlog.DevlogError, '源码改变'):
            verify.require_report(self.root, self.task, supplied)
        verify.verify(self.root, 'B01-T01')
        changed = copy.deepcopy(self.task)
        changed['acceptance'].append('new behavior')
        with self.assertRaisesRegex(devlog.DevlogError, '定义改变'):
            verify.require_report(self.root, changed, supplied)

    def test_zero_tests_and_command_failure_are_not_passes(self):
        for source in ('print("Ran 0 tests in 0.001s")', 'raise SystemExit(2)'):
            self.task['checks'][0]['argv'] = ['{python}', '-c', source]
            self.save()
            self.assertFalse(verify.verify(self.root, 'B01-T01')['passed'])

    def test_missing_output_and_mutation_during_check_fail(self):
        (self.root / 'source.py').unlink()
        self.assertFalse(verify.verify(self.root, 'B01-T01')['passed'])
        (self.root / 'source.py').write_text('before', encoding='utf-8')
        self.task['checks'][0]['argv'] = ['{python}', '-c', 'from pathlib import Path; Path("source.py").write_text("after"); print("Ran 1 test in 0.001s")']
        self.save()
        result = verify.verify(self.root, 'B01-T01')
        self.assertFalse(result['tree_unchanged'])
        self.assertFalse(result['passed'])

    def test_completion_cannot_bypass_missing_or_wrong_report(self):
        devlog.change_task(self.root, 'start', 'B01-T01', 'start')
        with self.assertRaisesRegex(devlog.DevlogError, '--report'):
            devlog.change_task(self.root, 'complete', 'B01-T01', 'done', ['checked'], ['source.py'])
        result = verify.verify(self.root, 'B01-T01')
        result['task_id'] = 'B02-T01'
        (self.root / '.artifacts/verification/B01-T01.json').write_text(json.dumps(result), encoding='utf-8')
        with self.assertRaisesRegex(devlog.DevlogError, '任务或版本不符'):
            verify.require_report(self.root, self.task, '.artifacts/verification/B01-T01.json')

    def test_outside_spec_and_missing_traceability_are_rejected(self):
        with self.assertRaisesRegex(devlog.DevlogError, '缺少覆盖'):
            plan.validate_plan(self.root, self.data)
        self.task['specs'] = ['../outside.md']
        with self.assertRaisesRegex(devlog.DevlogError, '非法计划路径'):
            plan.validate_plan(self.root, self.data)

    def test_test_summary_count_does_not_count_ignored_rust_cases(self):
        self.assertEqual(verify.test_count('test result: ok. 2 passed; 0 failed; 5 ignored\n'), 2)
        self.assertEqual(verify.test_count(' Test Files 3 passed\n      Tests 12 passed (12)\n'), 12)
        self.assertEqual(verify.test_count('Ran 0 tests in 0.0s'), 0)

    def test_missing_executable_and_timeout_fail(self):
        self.task['checks'][0]['argv'] = ['this-executable-does-not-exist-noetome']
        self.save()
        self.assertFalse(verify.verify(self.root, 'B01-T01')['passed'])
        self.task['checks'][0].update(argv=['{python}', '-c', 'import time; time.sleep(10)'], timeout_s=1)
        self.save()
        report = verify.verify(self.root, 'B01-T01')
        self.assertTrue(report['checks'][0]['timed_out'])
        self.assertFalse(report['passed'])

    def test_malformed_report_is_rejected_with_actionable_error(self):
        report = self.root / 'invalid.json'
        report.write_text('{"broken":', encoding='utf-8')
        with self.assertRaisesRegex(devlog.DevlogError, '有效UTF-8 JSON'):
            verify.require_report(self.root, self.task, 'invalid.json')
        report.write_text('[]', encoding='utf-8')
        with self.assertRaisesRegex(devlog.DevlogError, '必须是对象'):
            verify.require_report(self.root, self.task, 'invalid.json')

    @unittest.skipUnless(sys.platform == 'win32', 'Windows launcher rule')
    def test_windows_pnpm_rejects_shell_metacharacters(self):
        check = dict(name='bad', argv=['pnpm', 'test', 'x&echo'], timeout_s=5)
        with self.assertRaisesRegex(devlog.DevlogError, 'shell元字符'):
            verify.run_check(self.root, check)


if __name__ == '__main__':
    unittest.main()
