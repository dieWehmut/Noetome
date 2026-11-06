"""Rebuild the bounded matrix from native originals after independent acceptance.

This assembler checks original bytes, ownership and runtime bindings. It does not
replace the semantic collectors or independent source/geometry archive audits.
"""
from pathlib import Path, PurePosixPath
import hashlib
import json
import zipfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
RUNTIME_SHA = {
    'minimum': '88e00bc3481311959e6e13f0fdaf57cf16f13cae937058d541f1eb4a46dabfa5',
    'current': 'f7f74bbfcd029d6a9effca499fffd4e380b0b17877fcb17c448f7c573fac60cd',
}
NATIVE_SHA = {
    'minimum': '911d0e8fa33d8b086bfc7b9f0a273f421423b16f7c3fb20c17ae2d8b2d3e5d3c',
    'current': 'bd99ec64d57d5701f43d4490ff9359f4d639e90728c56cbbf15a43c05d6c158e',
}
PACKS = {
    '9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c': 'visual-compatibility',
    'e9f6a8b0e86718a38dba5cbf5073c25ffb0a6e39ca0c1d8c43a3648f4952339e': 'visual-compatibility-flow-extensions',
    'c174f543947b6e72c7ef7e786624c4d2fa870342df79abcd9d0fc7fbb90cc60d': 'visual-compatibility-corrected-v2',
}

def pin(body):
    return {'sha256': hashlib.sha256(body).hexdigest(), 'byte_length': len(body)}

def load(path):
    return json.loads(path.read_bytes())

def normalized_windows(path):
    return path.replace('/', '\\').casefold()

def current_original_row(row):
    work = Path(row['capture']['argv'][4])
    member = f'{work.parent.name}/{work.name}/result.json'
    return {**row, 'archive': 'current/native-originals.zip', 'result_member': member,
            'cleanup_member': str(PurePosixPath(member).parent / 'cleanup-result.json')}

