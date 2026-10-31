"""Manage the repository's development ledger and derived Markdown views.

The JSON ledger is authoritative. This tool records supplied evidence; it
never infers successful testing from commits or marks tasks done by itself.
"""

from __future__ import annotations

import argparse
from collections import Counter
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tempfile
from urllib.parse import quote


STATUSES = {"todo", "in_progress", "blocked", "done"}
LABELS = {"todo": "待开始", "in_progress": "进行中", "blocked": "已阻塞", "done": "已完成"}
TRANSITIONS = {"start": ({"todo"}, "in_progress"), "complete": ({"in_progress"}, "done"),
               "block": ({"todo", "in_progress"}, "blocked"), "reopen": ({"blocked", "done"}, "todo")}
VIEWS = ("TODO.md", "DONE.md", "STATUS.md")
LOG_OUTPUTS = {"dev-log/tasks.json", *("dev-log/" + name.lower() for name in VIEWS)}


class DevlogError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise DevlogError(message)


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def string_list(value, label, allow_empty=True):
    require(isinstance(value, list) and all(nonempty(item) for item in value), f"{label} 必须是非空字符串的列表")
    require(allow_empty or bool(value), f"{label} 不得为空")
    require(len(value) == len(set(value)), f"{label} 存在重复项")


def timestamp(value):
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        require(parsed.tzinfo is not None, "历史时间必须包含时区")
        return parsed
    except (AttributeError, TypeError, ValueError) as exc:
        raise DevlogError(f"无效历史时间 {value!r}") from exc


def validate(data):
    require(isinstance(data, dict), "账本必须是 JSON 对象")
    require(type(data.get("schema_version")) is int and data["schema_version"] == 1, "仅支持 schema_version 1")
    require(nonempty(data.get("current_batch")), "缺少 current_batch")
    tasks = data.get("tasks")
    require(isinstance(tasks, list) and bool(tasks), "tasks 必须是非空列表")
    ids = []
    for task in tasks:
        require(isinstance(task, dict), "任务必须是对象")
        for name in ("id", "title", "batch"):
            require(nonempty(task.get(name)), f"任务缺少 {name}")
        ident = task["id"]
        ids.append(ident)
        require(isinstance(task.get("status"), str) and task["status"] in STATUSES, f"{ident} 状态无效")
        for name in ("depends_on", "requirements", "acceptance"):
            string_list(task.get(name), f"{ident}.{name}", allow_empty=name != "acceptance")
        history = task.get("history")
        require(isinstance(history, list), f"{ident}.history 必须是列表")
        previous_status, previous_time = "todo", None
        for event in history:
            require(isinstance(event, dict), f"{ident} 历史事件必须是对象")
            action = event.get("action")
            require(isinstance(action, str) and action in TRANSITIONS, f"{ident} 历史动作无效")
            allowed, target = TRANSITIONS[action]
            require(event.get("from_status") == previous_status and previous_status in allowed,
                    f"{ident} 历史状态链不连续")
            require(event.get("to_status") == target, f"{ident} 历史目标状态错误")
            require(nonempty(event.get("note")), f"{ident} 历史事件缺少说明")
            when = timestamp(event.get("at"))
            require(previous_time is None or when >= previous_time, f"{ident} 历史时间倒序")
            validations, artifacts = event.get("validation", []), event.get("artifacts", [])
            string_list(validations, f"{ident} 验证说明")
            require(isinstance(artifacts, list), f"{ident} 产物必须是列表")
            if action == "complete":
                require(validations and artifacts, f"{ident} 完成事件缺少验证说明或产物")
            for artifact in artifacts:
                require(isinstance(artifact, dict), f"{ident} 产物记录无效")
                path, digest = artifact.get("path"), artifact.get("sha256")
                require(nonempty(path) and not PurePosixPath(path).is_absolute()
                        and ".." not in PurePosixPath(path).parts and "\\" not in path and ":" not in path,
                        f"{ident} 产物必须是仓库内相对路径")
                require(isinstance(digest, str) and len(digest) == 64
                        and all(char in "0123456789abcdef" for char in digest), f"{ident} 产物哈希无效")
            previous_status, previous_time = target, when
        require(previous_status == task["status"], f"{ident} 当前状态与历史不一致；非 todo 初态也必须有历史")
    require(len(ids) == len(set(ids)), "任务 ID 重复")
    index = {task["id"]: task for task in tasks}
    require(any(task["batch"] == data["current_batch"] for task in tasks), "当前批次没有任务")
    require(sum(task["status"] == "in_progress" for task in tasks) <= 1, "同时只能有一个进行中任务")
    for task in tasks:
        for dep in task["depends_on"]:
            require(dep in index and dep != task["id"], f"{task['id']} 依赖不存在或依赖自身")
            if task["status"] in {"in_progress", "done"}:
                require(index[dep]["status"] == "done", f"{task['id']} 的依赖 {dep} 尚未完成")
        if task["status"] == "in_progress":
            require(task["batch"] == data["current_batch"], "进行中任务必须属于当前批次")
    visited, visiting = set(), set()
    def visit(ident):
        require(ident not in visiting, f"存在循环依赖 {ident}")
        if ident in visited:
            return
        visiting.add(ident)
        for dep in index[ident]["depends_on"]:
            visit(dep)
        visiting.remove(ident)
        visited.add(ident)
    for ident in index:
        visit(ident)
    batch_history = data.get("history", [])
    require(isinstance(batch_history, list), "顶层 history 必须是列表")
    previous_batch, previous_time = None, None
    for event in batch_history:
        require(isinstance(event, dict) and event.get("action") == "batch", "批次历史动作错误")
        require(all(nonempty(event.get(key)) for key in ("from_batch", "to_batch", "note")), "批次历史缺少字段")
        require(previous_batch is None or previous_batch == event["from_batch"], "批次历史不连续")
        when = timestamp(event.get("at"))
        require(previous_time is None or when >= previous_time, "批次历史时间倒序")
        previous_batch, previous_time = event["to_batch"], when
    require(previous_batch is None or previous_batch == data["current_batch"], "当前批次与历史不一致")
    return index


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"JSON 字段重复 {key}")
        result[key] = value
    return result


