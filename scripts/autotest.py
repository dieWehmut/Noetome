"""One entry point for repository checks and the current implementation task."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import time

import devlog


def commands(task=None):
    base = [
        [sys.executable, '-X', 'utf8', '-m', 'unittest', 'discover', '-s', 'tests', '-p', 'test_*.py'],
        [sys.executable, '-X', 'utf8', 'scripts/devlog.py', 'check'],
        [sys.executable, '-X', 'utf8', 'scripts/plan.py', 'check'],
        [sys.executable, '-X', 'utf8', 'scripts/check_docs.py'],
        ['git', 'diff', '--check'],
    ]
    if task:
        base.append([sys.executable, '-X', 'utf8', 'scripts/verify.py', task])
    return base


def run(root, task=None):
    results = []
    for argv in commands(task):
        started = time.monotonic()
        try:
            code = subprocess.run(argv, cwd=root, check=False).returncode
        except OSError:
            code = 127
        results.append(dict(argv=argv, exit_code=code, duration_s=round(time.monotonic() - started, 3)))
        if code:
            break
    report = dict(schema_version=1, at=datetime.now(timezone.utc).isoformat(), task_id=task,
                  scope='repository_and_task' if task else 'repository_only',
                  passed=len(results) == len(commands(task)) and all(r['exit_code'] == 0 for r in results), checks=results)
    devlog.atomic_write(root / '.artifacts/autotest/latest.json',
                        (json.dumps(report, ensure_ascii=False, indent=2) + '\n').encode('utf-8'))
    print(f"自动检查{'通过' if report['passed'] else '失败'}；范围={report['scope']}。报告不是产品真实质量或用户验收的替代。", flush=True)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--task', help='本轮已实现任务；同时运行它的完整检查并生成verify报告')
    args = parser.parse_args(argv)
    try:
        return 0 if run(args.root.resolve(), args.task)['passed'] else 1
    except (OSError, devlog.DevlogError) as exc:
        print(f'自动检查错误：{exc}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
