"""Capture an actual authorized check, without recording unrelated environment."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
sys.stderr.reconfigure(encoding='utf-8', errors='replace')

parser = argparse.ArgumentParser()
parser.add_argument('--name', required=True)
parser.add_argument('--pdf-python')
parser.add_argument('--test-python')
parser.add_argument('--compiler-temp', action='store_true')
parser.add_argument('command', nargs=argparse.REMAINDER)
args = parser.parse_args()
command = args.command[1:] if args.command[:1] == ['--'] else args.command
root = Path.cwd().resolve()
out = root / '.artifacts' / 'b05u'
out.mkdir(parents=True, exist_ok=True)
environment = os.environ.copy()
environment['PYTHONIOENCODING'] = 'utf-8'
environment['CARGO_TERM_COLOR'] = 'never'
chosen = {}
if args.pdf_python:
    for key in ('NOETOME_TEST_PYTHON', 'NOETOME_PDF_PYTHON'):
        environment[key] = str(Path(args.pdf_python).resolve())
        chosen[key] = environment[key]
if args.test_python:
    environment['NOETOME_TEST_PYTHON'] = str(Path(args.test_python).resolve())
    chosen['NOETOME_TEST_PYTHON'] = environment['NOETOME_TEST_PYTHON']
if args.compiler_temp:
    temporary = root / 'target' / 'compiler-temp'
    if temporary.exists() and (not temporary.is_dir() or temporary.is_symlink() or temporary.is_junction()):
        raise SystemExit('Compiler temporary directory must be ordinary.')
    temporary.mkdir(parents=True, exist_ok=True)
    environment['TEMP'] = environment['TMP'] = str(temporary)
    chosen['compiler_temp'] = str(temporary)
if Path(command[0]).name.lower() == 'node.exe':
    environment['PATH'] = str(Path(command[0]).parent) + os.pathsep + environment['PATH']
started = datetime.now(timezone.utc).isoformat()
clock = time.monotonic()
result = subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
streams = {}
for label, content in [('stdout', result.stdout), ('stderr', result.stderr)]:
    content = content.replace(b'\r\n', b'\n')
    path = out / f'{args.name}-{label}.log'
    path.write_bytes(content)
    streams[label] = {'path': path.relative_to(root).as_posix(), 'bytes': len(content), 'sha256': hashlib.sha256(content).hexdigest()}
report = {'command': command, 'cwd': str(root), 'selected_environment': chosen, 'started_utc': started, 'elapsed_seconds': round(time.monotonic() - clock, 3), 'exit_code': result.returncode, 'stream_normalization': 'CRLF to LF only', 'streams': streams}
(out / f'{args.name}.json').write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(report, ensure_ascii=True))
for content in [result.stdout, result.stderr]:
    lines = content.decode('utf-8', 'replace').splitlines()
    print('\n'.join(lines[-14:]))
sys.exit(result.returncode)
