"""Archive completed, explicitly selected B06 native test evidence; never run tests."""
from __future__ import annotations
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path, PureWindowsPath
import re
import shutil
import stat
import zipfile


def fingerprint_bytes(data):
    return {'sha256': hashlib.sha256(data).hexdigest(), 'byte_length': len(data)}


def fingerprint(path):
    digest = hashlib.sha256()
    total = 0
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
            total += len(block)
    return {'sha256': digest.hexdigest(), 'byte_length': total}


def load(path):
    return json.loads(path.read_bytes())


def save(path, value):
    with path.open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('native_root', type=Path)
parser.add_argument('repository', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--confirmed-terminal', action='store_true', required=True,
                    help='Only supply after the owning process handles are confirmed terminal.')
parser.add_argument('--credential-temp-prefix', type=Path)
args = parser.parse_args()
root, repository, destination = args.native_root, args.repository, args.output
assert not destination.exists(), 'new archive directory required'
config = load(root / 'root.json')
final = load(root / 'native-final.json')
manifest = load(root / 'snapshot.json')
commands = load(root / 'native-commands.json')
assert load(root / 'native-running.json')['status'] == 'terminal'
assert set(final['required_latest_exits']) == {'fmt', 'workspace-check', 'workspace-tests', 'python-full', 'presentation-audit'}
assert all(v == 0 for v in final['required_latest_exits'].values())
assert final['commands'] == commands
assert final['head'] == manifest['head'] == config['snapshot_head']
assert final['tracked_before_after_match']
ntfs_pointer = load(root / 'native-ntfs-pointer.json')
assert ntfs_pointer['status'] == 'passed'


def local(native_path):
    native_path = native_path.removeprefix('\\\\?\\')
    relative = PureWindowsPath(native_path).relative_to(PureWindowsPath(config['native']))
    assert '..' not in relative.parts
    return root.joinpath(*relative.parts)


ntfs_root = local(ntfs_pointer['work'])
ntfs = load(ntfs_root / 'final.json')
assert ntfs['status'] == 'passed' and ntfs['snapshot_head'] == final['head']
assert ntfs['junction_cleanup']['junction_absent'] and ntfs['junction_cleanup']['protected_before_after_match']
assert load(ntfs_root / 'protected-before.json') == load(ntfs_root / 'protected-after.json')
for command in commands:
    assert command['tracked_before_after_match'] and command['external_sentinel_before_after_match']
    assert not command['timed_out']
    for field in ('stdout', 'stderr'):
        expected = command[field]
        assert fingerprint(local(expected['path'])) == {k: expected[k] for k in ('sha256', 'byte_length')}, (command['name'], field)
for command in ntfs['commands']:
    assert command['exit_code'] == command['expected_exit'] and command['protected_before_after_match']
    for field in ('stdout', 'stderr'):
        expected = command[field]
        assert fingerprint(local(expected['path'])) == {k: expected[k] for k in ('sha256', 'byte_length')}
first_python = next(c for c in commands if c['name'] == 'python-full' and c['attempt'] == 1)
assert first_python['exit_code'] == 1
initial_failure = local(first_python['stderr']['path']).read_text(encoding='utf-8')
assert 'FAILED (failures=15, skipped=2)' in initial_failure, 'preserve the observed TEMP failure history'

snapshot_checked = {}
current_mismatch = {}
required_source_prefixes = ('crates/', 'scripts/', 'tests/', 'sidecars/', 'fixtures/', 'schemas/')
required_source_names = {'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'package.json', 'package-lock.json'}
current_source_checked = {}
for name, expected in manifest['files'].items():
    actual = fingerprint(root / 'repo' / name)
    assert actual == expected, ('native snapshot changed', name)
    snapshot_checked[name] = actual
    present = repository / name
    now = fingerprint(present) if present.is_file() else None
    if name.startswith(required_source_prefixes) or name in required_source_names:
        assert now == actual, ('current tested source differs', name)
        current_source_checked[name] = now
    elif now != actual:
        current_mismatch[name] = {'tested': actual, 'current': now}

excluded = {}
archive_results = {}
links_not_followed = []
destination.mkdir(parents=True)
shutil.copyfile(Path(__file__), destination / 'archive_native_evidence.py')


def gather(path, archive_path, members):
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode):
        links_not_followed.append({'source': str(path), 'archive_path': archive_path, 'reason': 'link not followed'})
        return
    if stat.S_ISDIR(info.st_mode):
        members[archive_path.rstrip('/') + '/'] = (path, None)
        for child in sorted(path.iterdir()):
            gather(child, archive_path.rstrip('/') + '/' + child.name, members)
        return
    assert stat.S_ISREG(info.st_mode), ('unexpected evidence file type', str(path))
    if path.suffix.lower() in {'.exe', '.dll', '.pdb', '.rlib', '.rmeta', '.lib', '.o', '.obj', '.pyc'} or '__pycache__' in path.parts:
        excluded[str(path)] = {'reason': 'binary/build/cache not archived', **fingerprint(path)}
        return
    members[archive_path] = (path, fingerprint(path))