def build():
    rows = []
    archives = {}
    archive_pins = {}
    runtimes = {}
    for role in ['minimum', 'current']:
        runtime_path = HERE / 'runtime' / f'{role}-runtime-observed.json'
        assert pin(runtime_path.read_bytes())['sha256'] == RUNTIME_SHA[role]
        runtime = load(runtime_path)
        runtimes[role] = runtime
        if role == 'minimum':
            originals = load(HERE / role / 'audit-result.json')['records']
            originals = [{**row, 'archive': 'minimum/native-originals.zip'} for row in originals]
        else:
            originals = [current_original_row(row) for row in load(HERE / role / 'native-archive-receipt.json')['selected_rows']]
        assert len(originals) == 32
        supplemental_path = (HERE / role / 'format-identity-override.json' if role == 'minimum'
                             else HERE / role / 'format-identity-supplement/override-records.json')
        supplemental = load(supplemental_path)
        assert supplemental['status'] == 'passed' and len(supplemental['records']) == 2
        final = [row for row in originals if row['case_id'] != 'hand-authored-format-probe'] + supplemental['records']
        assert len(final) == 32
        for row in final:
            archive_name = row['archive']
            if '/' not in archive_name:
                archive_name = f'{role}/{archive_name}'
            if archive_name.startswith('doc/validation/assets/B06V-T03/'):
                archive_name = archive_name.removeprefix('doc/validation/assets/B06V-T03/')
            archive_path = HERE / archive_name
            if archive_name not in archives:
                archive_pins[archive_name] = pin(archive_path.read_bytes())
                if archive_name == f'{role}/native-originals.zip':
                    assert archive_pins[archive_name]['sha256'] == NATIVE_SHA[role]
                else:
                    expected_archive_pin = row.get('archive_pin', supplemental.get('archive'))
                    assert all(archive_pins[archive_name][key] == expected_archive_pin[key]
                               for key in ['sha256', 'byte_length'])
                archive = zipfile.ZipFile(archive_path)
                assert len(archive.namelist()) == len(set(archive.namelist()))
                assert archive.testzip() is None
                archives[archive_name] = archive
            archive = archives[archive_name]
            member = row['result_member']
            prefix = PurePosixPath(member).parent
            result_body = archive.read(member)
            result = json.loads(result_body)
            if 'result_pin' in row:
                assert pin(result_body) == row['result_pin']
            input_member = row.get('input_member', str(prefix / 'visual-input.json'))
            original_input = json.loads(archive.read(input_member))
            spec = original_input['spec']
            assert result['status'] == 'passed'
            assert spec['case'] == row['case_id'] and spec['theme'] == row['theme']
            identity = result['identity']
            if 'appVersion' in identity:
                assert identity['appVersion'] == runtime['app_version']
            assert normalized_windows(identity['userData']) == normalized_windows(original_input['profile'])
            assert normalized_windows(identity['vaultPath']) == normalized_windows(original_input['vault'])
            assert identity['communityPlugins'] == []
            installed = result['compatibilityIdentity']
            assert installed['appVersion'] == installed['appPackageVersion'] == installed['obsidianPackageVersion'] == runtime['app_version']
            assert normalized_windows(installed['execPath']) == normalized_windows(runtime['executable']['windows_path'])
            assert f"theme-{row['theme']}" in installed['themeClass'].split()
            for name in ['executable', 'app_asar', 'obsidian_asar']:
                assert installed['files'][name] == {key: runtime[name][key] for key in ['sha256', 'byte_length']}
            cleanup_member = row['cleanup_member']
            cleanup = json.loads(archive.read(cleanup_member))
            assert cleanup['ownedPidAbsent'] and cleanup['ownedListenerAbsent']
            assert cleanup['profileProcessCount'] == 0 and cleanup['emergencyTerminated'] == []
            assert cleanup['ownedPort'] == original_input['port']
            modules = result['compatibilityRuntime']['modules']
            required = ['app_bundle']
            if spec['require_mathjax_runtime']:
                required.append('mathjax')
                assert result['compatibilityRuntime']['globals']['mathjaxVersion'] == runtime['mathjax']['version']
            if spec['require_mermaid_runtime']:
                required.append('mermaid')
            for name in required:
                module = modules[name]
                assert module['status'] == 'passed'
                assert module['member'] == runtime[name]['member']
                assert module['rawSha256'] == runtime[name]['member']['sha256']
                assert module['rawByteLength'] == runtime[name]['member']['byte_length']
            fixture_sha = row['fixture_sha256']
            assert row['runtime_sha256'] == RUNTIME_SHA[role]
            pack = ROOT / 'fixtures' / PACKS[fixture_sha]
            assert pin((pack / 'frozen-sha256.json').read_bytes())['sha256'] == fixture_sha
            cases = {case['case_id']: case for case in load(pack / 'cases.json')['cases']}
            case = cases[row['case_id']]
            expected_path = pack / case['expected_file']
            assert spec['format_expected'] == load(expected_path)
            for filename, original_pin in original_input['source_pins'].items():
                assert pin(archive.read(str(prefix / 'vault' / filename))) == original_pin
            rows.append({'role': role, 'app_version': runtime['app_version'], 'case_id': row['case_id'],
                         'theme': row['theme'], 'feature': case['feature'], 'representative_kind': case['representative_kind'],
                         'status': 'passed', 'fixture': f'fixtures/{PACKS[fixture_sha]}', 'fixture_sha256': fixture_sha,
                         'expected': str(expected_path.relative_to(ROOT)), 'expected_pin': pin(expected_path.read_bytes()),
                         'runtime_sha256': RUNTIME_SHA[role], 'archive': archive_name, 'result_member': member,
                         'result_pin': pin(result_body), 'input_member': input_member, 'cleanup_member': cleanup_member,
                         'actual_identity': identity, 'actual_installation': installed, 'required_modules': required,
                         'actual_runtime_modules': modules, 'source_files_checked': len(original_input['source_pins'])})
    assert len(rows) == 64 and len({(row['role'], row['case_id'], row['theme']) for row in rows}) == 64
    assert len({row['case_id'] for row in rows}) == 16
    assert sum(row['representative_kind'] == 'product_output' for row in rows) == 60
    for archive in archives.values():
        archive.close()
    return {'schema_version': 1, 'producer': 'bounded_native_visual_compatibility_matrix', 'native_status': 'passed',
            'platform': 'Windows x64', 'minimum_product_version': '1.6.7', 'current_stable_at_verification': '1.15.0',
            'current_release_verified_at': '2026-11-06', 'case_count': 16, 'product_case_count': 15,
            'hand_authored_probe_count': 1, 'themes': ['light', 'dark'], 'native_combination_count': 64,
            'runtime_pins': RUNTIME_SHA, 'archives': archive_pins, 'records': rows,
            'scope': 'Frozen representative outputs and separately marked hand-authored format probe; 4 final format records supplement original 64 without deleting old attempts',
            'scientific_quality': 'not_run', 'published': False, 'real_model_calls': 0,
            'not_run': ['macOS/Linux Obsidian native rendering', 'scientific accuracy benchmark', 'general OCR or arbitrary formulas', 'production Vault publication or manual-edit protection', '24-hour or large-project performance'],
            'assembler_limit': 'Semantic acceptance requires the original collectors plus separate independent source/geometry/code audits; this script rebuilds runtime/ownership/byte bindings only'}

if __name__ == '__main__':
    matrix = build()
    output = HERE / 'matrix.json'
    output.write_text(json.dumps(matrix, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'native_combinations': matrix['native_combination_count'], 'case_count': matrix['case_count'], 'matrix': pin(output.read_bytes())}))
