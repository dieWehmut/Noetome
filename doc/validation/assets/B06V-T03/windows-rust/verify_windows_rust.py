"""Verify only the explicitly frozen Windows regression roots, then archive.

The source snapshot, exact commands, native runtime, stdout/stderr, byte guards
and selected real CLI are checked. This does not run Rust or call a model.
"""
import argparse
import hashlib
import json
from pathlib import Path, PureWindowsPath
import re
import zipfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
BASE = Path('/mnt/d/Noetome/target')


def pin(raw):
    return {'sha256': hashlib.sha256(raw).hexdigest(), 'byte_length': len(raw)}


def read(path):
    return json.loads(path.read_bytes())


def root(stage):
    return BASE / ('b06v-t03-final-rust-20261106-' + stage)


def build_archive(name, entries):
    path = HERE / name
    assert not path.exists(), 'original archive must never be overwritten'
    members = {}
    with zipfile.ZipFile(path, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as out:
        for name, source in entries:
            raw = source.read_bytes()
            assert name not in members
            out.writestr(name, raw)
            members[name] = {'source': str(source), **pin(raw)}
    (HERE / (path.stem + '-manifest.json')).write_text(json.dumps({'archive': path.name, **pin(path.read_bytes()), 'members': members}, indent=2) + '\n')


def verify_archive(name):
    path = HERE / name
    manifest = read(HERE / (path.stem + '-manifest.json'))
    assert pin(path.read_bytes()) == {k: manifest[k] for k in ('sha256', 'byte_length')}
    with zipfile.ZipFile(path) as z:
        assert len(z.namelist()) == len(set(z.namelist())) == len(manifest['members'])
        assert set(z.namelist()) == set(manifest['members']) and z.testzip() is None
        for name, item in manifest['members'].items():
            assert pin(z.read(name)) == {k: item[k] for k in ('sha256', 'byte_length')}
    return {k: manifest[k] for k in ('archive', 'sha256', 'byte_length')} | {'member_count': len(manifest['members'])}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--build', action='store_true')
    args = parser.parse_args()
    source_counts = {}
    for stage in ('a', 'b', 'c'):
        snapshot = read(root(stage) / 'snapshot.json')
        for name, expected in snapshot['files'].items():
            assert pin((root(stage) / 'repo' / name).read_bytes()) == expected
        source_counts[stage] = len(snapshot['files'])
    final_root = root('c')
    snapshot = read(final_root / 'snapshot.json')
    current_changes = [name for name, expected in snapshot['files'].items() if pin((REPO / name).read_bytes()) != expected]
    current_names = set()
    for top in ('crates', 'apps', 'fixtures', 'scripts', 'sidecars', 'tests', 'schemas'):
        for path in (REPO / top).rglob('*'):
            relative = path.relative_to(REPO)
            if path.is_file() and not any(part in ('__pycache__', 'node_modules', 'target', 'dist', '.venv') for part in relative.parts) and path.suffix != '.pyc':
                current_names.add(relative.as_posix())
    additions = sorted(current_names - set(snapshot['files']))
    assert not any(name.startswith(('crates/', 'apps/desktop/src-tauri/', 'sidecars/', 'fixtures/')) for name in additions), 'new Rust dependency was not in tested snapshot'
    subset = read(final_root / 'rust-dependency-subset.json')['files']
    rust_changes = [name for name, expected in subset.items() if pin((REPO / name).read_bytes()) != expected]
    assert not rust_changes, 'final current Rust source does not match actual Windows tested source'
    final = read(final_root / 'native-rust-final.json')
    assert final['source_files'] == snapshot['files'] and final['source_before_after_equal'] and final['external_sentinel_before_after_equal']
    assert all(code == 0 for code in final['required_latest_exits'].values())
    assert final['rust_totals']['failed'] == final['rust_totals']['ignored'] == 0
    assert pin((final_root / 'noetome-selected.exe').read_bytes()) == final['cli']
    assert final['selected_cli']['nlink'] == 1 and final['real_api_calls'] == final['real_model_calls'] == final['paid_calls'] == 0
    commands = read(final_root / 'native-commands.json')
    for command in commands:
        assert command['exit_code'] == 0 and not command['timed_out'] and command['source_before_after_equal'] and command['external_sentinel_before_after_equal']
        for stream in ('stdout', 'stderr'):
            path = final_root / PureWindowsPath(command[stream]['path']).name
            assert pin(path.read_bytes()) == {k: command[stream][k] for k in ('sha256', 'byte_length')}
    unit_copy = read(final_root / 'unit-test-cli-copy.json')
    assert unit_copy['copied_bytes_equal'] and pin((final_root / 'repo/target/debug/noetome.exe').read_bytes()) == unit_copy['fingerprint']
    text = (final_root / 'workspace-tests-attempt-1.stdout.bin').read_text(encoding='utf8')
    summaries = [tuple(map(int, v)) for v in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)]
    assert summaries == [tuple(v) for v in final['rust_suite_summaries']]
    observed = (final_root / 'workspace-tests-attempt-1.stderr.bin').read_text(encoding='utf8')
    assert all('Observed live raw sharing violation; mode=' + str(i) + ', outcome=' in observed for i in range(4))
    assert read(root('a') / 'workspace-tests-attempt-1.json')['exit_code'] == 101
    assert read(root('a') / 'focused-race-diagnostic.json')['status'] == 'passed'
    assert read(root('b') / 'fixed-cache-result.json')['status'] == 'passed'
    assert source_counts == {'a': 1915, 'b': 1915, 'c': 1915}
    if args.build:
        native, source = [], []
        for stage in ('a', 'b', 'c'):
            for path in sorted(root(stage).iterdir()):
                if path.is_file():
                    native.append((stage + '/' + path.name, path))
            fallback = root(stage) / 'repo/target/debug/noetome.exe'
            if fallback.is_file():
                native.append((stage + '/unit-test-fallback-noetome.exe', fallback))
            for name in read(root(stage) / 'snapshot.json')['files']:
                source.append((stage + '/repo/' + name, root(stage) / 'repo' / name))
            source.append((stage + '/snapshot.json', root(stage) / 'snapshot.json'))
            source.append((stage + '/rust-dependency-subset.json', root(stage) / 'rust-dependency-subset.json'))
        build_archive('native-originals.zip', native)
        build_archive('source-originals.zip', source)
    archives = [verify_archive(name) for name in ('native-originals.zip', 'source-originals.zip')]
    result = {'status': 'passed', 'producer': 'independent_exact_windows_rust_originals_audit', 'selected_final_stage': 'c',
              'actual_rust_totals': final['rust_totals'], 'actual_suite_summaries': summaries, 'source_files_per_stage': source_counts,
              'final_rust_dependency_files': len(subset), 'current_rust_source_exactly_matches_tested_snapshot': True,
              'current_non_rust_snapshot_changes': current_changes, 'current_non_rust_snapshot_additions': additions, 'final_cli': final['cli'],
              'actual_raw_cache_guard_modes': [0, 1, 2, 3], 'old_full_failure_preserved': True,
              'failed_stage': {'a': '467 CLI unit tests passed; one observer did not see a live sharing guard; full exit101'},
              'diagnostic_stage': {'a': 'three selected exact tests actually observed mode0 and passed; not a full pass', 'b': 'after readiness fix, all four cache tests passed at four threads'},
              'archives': archives, 'real_api_calls': 0, 'real_model_calls': 0, 'paid_calls': 0,
              'limits': ['Windows Rust only; two-platform Python and five independent visual audits are separate', 'new owned snapshot/temp/sentinel; no install or production Vault operation', 'runtime-temp compiler/test scratch omitted; commands, source snapshots, guard/sentinel receipts, original logs and selected PE bytes retained']}
    (HERE / 'audit-result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
