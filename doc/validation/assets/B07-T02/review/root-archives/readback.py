"""Read closed public original ZIPs, source bindings and complete program bodies."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import io
import json
from pathlib import Path
import re
import zipfile
import zlib

REPO = Path('/home/lyinn/project/Noetome')
ROOT = REPO / 'doc/validation/assets/B07-T02'
OUT = Path(__file__).parent
KEYS = ('sha256', 'byte_length')


def pin_bytes(raw):
    return {'sha256': hashlib.sha256(raw).hexdigest(), 'byte_length': len(raw)}


def pin(path):
    h = hashlib.sha256()
    size = 0
    with path.open('rb') as stream:
        for raw in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(raw)
            size += len(raw)
    return {'sha256': h.hexdigest(), 'byte_length': size}


def norm(value):
    return {key: value[key] for key in KEYS}


def catalog_pin(value):
    return pin_bytes(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def save(name, value):
    (OUT / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf8')


def manifest_items(raw):
    value = json.loads(raw)
    if isinstance(value.get('members'), list):
        items = value['members']
        assert len(items) == len({row['member'] for row in items})
        return {row['member']: norm(row['pin']) for row in items}
    return {name: norm(item) for name, item in value.items()}


def read_member(stream, info):
    h = hashlib.sha256()
    length = crc = 0
    prefix = b''
    for raw in iter(lambda: stream.read(1024 * 1024), b''):
        if len(prefix) < 4:
            prefix = (prefix + raw)[:4]
        h.update(raw)
        length += len(raw)
        crc = zlib.crc32(raw, crc)
    assert length == info.file_size and f'{crc & 0xffffffff:08x}' == f'{info.CRC:08x}', info.filename
    return {'sha256': h.hexdigest(), 'byte_length': length}, prefix


def audit_archive(label):
    base = ROOT / label
    receipt_name = 'actual.json' if label == 'linux-run' else 'receipt.json'
    receipt_path = base / receipt_name
    manifest_path = base / 'manifest.json'
    archive_path = base / 'originals.zip'
    receipt_before, manifest_before = pin(receipt_path), pin(manifest_path)
    receipt = json.loads(receipt_path.read_bytes())
    expected = manifest_items(manifest_path.read_bytes())
    expected_archive = norm(receipt['archive'])
    assert pin(archive_path) == expected_archive
    actual = {}
    size = 0
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        assert len(infos) == len({info.filename for info in infos}) == len(expected) == receipt['members']
        assert {info.filename for info in infos} == set(expected)
        for info in infos:
            assert not info.is_dir() and not info.filename.startswith('/')
            assert all(part not in {'', '.', '..'} for part in info.filename.split('/'))
            with archive.open(info) as stream:
                actual[info.filename], _ = read_member(stream, info)
            assert actual[info.filename] == expected[info.filename], (label, info.filename)
            size += actual[info.filename]['byte_length']
    assert actual == expected and pin(archive_path) == expected_archive
    assert pin(receipt_path) == receipt_before and pin(manifest_path) == manifest_before
    return {'label': label, 'archive': expected_archive, 'receipt': receipt_before,
            'manifest': manifest_before, 'member_count': len(actual), 'original_byte_length': size,
            'all_members_crc_sha_length': True, 'manifest_exact_named_inventory': True,
            'actual_member_catalog': catalog_pin(actual),
            'scope': receipt.get('scope', 'original archive preservation only')}


def audit_programs():
    base = ROOT / 'programs'
    manifest_path = base / 'manifest.json'
    before = pin(manifest_path)
    expected = json.loads(manifest_path.read_bytes())
    assert len(expected) == 6
    results = {}
    with zipfile.ZipFile(ROOT / 'diagnostic-originals/originals.zip') as diagnostic:
        for label, descriptor in expected.items():
            archive_path = base / (label + '.zip')
            assert pin(archive_path) == descriptor['archive']
            suffix = 'request-test-driver' if label.endswith('request-test-driver') else 'noetome'
            member = suffix
            pair = re.search(r'linux-pair-(\d+)-', label)[1]
            diagnostic_name = f'b07-t02-linux-pair-{pair}-20261107/actual.json'
            pair_raw = diagnostic.read(diagnostic_name)
            pair_actual = json.loads(pair_raw)
            field = 'test_driver' if suffix == 'request-test-driver' else 'cli'
            assert descriptor['original_executable'] == norm(pair_actual[field])
            assert descriptor['actual_compiler_fingerprint'] == pair_actual['actual_cli_fingerprint']
            with zipfile.ZipFile(archive_path) as archive:
                assert set(archive.namelist()) == {member, 'pair-actual.json'} and len(archive.namelist()) == 2
                assert archive.read('pair-actual.json') == pair_raw
                info = archive.getinfo(member)
                with archive.open(info) as stream:
                    actual, prefix = read_member(stream, info)
                assert actual == descriptor['original_executable'] and prefix == b'\x7fELF'
            assert pin(archive_path) == descriptor['archive']
            results[label] = {'archive': descriptor['archive'], 'whole_ELF': actual,
                              'pair_actual': pin_bytes(pair_raw), 'actual_fp': descriptor['actual_compiler_fingerprint'],
                              'member_count': 2, 'all_members_crc_sha_length': True}
    assert pin(manifest_path) == before
    return {'program_count': 6, 'manifest': before, 'receipt': pin(base / 'receipt.json'), 'programs': results}


def diagnostic_source_binding():
    result = {}
    with zipfile.ZipFile(ROOT / 'diagnostic-originals/originals.zip') as archive:
        for pair in ('02', '03', '04'):
            prefix = f'b07-t02-linux-pair-{pair}-20261107/'
            before = json.loads(archive.read(prefix + 'source-before.json'))
            after = json.loads(archive.read(prefix + 'source-after.json'))
            actual = json.loads(archive.read(prefix + 'actual.json'))
            assert before == after and len(before) == actual['source_files'] == 130
            expected = {name: norm(item) for name, item in before.items()}
            raw_zip = archive.read(prefix + 'source-originals.zip')
            assert pin_bytes(raw_zip) == norm(actual['source_archive'])
            with zipfile.ZipFile(io.BytesIO(raw_zip)) as source:
                assert len(source.namelist()) == len(set(source.namelist())) == len(expected)
                assert set(source.namelist()) == set(expected)
                for name in source.namelist():
                    assert pin_bytes(source.read(name)) == expected[name]
            probe = json.loads(archive.read(prefix + 'cli-probe.stdout.bin'))
            assert probe['compiler_fingerprint'] == actual['actual_cli_fingerprint']
            result[pair] = {'source_count': len(expected), 'source_archive': pin_bytes(raw_zip),
                            'actual_receipt': pin_bytes(archive.read(prefix + 'actual.json')),
                            'actual_fp': actual['actual_cli_fingerprint'], 'source_files': expected}
    return result


def linux_python_current(binding):
    base = ROOT / 'linux-python'
    actual_raw = (base / 'actual.json').read_bytes()
    actual = json.loads(actual_raw)
    saved_binding = json.loads((base / 'compiler-source-binding.json').read_bytes())
    assert actual['status'] == 'passed' and actual['exit_code'] == 0
    assert all(actual[key] is True for key in ('snapshot_source_unchanged', 'worktree_source_unchanged', 'named_input_inventory_unchanged'))
    assert saved_binding['current_exact_equal'] is True
    assert saved_binding['receipt_pin'] == binding['04']['actual_receipt']
    assert saved_binding['rust_cargo_sources'] == binding['04']['source_files']
    assert all(pin(REPO / name) == stamp for name, stamp in saved_binding['rust_cargo_sources'].items())
    prefix = 'linux-python-final-04/'
    with zipfile.ZipFile(base / 'originals.zip') as archive:
        assert archive.read(prefix + 'actual.json') == actual_raw
        before = json.loads(archive.read(prefix + 'source-before.json'))
        after = json.loads(archive.read(prefix + 'source-after.json'))
        assert before == after and len(before) == actual['source_files']
        work_before = json.loads(archive.read(prefix + 'worktree-source-before.json'))
        work_after = json.loads(archive.read(prefix + 'worktree-source-after.json'))
        assert work_before == work_after
        stderr = archive.read(prefix + 'stderr.bin')
        assert pin_bytes(stderr) == norm(actual['stderr'])
        match = re.search(rb'Ran (\d+) tests in ([\d.]+)s', stderr)
        assert match and int(match[1]) == actual['tests_run'] and re.search(rb'\nOK \(skipped=22\)', stderr)
        assert len(actual['skipped_tests']) == 22
        for name in ('scripts/verify_request_budget.py', 'tests/test_verify_request_budget.py'):
            assert before[name] == pin(REPO / name)
            assert pin_bytes(archive.read(prefix + 'repo/' + name)) == before[name]
        assert before['scripts/verify_request_budget.py']['sha256'] == 'eb6c4476c57b6ab24ccd71646045bb9b8386079bb415f677134bde9b7bddca34'
    return {'actual': pin_bytes(actual_raw), 'source_binding': pin(base / 'compiler-source-binding.json'),
            'tests_run': actual['tests_run'], 'passed': actual['tests_run'] - 22, 'skipped': 22,
            'source_count': len(before), 'paired_source_count': len(saved_binding['rust_cargo_sources']),
            'paired_fp': binding['04']['actual_fp'], 'raw_summary_verified': True,
            'scope': 'Actual complete selected Linux Python source; no full Linux Rust inferred.'}


def windows_history():
    base = ROOT / 'windows-history-01'
    with zipfile.ZipFile(base / 'originals.zip') as archive:
        outer = json.loads(archive.read('outer-native-result.json'))
        assert outer['exit_code'] == 1 and outer['worktree_before_after_equal'] is False and outer['control_originals_equal'] is True
        raw = archive.read('workspace-tests.stdout.bin')
        rust = json.loads(archive.read('workspace-tests.json'))
        assert rust['exit_code'] == 0 and pin_bytes(raw) == norm(rust['stdout'])
        rows = [tuple(map(int, row)) for row in re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', raw)]
        assert len(rows) == 23 and sum(row[0] for row in rows) == 789 and all(row[1] == row[2] == 0 for row in rows)
        python = json.loads(archive.read('full-python.json'))
        stderr = archive.read('full-python.stderr.bin')
        assert python['exit_code'] == 1 and pin_bytes(stderr) == norm(python['stderr'])
        match = re.search(rb'Ran (\d+) tests in ([\d.]+)s', stderr)
        assert match and int(match[1]) == 527
        assert b'FAILED (errors=2' in stderr
        build = json.loads(archive.read('selected-build-receipt.json'))
        for member, field in [('selected/noetome.exe', 'cli'), ('selected/noetome-cli-testdriver.exe', 'test_driver')]:
            raw = archive.read(member)
            assert pin_bytes(raw) == norm(build[field]) and raw[:2] == b'MZ'
        source_raw = archive.read('source-originals.zip')
        prepared = json.loads(archive.read('prepared-source.json'))
        assert pin_bytes(source_raw) == prepared['source_archive']
        snapshot = json.loads(archive.read('snapshot.json'))
        with zipfile.ZipFile(io.BytesIO(source_raw)) as source:
            names = source.namelist()
            assert len(names) == len(set(names)) == len(snapshot['files']) + 4 == 2060
            for name, stamp in snapshot['files'].items():
                assert pin_bytes(source.read('repo/' + name)) == stamp
            for name in ('snapshot.json', 'rust-dependency-subset.json', 'precise-source-scopes.json', 'runner.py'):
                assert source.read(name) == archive.read(name)
    return {'outer_exit_code': 1, 'rust_passed': 789, 'rust_summary_count': 23,
            'python_run': 527, 'python_errors': 2, 'source_member_count': 2060,
            'source_archive': pin_bytes(source_raw), 'actual_fp': build['actual_compiler_fingerprint'],
            'scope': 'Old source history only; failed full Python and original bytes retained, never marked final.'}


def full05_source(binding):
    with zipfile.ZipFile(ROOT / 'linux-run/originals.zip') as archive:
        prefix = 'b07-t02-request-full-05-20261107/'
        actual_raw = archive.read(prefix + 'actual.json')
        actual = json.loads(actual_raw)
        source = json.loads(archive.read(prefix + 'checker-source-before.json'))
        after = json.loads(archive.read(prefix + 'checker-source-after.json'))
        assert source == after
        for name, item in source.items():
            if Path(name).name == 'verify_request_budget.py':
                assert item['sha256'] == 'eb6c4476c57b6ab24ccd71646045bb9b8386079bb415f677134bde9b7bddca34'
        build = json.loads(archive.read(prefix + 'build-receipt-original.json'))
        assert build['actual_cli_fingerprint'] == binding['04']['actual_fp']
        assert actual['status'] == 'passed'
    return {'actual': pin_bytes(actual_raw),
            'checker_before_after_equal': True, 'source_fp': build['actual_cli_fingerprint'],
            'scope': 'Only archived final05 source binding checked here; independent product semantics/OSkill peer review is separate.'}


def main():
    started = datetime.now(timezone.utc).isoformat()
    labels = ['diagnostic-originals', 'linux-python-history-01', 'linux-python-history-02',
              'linux-python-history-03', 'linux-python', 'windows-history-01', 'review-history', 'linux-run']
    with ThreadPoolExecutor(max_workers=3) as pool:
        archives = list(pool.map(audit_archive, labels))
    programs = audit_programs()
    binding = diagnostic_source_binding()
    result = {'producer': 'independent_closed_root_archives_readback', 'status': 'passed_all_selected_archives',
              'started_utc': started, 'finished_utc': datetime.now(timezone.utc).isoformat(),
              'archives': archives, 'programs': programs, 'diagnostic_pair_source_binding': binding,
              'current_linux_python': linux_python_current(binding), 'old_windows_history': windows_history(),
              'current_full05_source': full05_source(binding),
              'product_commands_by_audit': 0,
              'scope': 'Archive CRC/SHA/length and original source provenance only. History failures preserved; final Native04 not yet covered; no product rerun or Linux full-Rust inference.'}
    save('actual.json', result)
    print(json.dumps({'status': result['status'], 'archive_count': len(archives) + 6,
                      'archive_member_counts': {item['label']: item['member_count'] for item in archives},
                      'linux_python': result['current_linux_python'], 'windows_history': result['old_windows_history']}, ensure_ascii=False))


if __name__ == '__main__':
    main()