def load(root):
    path = root / "dev-log" / "tasks.json"
    try:
        raw = path.read_bytes()
        data = json.loads(raw.decode("utf-8-sig"), object_pairs_hook=unique_object)
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise DevlogError(f"无法读取账本 {path}: {exc}") from exc
    validate(data)
    return data, raw


@contextmanager
def writer_lock(root):
    directory = root / "dev-log"
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / ".devlog.lock").open("a+b") as lock:
        lock.seek(0, os.SEEK_END)
        if lock.tell() == 0:
            lock.write(b"\0")
            lock.flush()
        lock.seek(0)
        try:
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            raise DevlogError("另一个开发日志写操作正在运行，请稍后重试") from exc
        try:
            yield
        finally:
            lock.seek(0)
            if os.name == "nt":
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock.fileno(), fcntl.LOCK_UN)


def atomic_write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = None
    try:
        with tempfile.NamedTemporaryFile(mode="wb", dir=path.parent, prefix="." + path.name + ".",
                                         suffix=".tmp", delete=False) as stream:
            temp_path = Path(stream.name)
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, path)
    finally:
        if temp_path is not None and temp_path.exists():
            temp_path.unlink()


def md(value):
    return " ".join(str(value).split()).replace("|", "\\|")


def display_time(value):
    local = timestamp(value).astimezone(timezone(timedelta(hours=8)))
    return local.strftime("%Y-%m-%d %H:%M:%S") + " +08:00（Asia/Shanghai）"