def archive(name, members, omissions=None):
    path = destination / (name + '.zip')
    expected = {}
    with zipfile.ZipFile(path, 'x', zipfile.ZIP_DEFLATED, compresslevel=9) as target:
        for name_in_zip, (source, before) in sorted(members.items()):
            assert not name_in_zip.startswith('/') and '..' not in Path(name_in_zip).parts
            if before is None:
                target.writestr(name_in_zip, b'')
                expected[name_in_zip] = {'directory': True}
            else:
                data = source.read_bytes()
                assert fingerprint_bytes(data) == before, ('evidence changed while archiving', source)
                target.writestr(name_in_zip, data)
                assert fingerprint(source) == before
                expected[name_in_zip] = {'directory': False, **before}
    with zipfile.ZipFile(path) as saved:
        assert saved.testzip() is None, ('CRC mismatch', path)
        assert set(saved.namelist()) == set(expected) and len(saved.namelist()) == len(expected)
        for member, wanted in expected.items():
            data = saved.read(member)
            if wanted['directory']:
                assert not data
            else:
                assert fingerprint_bytes(data) == {k: wanted[k] for k in ('sha256', 'byte_length')}, member
    result = {'archive': path.name, **fingerprint(path), 'crc_readback_passed': True,
              'all_member_sha256_and_lengths_read_back': True, 'members': expected,
              'file_count': sum(not m['directory'] for m in expected.values()),
              'directory_count': sum(m['directory'] for m in expected.values()),
              'uncompressed_file_bytes': sum(m.get('byte_length', 0) for m in expected.values()),
              'omitted_cumulative_snapshots': omissions or {}}
    save(destination / (name + '-manifest.json'), result)
    archive_results[name] = {k: v for k, v in result.items() if k not in ('members', 'omitted_cumulative_snapshots')}
    archive_results[name]['omitted_cumulative_count'] = len(omissions or {})


# Preserve both command attempts and prepared harnesses, including the old TEMP
# setting. Git archives and all executables receive fingerprints but no copy.
root_files = {}
for path in sorted(root.iterdir()):
    if not path.is_file():
        continue
    if path.name.endswith('.tar') or path.suffix.lower() == '.exe':
        excluded[str(path)] = {'reason': 'Git source archive / executable kept only by fingerprint', **fingerprint(path)}
    else:
        before = fingerprint(path)
        shutil.copyfile(path, destination / path.name)
        assert fingerprint(destination / path.name) == before
        root_files[path.name] = before
external = root.parent / (root.name + '-external-sentinel.txt')
assert fingerprint(external) == final['external_sentinel_before'] == final['external_sentinel_after']
shutil.copyfile(external, destination / 'external-sentinel.txt')
root_files['external-sentinel.txt'] = fingerprint(external)
focused_temp = None
if args.credential_temp_prefix is not None:
    for field in ('stdout', 'stderr'):
        source = Path(str(args.credential_temp_prefix) + '.' + field + '.bin')
        target = destination / ('credential-temp-focused.' + field + '.bin')
        shutil.copyfile(source, target)
        assert fingerprint(target) == fingerprint(source)
        root_files[target.name] = fingerprint(source)
    focused_text = (destination / 'credential-temp-focused.stderr.bin').read_text(encoding='utf-8')
    assert re.search(r'Ran 16 tests .*\n\nOK', focused_text)
    focused_temp = {'driver_session': 95976, 'reported_terminal_exit_code': 0,
                    'observed_summary': re.findall(r'Ran .*|^OK$', focused_text, flags=re.MULTILINE),
                    'scope': 'Owner confirmed driver terminal; raw native unittest stream independently read back.'}

# Full audit inputs, CLI streams, mutated outputs and the final cumulative
# receipt. Earlier cumulative snapshots are redundant; keep their pins.
audit_members = {}
omissions = {}
for audit_root in sorted(root.glob('presentation-audit-*')):
    if not audit_root.is_dir():
        continue
    snapshots = sorted(audit_root.glob('audit-*.json'), key=lambda p: int(p.stem.split('-')[-1]))
    final_snapshot = snapshots[-1]
    for path in sorted(audit_root.iterdir()):
        if path in snapshots[:-1]:
            omissions[str(path.relative_to(root))] = fingerprint(path)
        else:
            gather(path, str(path.relative_to(root)), audit_members)
