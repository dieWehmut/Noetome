"""Run task checks without a shell and retain evidence bound to the source tree."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

import devlog
import plan


IGNORED_EVIDENCE = {"dev-log/tasks.json", "dev-log/TODO.md", "dev-log/DONE.md", "dev-log/STATUS.md"}


def tree_hash(root):
    proc = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
                          cwd=root, capture_output=True, check=True)
    digest = hashlib.sha256()
    for name in sorted(set(proc.stdout.decode("utf-8").split("\0")) - {""}):
        if name in IGNORED_EVIDENCE or name.startswith((".artifacts/", "dev-log/evidence/")):
            continue
        path = root / name
        digest.update(name.encode("utf-8") + b"\0")
        if path.is_symlink():
            digest.update(b"symlink:" + os.readlink(path).encode("utf-8"))
        elif path.is_file():
            digest.update(hashlib.sha256(path.read_bytes()).digest())
        else:
            digest.update(b"<deleted>")
    return digest.hexdigest()


def test_count(output):
    output = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', output)
    counts = [int(n) for n in re.findall(r"test result: ok\. (\d+) passed", output)]
    counts += [int(n) for n in re.findall(r"Ran (\d+) tests? in", output)]
    # Vitest summary contains a single aggregate Tests line; not Test Files.
    counts += [int(n) for n in re.findall(r"(?m)^\s*Tests\s+(\d+) passed", output)]
    counts += [int(n) for n in re.findall(r"(?m)^\s*(?:\[[^\]\r\n]+\]\s*)?(\d+) passing\b", output)]
    return sum(counts)


def run_check(root, check):
    argv = [sys.executable if arg == "{python}" else arg for arg in check["argv"]]
    started = time.monotonic()
    timed_out = False
    try:
        executable = argv[0]
        # Windows package-manager .cmd launchers require cmd.exe; the argv below
        # comes only from reviewed task definitions, never model/source material.
        if os.name == "nt" and executable == "pnpm":
            devlog.require(not any(re.search(r'[&|<>^%!\r\n\"]', arg) for arg in argv[1:]),
                           'Windows pnpm参数包含shell元字符，拒绝执行')
            argv = ["cmd.exe", "/d", "/c", "pnpm", *argv[1:]]
        proc = subprocess.Popen(argv, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                start_new_session=os.name != "nt")
        try:
            output, _ = proc.communicate(timeout=check["timeout_s"])
        except subprocess.TimeoutExpired:
            timed_out = True
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True)
            else:
                os.killpg(proc.pid, signal.SIGKILL)
            proc.kill()
            output, _ = proc.communicate()
        code = proc.returncode
    except OSError as exc:
        output, code = str(exc).encode("utf-8"), 127
    count = test_count(output.decode("utf-8", errors="replace"))
    result = {"name": check["name"], "argv": check["argv"], "exit_code": code,
              "duration_s": round(time.monotonic() - started, 3), "timed_out": timed_out,
              "tests_passed": count, "output_sha256": hashlib.sha256(output).hexdigest(),
              "passed": code == 0 and not timed_out and count >= check.get("min_tests", 0)}
    # Keep output out of the durable report (it can contain private fixture text).
    if not result["passed"]:
        print(output.decode("utf-8", errors="replace")[-12000:], file=sys.stderr)
    return result


def verify(root, ident):
    data, _ = devlog.load(root)
    task = next((t for t in data["tasks"] if t["id"] == ident), None)
    devlog.require(task is not None and "plan_version" in task, "任务没有完整验证定义")
    before = tree_hash(root)
    results = []
    for check in task["checks"]:
        result = run_check(root, check)
        results.append(result)
        print(f"{check['name']}: {'PASS' if result['passed'] else 'FAIL'}", flush=True)
        if not result["passed"]:
            break
    outputs = []
    for path in task["outputs"]:
        target = plan.internal(root, path)
        if target.is_file():
            outputs.append({"path": path, "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
    after = tree_hash(root)
    passed = (before == after and len(results) == len(task["checks"]) and all(r["passed"] for r in results)
              and len(outputs) == len(task["outputs"]))
    report = {"schema_version": 1, "task_id": ident, "at": datetime.now(timezone.utc).isoformat(),
              "definition_sha256": plan.definition_hash(task), "tree_sha256": after,
              "tree_unchanged": before == after, "checks": results, "outputs": outputs, "passed": passed}
    path = root / ".artifacts" / "verification" / f"{ident}.json"
    devlog.atomic_write(path, (json.dumps(report, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    print(f"验证报告：{path}；{'通过' if passed else '未通过（检查、产物或源码一致性）'}")
    return report


def require_report(root, task, supplied):
    devlog.require(supplied, "计划任务完成必须提供 --report（由 scripts/verify.py 生成）")
    path = (root / supplied).resolve()
    devlog.require(path.is_relative_to(root.resolve()) and path.is_file(), "验证报告必须在仓库内且存在")
    try:
        report = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=devlog.unique_object)
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise devlog.DevlogError('验证报告不是有效UTF-8 JSON') from exc
    devlog.require(isinstance(report, dict), '验证报告必须是对象')
    devlog.require(report.get("schema_version") == 1 and report.get("task_id") == task["id"], "验证报告任务或版本不符")
    devlog.require(report.get("passed") is True and report.get("tree_unchanged") is True, "验证报告未通过")
    devlog.require(report.get("definition_sha256") == plan.definition_hash(task), "验证后任务定义改变，请重新验证")
    devlog.require(report.get("tree_sha256") == tree_hash(root), "验证后源码改变，请重新验证")
    checks = report.get("checks", [])
    devlog.require(len(checks) == len(task["checks"]), "验证检查不完整")
    for expected, actual in zip(task["checks"], checks):
        devlog.require(actual.get("name") == expected["name"] and actual.get("argv") == expected["argv"]
                       and actual.get("exit_code") == 0 and actual.get("passed") is True
                       and actual.get("timed_out") is False
                       and actual.get("tests_passed", -1) >= expected.get("min_tests", 0), "验证检查失败或不匹配")
    expected_outputs = devlog.collect_artifacts(root, task["outputs"])
    devlog.require(report.get("outputs") == expected_outputs, "验证产物缺失或已改变")
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("task")
    args = parser.parse_args(argv)
    try:
        return 0 if verify(args.root.resolve(), args.task)["passed"] else 1
    except (devlog.DevlogError, OSError, subprocess.SubprocessError, ValueError) as exc:
        print(f"验证错误：{exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