def views(data, raw):
    digest = hashlib.sha256(raw).hexdigest()
    marker = f"<!-- Generated by scripts/devlog.py; ledger_sha256={digest} -->\n\n"
    todo = ["# 开发待办", "", marker.rstrip(), "", f"当前批次：{md(data['current_batch'])}。状态以 tasks.json 为准，请通过日志工具更新。", ""]
    remaining = [task for task in data["tasks"] if task["status"] != "done"]
    if not remaining:
        todo += ["所有已登记任务均已完成。", ""]
    for task in remaining:
        todo += [f"## {md(task['id'])} {md(task['title'])}", "",
                 f"状态：{LABELS[task['status']]} · 批次：{md(task['batch'])}", "",
                 "依赖：" + ("、".join(map(md, task["depends_on"])) or "无") + "。", "",
                 "需求：" + ("、".join(map(md, task["requirements"])) or "工程治理任务") + "。", "",
                 "验收条件：", ""]
        todo += ["- " + md(item) for item in task["acceptance"]]
        if task["history"]:
            todo += ["", "最近记录：" + md(task["history"][-1]["note"])]
        todo.append("")
    done = ["# 开发完成历史", "", marker.rstrip(), "",
            "以下保留每次完成事件及当时提交的证据。重新打开任务不会删除旧记录；当前状态仍以 tasks.json 为准。", "",
            "历史文件内容由 Git 版本记录保留；产物链接指向当前文件，完成时指纹不等于历史文件副本。显示时间使用 Asia/Shanghai（+08:00）。", ""]
    completions = [(event["at"], task, event) for task in data["tasks"] for event in task["history"] if event["action"] == "complete"]
    for _, task, event in sorted(completions, key=lambda item: timestamp(item[0])):
        done += [f"## {md(task['id'])} {md(task['title'])}", "",
                 f"完成时间：{display_time(event['at'])} · 当前状态：{LABELS[task['status']]} · 批次：{md(task['batch'])}", "",
                 md(event["note"]), "", "验证记录：", ""]
        done += ["- " + md(item) for item in event["validation"]]
        done += ["", "产物与完成时指纹：", ""]
        for artifact in event["artifacts"]:
            done.append(f"- [{md(artifact['path'])}](../{quote(artifact['path'], safe='/')}) · SHA256 `{artifact['sha256']}`")
        done.append("")
    if not completions:
        done += ["尚无完成记录。", ""]
    counts = Counter(task["status"] for task in data["tasks"])
    status = ["# 开发状态", "", marker.rstrip(), "", f"当前批次：{md(data['current_batch'])}", "",
              "| 状态 | 任务数 |", "| --- | --- |"]
    status += [f"| {LABELS[name]} | {counts[name]} |" for name in ("todo", "in_progress", "blocked", "done")]
    status += ["", "本批次：", ""]
    status += [f"- {md(task['id'])} {md(task['title'])}：{LABELS[task['status']]}" for task in data["tasks"] if task["batch"] == data["current_batch"]]
    status += ["", "完成记录是执行者提交的验证证据，不代表工具自行运行测试或验收。", ""]
    return dict(zip(VIEWS, ("\n".join(todo), "\n".join(done), "\n".join(status))))


def write_views(root, data, raw):
    for name, content in views(data, raw).items():
        atomic_write(root / "dev-log" / name, content.encode("utf-8"))


def render(root):
    with writer_lock(root):
        data, raw = load(root)
        write_views(root, data, raw)
    return "已根据 tasks.json 刷新 TODO、DONE 和 STATUS"


def check(root):
    """Read only: suitable for pre-commit without changing files or taking a write lock."""
    data, raw = load(root)
    for name, expected in views(data, raw).items():
        path = root / "dev-log" / name
        require(path.is_file() and path.read_bytes() == expected.encode("utf-8"), f"{name} 缺失或过期，请运行 devlog.py render")
    require((root / "dev-log" / "tasks.json").read_bytes() == raw, "检查期间账本发生变化，请重新检查")
    return f"开发日志检查通过，共 {len(data['tasks'])} 项任务；未修改任何文件"