archive('presentation-audit-originals', audit_members, omissions)

# UUID-owned Rust presentation preparation/matrix/guard directories from both
# native executions. Include their external sentinel references without walking
# any other test families or build directories.
rust_members = {}
compiler = root / 'repo/target/compiler-temp'
for path in sorted(compiler.glob('presentation-*')):
    gather(path, str(path.relative_to(root)), rust_members)
    if path.is_dir():
        for manifest_path in path.glob('protection-*.json'):
            for native in load(manifest_path):
                try:
                    source = local(native)
                except ValueError:
                    continue
                if source.parent == compiler and source.name.startswith('coordination-external-sentinel-') and source.is_file():
                    gather(source, str(source.relative_to(root)), rust_members)
archive('rust-presentation-originals', rust_members)
ntfs_members = {}
for path in sorted(root.glob('ntfs-probe-*')):
    if path.is_dir():
        gather(path, str(path.relative_to(root)), ntfs_members)
archive('ntfs-originals', ntfs_members)

# The newly added Python v1/v2/v3 display-checker cases save ordinary CLI and
# full independent read-back evidence outside compiler-temp.
python_members = {}
python_omissions = {}
for path in sorted((root / 'repo/target').glob('presentation-display-python-*')):
    for item in sorted(path.iterdir()):
        if item.name == 'audit' and item.is_dir():
            snapshots = sorted(item.glob('audit-*.json'), key=lambda p: int(p.stem.split('-')[-1]))
            for child in sorted(item.iterdir()):
                if child in snapshots[:-1]:
                    python_omissions[str(child.relative_to(root))] = fingerprint(child)
                else:
                    gather(child, str(child.relative_to(root)), python_members)
        else:
            gather(item, str(item.relative_to(root)), python_members)
if python_members:
    archive('python-display-originals', python_members, python_omissions)

assert fingerprint(root / 'repo/target/debug/noetome.exe') == final['cli']
excluded[str(root / 'repo/target/debug/noetome.exe')] = {'reason': 'tested CLI executable fingerprint only', **final['cli']}
audit_final = load(root / 'presentation-audit-final.json')
audit_kinds = dict(Counter(row['kind'] for row in audit_final['scenarios']))
assert len(audit_final['scenarios']) == 39 and len(audit_final['calls']) == 124
assert audit_kinds.get('not_available_on_this_host') == 1
readback = {
    'schema_version': 1, 'task_id': 'B06-T01', 'captured_at': datetime.now(timezone.utc).isoformat(),
    'native_root': str(root), 'snapshot_head': final['head'],
    'initial_snapshot_head': load(root / 'initial-snapshot.json')['head'],
    'terminal_confirmation_required': True, 'final_native_running': load(root / 'native-running.json'),
    'required_latest_exits': final['required_latest_exits'], 'ntfs_status': ntfs['status'],
    'command_originals_rehashed': len(commands), 'ntfs_command_originals_rehashed': len(ntfs['commands']),
    'first_python_failure_preserved': {'exit_code': 1, 'summary': re.findall(r'Ran .*|FAILED.*', initial_failure)},
    'credential_temp_focused': focused_temp,
    'native_presentation_audit': {'scenario_rows': 39, 'ordinary_cli_calls': 124, 'row_kinds': audit_kinds,
                                'unavailable_rows_are_not_executed_tests': True,
                                'actual_ntfs_checks_are_separate': True},
    'native_snapshot_files_verified': len(snapshot_checked), 'current_source_files_verified': len(current_source_checked),
    'current_source_pins': current_source_checked, 'current_non_source_deltas': current_mismatch,
    'native_held_summary': load(root / 'native-held-summary.json'), 'archives': archive_results,
    'root_file_pins': root_files, 'excluded_fingerprints': excluded, 'links_not_followed': links_not_followed,
    'scope': 'Archive CRC/member SHA read-back and correspondence to completed native test evidence, not an independent scientific-quality or Obsidian claim.'}
save(destination / 'closeout-readback.json', readback)
print(json.dumps({k: readback[k] for k in ['snapshot_head', 'required_latest_exits', 'ntfs_status', 'native_snapshot_files_verified', 'current_source_files_verified', 'archives']}, ensure_ascii=False, indent=2))
