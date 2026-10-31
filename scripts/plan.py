"""Read and validate the complete implementation plan; never execute tasks."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys

import devlog


PLAN_FIELDS = ("specs", "steps", "outputs", "non_goals", "failure_policy", "scenarios", "quality")


def internal(root, value):
    part = PurePosixPath(value)
    devlog.require(not part.is_absolute() and ".." not in part.parts
                   and "\\" not in value and ":" not in value, f"非法计划路径 {value}")
    target = (root / value).resolve()
    devlog.require(target.is_relative_to(root.resolve()), f"路径越界 {value}")
    return target


def validate_plan(root, data):
    index = devlog.validate(data)
    tasks = [t for t in data["tasks"] if "plan_version" in t]
    devlog.require(tasks, "未登记完整执行计划")
    seen = {"requirements": set(), "scenarios": set(), "quality": set()}
    for task in tasks:
        ident = task["id"]
        devlog.require(task["plan_version"] == 1, f"{ident} 计划版本无效")
        for field in PLAN_FIELDS:
            devlog.string_list(task.get(field), f"{ident}.{field}", allow_empty=field in {"scenarios", "quality"})
        for path in task["specs"]:
            devlog.require(internal(root, path).is_file(), f"{ident} 规范不存在 {path}")
        for path in task["outputs"]:
            internal(root, path)  # Planned outputs need not exist yet.
        checks = task.get("checks")
        devlog.require(isinstance(checks, list) and checks, f"{ident} 无验证命令")
        names = set()
        for check in checks:
            devlog.require(isinstance(check, dict), f"{ident} 检查必须是对象")
            devlog.require(devlog.nonempty(check.get("name")) and check["name"] not in names, f"{ident} 检查名无效")
            names.add(check["name"])
            devlog.string_list(check.get("argv"), f"{ident} 检查argv", allow_empty=False)
            devlog.require(type(check.get("timeout_s")) is int and 1 <= check["timeout_s"] <= 93600, f"{ident} 超时无效")
            devlog.require(type(check.get("min_tests", 0)) is int and check.get("min_tests", 0) >= 0, f"{ident} 测试数无效")
        for field, prefix, count in (("requirements", "FR", 40), ("scenarios", "AC", 40), ("quality", "NFR", 17)):
            for reference in task[field]:
                devlog.require(reference in {f"{prefix}-{n:02}" for n in range(1, count + 1)}, f"{ident} 未知需求 {reference}")
                seen[field].add(reference)
        for dep in task["depends_on"]:
            devlog.require(index[dep]["batch"] <= task["batch"], f"{ident} 依赖未来批次 {dep}")
    for field, prefix, count in (("requirements", "FR", 40), ("scenarios", "AC", 40), ("quality", "NFR", 17)):
        missing = {f"{prefix}-{n:02}" for n in range(1, count + 1)} - seen[field]
        devlog.require(not missing, f"计划缺少覆盖 {sorted(missing)}")
    return tasks


def definition_hash(task):
    definition = {key: value for key, value in task.items() if key not in {"status", "history"}}
    return hashlib.sha256(json.dumps(definition, ensure_ascii=False, sort_keys=True).encode()).hexdigest()


def card(task):
    lines = [f"## {task['id']} {task['title']}", "", "依赖：" + "、".join(task["depends_on"]), "",
             "关联：" + "、".join(task["requirements"] + task["scenarios"] + task["quality"]), ""]
    labels = (("specs", "先读规范"), ("steps", "实现步骤"), ("outputs", "必须交付的文件"),
              ("acceptance", "验收结果"), ("non_goals", "本任务边界"), ("failure_policy", "失败处理"))
    for field, title in labels:
        lines += [f"**{title}**", ""]
        for n, item in enumerate(task[field], 1):
            if field == "specs":
                lines.append(f"- [{item}](../../../{item})")
            elif field == "steps":
                lines.append(f"{n}. {item}")
            else:
                lines.append(f"- {item}")
        lines.append("")
    lines += ["**自动检查**（命令通过不替代上面的行为验收）", "", "```text",
              f"python scripts/verify.py {task['id']}"]
    lines += [" ".join(c["argv"]).replace("{python}", "python") for c in task["checks"]]
    lines += ["```", ""]
    return "\n".join(lines)


def generated(root, data):
    tasks = validate_plan(root, data)
    batches = sorted({t["batch"] for t in tasks})
    result = {}
    for batch in batches:
        selected = [t for t in tasks if t["batch"] == batch]
        content = f"# {batch} 执行卡\n\n由 `scripts/plan.py render` 从账本生成，禁止手工编辑。任务描述是开发要求，不表示已经实现。状态见 `dev-log/STATUS.md`。\n\n"
        content += "\n".join(card(t) for t in selected)
        result[f"doc/execution/tasks/{batch}.md"] = content
    lines = ["# 全项目需求追溯", "", "由任务账本生成。关联表示计划覆盖，不表示需求已通过；最终证据见 RELEASE 门禁。", "",
             "| 需求或场景 | 实现/验证任务 |", "| --- | --- |"]
    for field, prefix, count in (("requirements", "FR", 40), ("scenarios", "AC", 40), ("quality", "NFR", 17)):
        for n in range(1, count + 1):
            reference = f"{prefix}-{n:02}"
            owners = [f"[{t['id']}](tasks/{t['batch']}.md)" for t in tasks if reference in t[field]]
            lines.append(f"| {reference} | {', '.join(owners)} |")
    result["doc/execution/TRACEABILITY.md"] = "\n".join(lines) + "\n"
    return result


def next_action(data):
    index = devlog.validate(data)
    active = [t for t in data["tasks"] if t["status"] == "in_progress"]
    if active:
        return {"action": "resume", "task": active[0]["id"]}
    current = [t for t in data["tasks"] if t["batch"] == data["current_batch"]]
    ready = [t for t in current if t["status"] == "todo" and all(index[d]["status"] == "done" for d in t["depends_on"])]
    if ready:
        return {"action": "start", "task": ready[0]["id"]}
    if any(t["status"] != "done" for t in current):
        return {"action": "blocked", "tasks": [t["id"] for t in current if t["status"] != "done"],
                "instruction": "记录缺失证据或解除条件；禁止跳过批次或伪记完成。"}
    later = sorted({t["batch"] for t in data["tasks"] if t["status"] != "done"})
    return {"action": "batch", "batch": later[0]} if later else {"action": "complete"}


def check(root):
    data, raw = devlog.load(root)
    expected = generated(root, data)
    for path, content in expected.items():
        p = root / path
        devlog.require(p.is_file() and p.read_bytes() == content.encode("utf-8"), f"{path} 过期，请运行 plan.py render")
    devlog.require((root / "dev-log/tasks.json").read_bytes() == raw, "检查期间账本变化")
    return f"计划检查通过：{len([t for t in data['tasks'] if 'plan_version' in t])}张任务卡，40 FR / 40 AC / 17 NFR 全部有任务关联；不代表产品已验收"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("command", choices=("next", "show", "check", "render"))
    parser.add_argument("id", nargs="?")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    try:
        data, _ = devlog.load(root)
        if args.command == "next":
            print(json.dumps(next_action(data), ensure_ascii=False))
        elif args.command == "show":
            task = next((t for t in data["tasks"] if t["id"] == args.id), None)
            devlog.require(task is not None, "任务不存在")
            print(card(task) if "plan_version" in task else json.dumps(task, ensure_ascii=False, indent=2))
        elif args.command == "render":
            with devlog.writer_lock(root):
                data, _ = devlog.load(root)
                for path, content in generated(root, data).items():
                    devlog.atomic_write(root / path, content.encode("utf-8"))
            print("执行卡与追溯表已生成")
        else:
            print(check(root))
        return 0
    except (devlog.DevlogError, OSError) as exc:
        print(f"计划错误：{exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
