from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
import zipfile

ROOT = Path('/home/lyinn/project/Noetome')
BASE = Path('/mnt/d/Noetome/.artifacts/b06v-t03-windows-python-final-02-20261106')
BASE.mkdir()
SNAP = BASE / 'repo'
SNAP.mkdir()

def pin(path):
    body = path.read_bytes()
    return {'sha256': hashlib.sha256(body).hexdigest(), 'byte_length': len(body)}

def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

paths = {path for path in json.loads((ROOT / '.artifacts/b06v-t03-linux-final-03-20261106/source-before.json').read_bytes()) if not path.startswith('target/')}
paths = sorted(paths)
before = {path: pin(ROOT / path) for path in paths}
assert before['scripts/prepare_visual_compatibility.py']['sha256'] == '9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f'
for path in paths:
    destination = SNAP / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / path, destination)
assert {path: pin(SNAP / path) for path in paths} == before
save(BASE / 'source-before.json', before)
with zipfile.ZipFile(BASE / 'source-originals.zip', 'x', zipfile.ZIP_DEFLATED) as archive:
    for path in paths:
        archive.write(SNAP / path, path)
    archive.writestr('source-before.json', (BASE / 'source-before.json').read_bytes())
with zipfile.ZipFile(BASE / 'source-originals.zip') as archive:
    assert archive.testzip() is None
    for path in paths:
        body = archive.read(path)
        assert {'sha256': hashlib.sha256(body).hexdigest(), 'byte_length': len(body)} == before[path]

selected = Path('/mnt/d/Noetome/target/b06v-t03-final-rust-20261106-c/noetome-selected.exe')
expected_cli = {'sha256': 'e42c534d07f66662411fc52f22561510b79ba1c63f14f8b6f419db7c4e0e3274', 'byte_length': 25939968}
assert pin(selected) == expected_cli
cli = SNAP / 'target/debug/noetome.exe'
cli.parent.mkdir(parents=True)
shutil.copyfile(selected, cli)
assert pin(cli) == expected_cli and cli.stat().st_nlink == 1
(SNAP / 'target/compiler-temp').mkdir()
shutil.copyfile(__file__, BASE / 'orchestrator.py')

win_base = 'D:\\Noetome\\.artifacts\\b06v-t03-windows-python-final-02-20261106'
win_cli = win_base + '\\repo\\target\\debug\\noetome.exe'
win_python = 'D:\\Noetome\\.artifacts\\b03-t02-python312\\Scripts\\python.exe'
ps = BASE / 'run-python.ps1'
ps.write_text(
    "$ErrorActionPreference='Stop'\n"
    "$env:PSExecutionPolicyPreference='Bypass'\n"
    "$env:PATH='C:\\Users\\Lyinn\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\native\\powershell;C:\\WINDOWS\\System32\\WindowsPowerShell\\v1.0;'+$env:PATH\n"
    f"$env:NOETOME_PDF_PYTHON='{win_python}'\n"
    "$env:NOETOME_TEST_PYTHON='C:\\Users\\Lyinn\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\python.exe'\n"
    f"$env:NOETOME_TEST_BINARY='{win_cli}'\n"
    "Remove-Item Env:NOETOME_TANGMAO_API_KEY -ErrorAction SilentlyContinue\n"
    f"Set-Location '{win_base}\\repo'\n"
    f"& '{win_python}' -X utf8 -m unittest discover -s tests -p 'test_*.py' -v\n"
    "exit $LASTEXITCODE\n", encoding='utf-8')
argv = ['/mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', win_base + '\\run-python.ps1']
env = os.environ.copy()
env.pop('NOETOME_TANGMAO_API_KEY', None)
started = time.monotonic()
with (BASE / 'python.stdout.bin').open('xb') as stdout, (BASE / 'python.stderr.bin').open('xb') as stderr:
    process = subprocess.Popen(argv, cwd=SNAP, env=env, stdout=stdout, stderr=stderr)
    save(BASE / 'launch.json', {'argv': argv, 'pid': process.pid, 'platform': 'windows', 'selected_cli': expected_cli, 'source_count': len(paths)})
    print(json.dumps({'base': str(BASE), 'pid': process.pid, 'source_count': len(paths)}), flush=True)
    code = process.wait(timeout=900)
after = {path: pin(SNAP / path) for path in paths}
worktree_after = {path: pin(ROOT / path) for path in paths}
save(BASE / 'source-after.json', after)
save(BASE / 'worktree-after.json', worktree_after)
raw_stderr = (BASE / 'python.stderr.bin').read_bytes()
decoded = raw_stderr.decode('utf-8', 'strict')
match = re.search(r'Ran (\d+) tests in ([\d.]+)s', decoded)
skips = [line for line in decoded.splitlines() if ' ... skipped ' in line]
result = {'schema_version': 1, 'platform': 'windows', 'argv': argv, 'exit_code': code, 'wall_seconds': time.monotonic() - started, 'tests_run': int(match[1]) if match else None, 'test_seconds': float(match[2]) if match else None, 'summary': decoded[-1500:], 'skips': skips, 'source_count': len(paths), 'snapshot_unchanged': before == after, 'worktree_unchanged': before == worktree_after, 'cli_unchanged': pin(cli) == pin(selected) == expected_cli, 'cli': expected_cli, 'stdout': pin(BASE / 'python.stdout.bin'), 'stderr': pin(BASE / 'python.stderr.bin'), 'source_archive': pin(BASE / 'source-originals.zip'), 'real_model_calls': 0, 'scope': 'Full Python on frozen final source; ordinary selected native PE; synthetic local HTTP and DPAPI test inputs, no real credentials or production Vault'}
save(BASE / 'result.json', result)
print(json.dumps(result, ensure_ascii=False), flush=True)
assert code == 0 and match and 'OK' in decoded
assert result['snapshot_unchanged'] and result['worktree_unchanged'] and result['cli_unchanged']
