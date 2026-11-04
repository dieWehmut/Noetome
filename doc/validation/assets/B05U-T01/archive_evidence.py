"""Archive actual B05U checks; never copy user references or secret contents."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from PIL import Image

root = Path.cwd()
local = root / '.artifacts/b05u'
archive = root / 'doc/validation/assets/B05U-T01'
archive.mkdir(parents=True, exist_ok=True)

def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(name, value):
    (archive / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')

def git(*args):
    return subprocess.check_output(['git', *args], cwd=root).decode('utf-8').strip()

copies = []
def copy(source, filename=None):
    destination = archive / (filename or source.name)
    shutil.copyfile(source, destination)
    assert digest(source) == digest(destination)
    copies.append({'original': source.relative_to(root).as_posix(), 'archived': destination.relative_to(root).as_posix(), 'sha256': digest(destination), 'bytes': destination.stat().st_size})

commands = ['devlog-scope', 'python-tests-final', 'rust-tests-final', 'rust-tests-verified',
            'web-check-final', 'web-check-review-fixes', 'web-build-final', 'desktop-build',
            'desktop-build-verified', 'desktop-build-final-source', 'desktop-install',
            'desktop-install-verified', 'native-acceptance', 'fmt', 'cargo-check',
            'devlog-before-integration', 'diff-before-integration']
for name in commands:
    report = read(local / (name + '.json'))
    copy(local / (name + '.json'))
    for stream in report['streams'].values():
        source = root / stream['path']
        assert source.stat().st_size == stream['bytes'] and digest(source) == stream['sha256']
        copy(source, source.stem + '.txt')

for name in ['initial-test-failures.json', 'browser-red.json', 'review-red.json',
             'browser-behavior.json', 'browser-console.json', 'native-first.json',
             'native-restart.json', 'native-supervisor.json', 'settings-before.json',
             'settings-after.json']:
    copy(local / name)
copy(root / '.artifacts/desktop-install.json', 'installer-payload.json')
for name in ['archive_evidence.py', 'run_command.py', 'native_check.mjs', 'native_supervisor.py',
             'capture_settings.py', 'create_theme_fixtures.py',
             'theme-invalid-test.html', 'theme-read-test.html', 'theme-write-test.html']:
    copy(local / name)

browser = read(local / 'browser-behavior.json')
selected = [0, 2, 3, 5, 6, 7, 8, 9, 14, 15, 18, 20, 21, 22, 23, 24, 25, 26, 27]
checks = [{'history_index': index, **browser['checks'][index]} for index in selected]
assert all(item['passed'] for item in checks)
console_errors = [item for item in read(local / 'browser-console.json') if item['level'] in ('warn', 'error')]
assert not console_errors, console_errors
write('browser-gate.json', {'history_is_preserved': True, 'history_rows': len(browser['checks']),
    'selected_passed_checks': len(checks), 'console_warnings_and_errors': console_errors,
    'excluded_pre_fix_contrast_row': 12, 'note': 'Historical RED and harness failures are retained, not counted as final passes. Preview IPC and storage fault fixtures assert expected errors.', 'checks': checks})

rust_rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', (local / 'rust-tests-verified-stdout.log').read_text(encoding='utf-8'))
rust = {'suites': len(rust_rows), 'passed': sum(int(row[0]) for row in rust_rows),
        'failed': sum(int(row[1]) for row in rust_rows), 'ignored': sum(int(row[2]) for row in rust_rows)}
assert rust == {'suites': 17, 'passed': 591, 'failed': 0, 'ignored': 0}
assert read(local / 'rust-tests-verified.json')['exit_code'] == 0
python_text = (local / 'python-tests-final-stderr.log').read_text(encoding='utf-8')
assert re.search(r'Ran 157 tests in [\d.]+s\s+OK\s*$', python_text)
assert read(local / 'python-tests-final.json')['exit_code'] == 0
native = [read(local / name) for name in ('native-first.json', 'native-restart.json')]
assert all(report['passed'] and not report['errors'] and all(check['passed'] for check in report['checks']) for report in native)
assert read(local / 'native-supervisor.json')['passed']
installation = read(root / '.artifacts/desktop-install.json')
assert installation['matches_current_build'] and installation['installer_exit_code'] == 0
assert installation['executable_sha256'] == read(local / 'native-supervisor.json')['installed_sha256']
before, after = [read(local / name) for name in ('settings-before.json', 'settings-after.json')]
assert len(before) == 4 and before.keys() == after.keys()
protection = []
for index, (filename, original) in enumerate(before.items()):
    current = after[filename]
    if index < 3:
        assert original == current
    else:
        assert original['sha256'] == current['sha256']
    protection.append({'file': str(Path(filename).relative_to(os.environ['LOCALAPPDATA'])),
        'before': original, 'after': current, 'content_unchanged': original['sha256'] == current['sha256'],
        'mtime_unchanged': original['mtime_ns'] == current['mtime_ns'],
        'mode': 'hash-only read' if index < 3 else 'real UI save and original bytes restored'})
write('settings-protection.json', {'passed': True, 'decrypted_credentials': False, 'files': protection})

source_head = git('rev-parse', 'HEAD')
source_names = subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode('utf-8').split('\0')
sources = []
for name in source_names:
    if name and (name.startswith(('apps/desktop/', 'crates/', 'scripts/', 'tests/')) or name in ('Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json')):
        path = root / name
        if path.is_file():
            sources.append({'path': name, 'sha256': digest(path), 'bytes': path.stat().st_size})
rust_python_diff = git('diff', '--name-only', 'eb674f1', '--', 'crates', 'scripts', 'tests', 'Cargo.toml', 'Cargo.lock')
assert rust_python_diff == ''
write('source-fingerprints.json', {'source_commit': source_head, 'rust_python_unchanged_from': git('rev-parse', 'eb674f1'), 'files': sources})

images = []
for path in sorted((root / 'doc/validation/assets').glob('B05U-T01-*.png')):
    with Image.open(path) as picture:
        assert picture.format == 'PNG'
        images.append({'path': path.relative_to(root).as_posix(), 'format': picture.format, 'width': picture.width, 'height': picture.height, 'sha256': digest(path), 'bytes': path.stat().st_size})
write('screenshots.json', {'note': 'Only Noetome images are archived. Browser captures originally returned JPEG bytes and were converted to PNG; installed WebView captures are original PNG at DPR 2.', 'images': images})
write('review.json', {'reviewer': '/root/ui_review', 'mode': 'read-only source review',
    'base': git('rev-parse', 'eb674f1'), 'initial_head': git('rev-parse', '7887f2e'),
    'initial_important_findings': ['Fresh search omitted unvisited settings pages', 'Narrow sidebar overlay obscured focused controls after navigation'],
    'fix_commit': source_head, 'browser_red_history_indices': [16, 17], 'browser_green_history_indices': [18, 20],
    'followup_scope': '7887f2e..012fd8b', 'followup_result': 'Ready to merge from source perspective; no actionable Critical, Important or Minor findings. Installed native acceptance was still required at review time and subsequently passed.',
    'native_acceptance': 'native-supervisor.json'})
write('gate.json', {'at_utc': datetime.now(timezone.utc).isoformat(), 'source_commit': source_head,
    'ui_acceptance_passed': True, 'rust': rust, 'python_passed': 157,
    'browser_selected_passed': len(checks), 'native_checks_passed': sum(len(report['checks']) for report in native),
    'installed_executable_sha256': installation['executable_sha256'],
    'git_integration': 'Pending actual main creation, push and remote default verification',
    'limitations': ['Windows CUA pointer/key input failed to invoke controls; actual installed WebView UI was tested via process-local loopback diagnostics instead.', 'No fresh full tray, physical window-drag, production Vault or model quality matrix is claimed.']})
write('archive-map.json', {'copied_bytes_verified': True, 'streams_normalized_before_archive': 'CRLF to LF only by capture runner', 'files': copies})
print(f'Archived {len(copies)} original check artifacts, {len(images)} Noetome PNG images; Rust {rust["passed"]}, Python 157, browser {len(checks)}, native 7 passed.')
