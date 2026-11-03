"""Run actual scoped project checks and save original command output."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'doc/validation/assets'
PWSH = sys.argv[2]
env = dict(os.environ)
env['NOETOME_PDF_PYTHON'] = sys.executable
env['NOETOME_TEST_PYTHON'] = sys.executable
env['CARGO_TERM_COLOR'] = 'never'

def run(name, command):
    path = OUT / ('B04U-T01-' + name + '.json')
    assert not path.exists(), path
    start = datetime.datetime.now(datetime.timezone.utc).isoformat()
    clock = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True)
    stdout = result.stdout.decode('utf-8')
    stderr = result.stderr.decode('utf-8')
    record = {'command': command, 'started_at': start, 'elapsed_seconds': time.monotonic() - clock,
              'exit_code': result.returncode, 'stdout': stdout, 'stderr': stderr,
              'stdout_sha256': hashlib.sha256(result.stdout).hexdigest(),
              'stderr_sha256': hashlib.sha256(result.stderr).hexdigest()}
    if name == 'workspace-tests':
        rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', stdout + stderr)
        record['result_rows'] = [[int(n) for n in row] for row in rows]
        for i, key in enumerate(('passed', 'failed', 'ignored')):
            record[key] = sum(int(row[i]) for row in rows)
    path.write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: record[k] for k in ('exit_code','elapsed_seconds')}
                     | {'step': name, 'passed': record.get('passed'), 'evidence': str(path)}, ensure_ascii=False), flush=True)
    if result.returncode:
        print((stdout + stderr)[-4000:], flush=True)
        raise SystemExit(result.returncode)
    return record

mode = sys.argv[1]
if mode == 'workspace':
    for name, text in [
        ('fmt', '.\\scripts\\cargo_local.ps1 fmt --all --check'),
        ('check', '.\\scripts\\cargo_local.ps1 check --workspace --locked --offline'),
        ('workspace-tests', ".\\scripts\\cargo_local.ps1 test --workspace --locked --offline '--' --test-threads=4")
    ]:
        record = run(name, [PWSH, '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', '& ' + text + '; exit $LASTEXITCODE'])
    assert record['passed'] == 499 and record['failed'] == record['ignored'] == 0, record
    binary = ROOT / 'target/debug/noetome.exe'
    summary = {'source_commit': subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT).decode().strip(),
               'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
               'binary_bytes': binary.stat().st_size, 'passed': record['passed'], 'model_calls': 0,
               'rust_test_threads': 4,
               'source_sha256': {str(p.relative_to(ROOT)).replace('\\','/'): hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in [ROOT/'crates/noetome-cli/src/extraction_api_config.rs',
                                           ROOT/'crates/noetome-cli/src/extraction_api_schema.rs',
                                           ROOT/'crates/noetome-cli/src/extraction_api_wire/chat.rs',
                                           ROOT/'crates/noetome-cli/tests/extraction_api.rs']}}
    (OUT/'B04U-T01-workspace-summary.json').write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8')
elif mode == 'desktop-build':
    env['PATH'] = str(Path(sys.argv[3]).parent) + os.pathsep + env['PATH']
    env['TEMP'] = env['TMP'] = str(ROOT/'target/compiler-temp')
    run('desktop-build', [sys.argv[3], 'node_modules/@tauri-apps/cli/tauri.js', 'build', '--config',
                          'crates/noetome-desktop/tauri.conf.json', '--ci', '--', '--locked', '--offline'])
else:
    raise ValueError(mode)
