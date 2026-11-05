"""New self-authored non-scientific notes, isolated installed Obsidian only."""
import argparse
import hashlib
import json
import string
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path


def pin(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def win(path):
    return subprocess.check_output(["wslpath", "-w", str(path)], text=True).strip()


def entity(text):
    return "".join(f"#{ord(c)};" if c in string.punctuation else c for c in text)


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("new_work", type=Path)
parser.add_argument("native_python", type=Path)
parser.add_argument("obsidian", type=Path)
parser.add_argument("powershell", type=Path)
parser.add_argument("playwright_module", type=Path)
parser.add_argument("--focused-boundaries", action="store_true")
args = parser.parse_args()
work = args.new_work
work.mkdir()
(work / "capture-script.mjs").write_bytes(Path(__file__).with_name("probe_capture.mjs").read_bytes())
(work / "launcher-script.py").write_bytes(Path(__file__).read_bytes())
vault = work / "vault"
profile = work / "profile"
vault.mkdir()
profile.mkdir()
warning = "仅为自造渲染能力探针，无科学含义，不是 Noetome 产品输出。\n\n"
notes = []


def note(name, filename, markdown, nodes=(), edges=0, edge_labels=(), ordered=(), required=True):
    (vault / filename).write_text("# " + name + "\n\n" + warning + markdown + "\n", encoding="utf-8")
    notes.append({"name": name, "file": filename, "nodes": list(nodes), "edges": edges,
                  "edgeLabels": list(edge_labels), "list": list(ordered), "required": required})


def fence(code):
    return "```mermaid\n" + code + "\n```\n"


note("基础顺序", "01-基础流程.md", fence('flowchart TD\n  n0["输入卡片"]\n  n1["复制标记"]\n  n2["输出卡片"]\n  n0 --> n1\n  n1 --> n2'),
     ["输入卡片", "复制标记", "输出卡片"], 2)
note("条件分支", "02-条件分支.md", fence('flowchart TD\n  n0["读取卡片"]\n  n1{"标记为 yes？"}\n  n2["放入甲盒"]\n  n3["放入乙盒"]\n  n0 --> n1\n  n1 -->|"yes：条件成立"| n2\n  n1 -->|"no：条件不成立"| n3') + '\n[[04-文字回退#完整步骤|查看文字回退]]\n',
     ["读取卡片", "标记为 yes？", "放入甲盒", "放入乙盒"], 3, ["yes：条件成立", "no：条件不成立"])
note("正负作用标签", "03-正负标签.md", fence('flowchart LR\n  n0["按钮甲"]\n  n1["指示灯乙"]\n  n2["指示灯丙"]\n  n0 -->|"+：启用"| n1\n  n0 -->|"−：停用"| n2'),
     ["按钮甲", "指示灯乙", "指示灯丙"], 2, ["+：启用", "−：停用"])
ordered = ["输入：读取卡片；若卡片破损则保留原卡，不继续分支。", "检查标记是否为 yes。", "yes：放入甲盒；no：放入乙盒。", "输出：保留卡片及分支记录。"]
note("文字回退", "04-文字回退.md", "## 完整步骤\n\n图形不可用时保留全部步骤、条件、输入输出与例外。\n\n" + "\n".join(f"{i+1}. {text}" for i, text in enumerate(ordered)) + '\n\n[[02-条件分支#条件分支|返回条件分支]]\n', ordered=ordered)
labels = ['中文 [括号] (圆括号) {花括号} | 竖线 ; 分号', '引号 "Q" 与 & 与字面 <br>', '反斜线 \\ 反引号 ` 星号 * 下划线 _ 美元 $']
code = 'flowchart TD\n' + '\n'.join(f'  n{i}["{entity(text)}"]' for i, text in enumerate(labels)) + '\n  n0 -->|"条件 #60; 2 #124; 另一路"| n1\n  n1 --> n2'
note("默认标签编码", "05-默认标签编码.md", fence(code), labels, 2, ['条件 < 2 | 另一路'], required=False)
note("纯 SVG 标签编码", "06-SVG标签编码.md", fence('%%{init: {"flowchart": {"htmlLabels": false}}}%%\n' + code), labels, 2, ['条件 < 2 | 另一路'], required=False)
newline_labels = ['第一行\n第二行', 'Tab A\tB']
note("标签换行边界", "07-标签换行边界.md", fence('flowchart TD\n  n0["第一行#10;第二行"]\n  n1["Tab A#9;B"]\n  n0 --> n1'), newline_labels, 1, required=False)
note("故意无效语法", "08-故意无效语法.md", fence('flowchart TD\n  n0["没有闭合'), ['没有闭合'], required=False)
if args.focused_boundaries:
    notes = [n for n in notes if n['file'].startswith(('05-', '06-', '07-', '08-'))]
before = {p.name: pin(p) for p in vault.glob('*.md')}
write(work / "author-before.json", {"scope": "self-authored capability probes frozen before opening Obsidian", "notes": notes, "markdown_pins": before})
(vault / ".obsidian").mkdir()
write(vault / ".obsidian/app.json", {"defaultViewMode": "preview"})
write(vault / ".obsidian/community-plugins.json", [])
write(profile / "obsidian.json", {"updateDisabled": True, "vaults": {uuid.uuid4().hex[:16]: {"path": win(vault), "ts": int(time.time()*1000), "open": True}}})
launch = '''import socket,subprocess,json,sys
from pathlib import Path
exe,profile,work=sys.argv[1:]
with socket.socket() as sock:
    sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
argv=[exe,'--user-data-dir='+profile,'--remote-debugging-port='+str(port),'--remote-debugging-address=127.0.0.1']
with (Path(work)/'obsidian.stdout.bin').open('xb') as out,(Path(work)/'obsidian.stderr.bin').open('xb') as err:
    p=subprocess.Popen(argv,stdout=out,stderr=err,creationflags=subprocess.DETACHED_PROCESS|subprocess.CREATE_NEW_PROCESS_GROUP)
print(json.dumps(dict(pid=p.pid,port=port,argv=argv)))
'''
(work / "launch_native.py").write_text(launch, encoding="utf-8")
run = subprocess.run([str(args.native_python), win(work / "launch_native.py"), win(args.obsidian), win(profile), win(work)], capture_output=True, timeout=30)
(work / "launch.stdout.bin").write_bytes(run.stdout)
(work / "launch.stderr.bin").write_bytes(run.stderr)
assert run.returncode == 0, run.stderr
receipt = json.loads(run.stdout)
write(work / "launch-receipt.json", {**receipt, "exit_code": run.returncode, "launcher": {"path": str(args.obsidian), **pin(args.obsidian)}})
write(work / "input.json", {"profile": win(profile), "vault": win(vault), "port": receipt["port"], "notes": notes})
for _ in range(120):
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{receipt["port"]}/json/version', timeout=1) as response:
            write(work / "cdp-version.json", json.load(response))
        break
    except OSError:
        time.sleep(.25)
else:
    raise RuntimeError("Own installed Obsidian CDP unavailable; inspect without restarting")
cmd = ["node", str(Path(__file__).with_name("probe_capture.mjs")), str(args.playwright_module), str(work)]
run = subprocess.run(cmd, capture_output=True, timeout=180)
(work / "capture.stdout.bin").write_bytes(run.stdout)
(work / "capture.stderr.bin").write_bytes(run.stderr)
write(work / "capture-receipt.json", {"argv": cmd, "exit_code": run.returncode})
after = {p.name: pin(p) for p in vault.glob('*.md')}
assert before == after
cleanup = "$p=Get-Process -Id " + str(receipt['pid']) + " -ErrorAction SilentlyContinue\n$l=Get-NetTCPConnection -LocalPort " + str(receipt['port']) + " -State Listen -ErrorAction SilentlyContinue\n@{ownedPidAbsent=($null -eq $p);ownedListenerAbsent=($null -eq $l)} | ConvertTo-Json -Compress\n"
(work / "cleanup.ps1").write_text(cleanup, encoding="utf-8")
probe = subprocess.run([str(args.powershell), "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", win(work / "cleanup.ps1")], capture_output=True, timeout=30)
(work / "cleanup.stdout.bin").write_bytes(probe.stdout)
(work / "cleanup.stderr.bin").write_bytes(probe.stderr)
check = json.loads(probe.stdout)
write(work / "protection-cleanup.json", {"markdown_unchanged": before == after, "markdown_after": after, "pid": receipt['pid'], "port": receipt['port'], **check})
assert check['ownedPidAbsent'] and check['ownedListenerAbsent'], check
assert run.returncode == 0, (work / "result.json").read_text()[-2000:]
print(run.stdout.decode())