def commit(root, data):
    validate(data)
    raw = (json.dumps(data, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    atomic_write(root / "dev-log" / "tasks.json", raw)
    try:
        write_views(root, data, raw)
    except OSError as exc:
        raise DevlogError("账本已安全更新，但视图刷新失败；请运行 devlog.py render，勿重复执行状态命令") from exc


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def collect_artifacts(root, paths):
    result, seen = [], set()
    for supplied in paths:
        candidate = Path(supplied)
        resolved = (candidate if candidate.is_absolute() else root / candidate).resolve()
        try:
            relative = resolved.relative_to(root.resolve()).as_posix()
        except ValueError as exc:
            raise DevlogError(f"产物必须位于当前仓库内：{supplied}") from exc
        require(resolved.is_file(), f"产物文件不存在：{supplied}")
        require(relative.casefold() not in LOG_OUTPUTS, "完成产物不能引用开发日志账本或生成视图，请指定实际工作文件")
        if relative not in seen:
            result.append({"path": relative, "sha256": hashlib.sha256(resolved.read_bytes()).hexdigest()})
            seen.add(relative)
    return result


def change_task(root, action, ident, note, validation=(), artifacts=(), report=None):
    require(nonempty(note), "必须提供 --note 说明")
    with writer_lock(root):
        data, _ = load(root)
        index = {task["id"]: task for task in data["tasks"]}
        require(ident in index, f"任务不存在：{ident}")
        task = index[ident]
        allowed, target = TRANSITIONS[action]
        require(task["status"] in allowed, f"{ident} 当前状态不能执行 {action}")
        if action == "start":
            require(task["batch"] == data["current_batch"], "只能启动当前批次任务")
            require(not any(item["status"] == "in_progress" for item in data["tasks"]), "已有进行中任务")
            require(all(index[dep]["status"] == "done" for dep in task["depends_on"]), "任务依赖尚未全部完成")
        if action == "reopen":
            require(not any(ident in item["depends_on"] and item["status"] in {"done", "in_progress"} for item in data["tasks"]),
                    "需先重新打开或阻塞依赖此任务的已完成及进行中任务")
        evidence = []
        verification = None
        if action == "complete":
            require(bool(validation) and all(nonempty(item) for item in validation), "完成必须提供 --validation 验证说明")
            require(bool(artifacts), "完成必须提供 --artifact 文件路径")
            evidence = collect_artifacts(root, artifacts)
            if "plan_version" in task:
                from verify import require_report
                verification = require_report(root, task, report)
        event = {"at": now(), "action": action, "from_status": task["status"], "to_status": target,
                 "note": note.strip(), "validation": list(validation), "artifacts": evidence}
        if verification is not None:
            event["verification"] = verification
        task["history"].append(event)
        task["status"] = target
        commit(root, data)
    return f"{ident} 已更新为{LABELS[target]}，历史与视图已记录"


def change_batch(root, batch, note, repair=False):
    require(nonempty(note), "切换批次必须提供 --note")
    with writer_lock(root):
        data, _ = load(root)
        current = data["current_batch"]
        require(batch != current, "目标已是当前批次")
        require(any(task["batch"] == batch for task in data["tasks"]), "目标批次没有任务")
        if repair:
            require(batch < current, "--repair只允许退回更早批次，不能跳过前进门禁")
            require(not any(task["status"] == "in_progress" for task in data["tasks"]), "退回修复前需先阻塞当前活动任务")
            require(any(task["batch"] == batch and task["status"] != "done" for task in data["tasks"]), "目标批次没有待修复任务，请先按依赖重新打开")
        else:
            require(all(task["status"] == "done" for task in data["tasks"] if task["batch"] == current), "当前批次仍有未完成或阻塞任务")
            remaining = sorted({task["batch"] for task in data["tasks"] if task["status"] != "done"})
            require(remaining and batch == remaining[0], "只能进入最早未完成批次，不能跳过中间批次")
        data.setdefault("history", []).append({"at": now(), "action": "batch", "from_batch": current, "to_batch": batch, "note": note.strip(), "repair": repair})
        data["current_batch"] = batch
        commit(root, data)
    return f"当前批次已切换为 {batch}"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1], help="仓库根目录，通常无需指定")
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("start", "complete", "block", "reopen", "batch"):
        command = commands.add_parser(name)
        command.add_argument("id")
        command.add_argument("--note", required=True)
        if name == "complete":
            command.add_argument("--validation", action="append", required=True)
            command.add_argument("--artifact", action="append", required=True)
            command.add_argument("--report", help="计划任务的自动验证报告路径")
        if name == "batch":
            command.add_argument("--repair", action="store_true", help="保留当前未完成任务，退回已重新打开的更早批次修复")
    commands.add_parser("check")
    commands.add_parser("render")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    try:
        if args.command == "check":
            result = check(root)
        elif args.command == "render":
            result = render(root)
        elif args.command == "batch":
            result = change_batch(root, args.id, args.note, args.repair)
        else:
            result = change_task(root, args.command, args.id, args.note,
                                 getattr(args, "validation", ()), getattr(args, "artifact", ()), getattr(args, "report", None))
        print(result)
        return 0
    except (DevlogError, OSError) as exc:
        print(f"开发日志错误：{exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
