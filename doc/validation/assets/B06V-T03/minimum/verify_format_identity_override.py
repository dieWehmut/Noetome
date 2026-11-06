"""Append actual final format identities without rewriting any prior archive."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import zipfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
ROOT = Path('/mnt/d/Noetome/.artifacts/b06v-t03-minimum-20261106-h')
SOURCE = REPO / '.artifacts/b06v-t03-minimum-capture-20261106-h'
TOOL_SHA = '9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f'


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read(path):
    return json.loads(path.read_bytes())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--build', action='store_true')
    args = parser.parse_args()
    audit = load('minimum_original_audit', HERE / 'audit_minimum.py')
    tool_path = SOURCE / 'repo/scripts/prepare_visual_compatibility.py'
    assert audit.pin(tool_path.read_bytes())['sha256'] == TOOL_SHA
    tool = load('minimum_serialized_format', tool_path)
    before = load('minimum_previous_format', REPO / '.artifacts/b06v-t03-minimum-capture-20261106-g/repo/scripts/prepare_visual_compatibility.py')
    assert tool.FORMAT_COLLECTOR.replace('  result.identity=identity;\n', '', 1) == before.FORMAT_COLLECTOR
    assert tool.RUNTIME_HELPERS == before.RUNTIME_HELPERS
    case = next(c for c in read(SOURCE / 'repo/fixtures/visual-compatibility/cases.json')['cases'] if c['case_id'] == 'hand-authored-format-probe')
    for name, expected in read(SOURCE / 'source-snapshot.json')['files'].items():
        assert audit.pin((SOURCE / name).read_bytes()) == expected
    records = []
    for theme in ('light', 'dark'):
        tag = 'hand-authored-format-probe-' + theme
        capture, prepared = ROOT / 'captures' / tag, ROOT / 'prepared' / tag
        native, visual, spec = read(capture / 'result.json'), read(capture / 'visual-input.json'), read(capture / 'capture-spec.json')
        execution, prep = read(ROOT / (tag + '-execution/result.json')), read(prepared / 'prepared.json')
        assert native['status'] == execution['status'] == 'passed' and all(c['exit_code'] == 0 for c in execution['commands'])
        assert read(capture / 'capture-receipt.json')['exit_code'] == 0
        identity = native['identity']
        norm = lambda value: value.replace('\\', '/').lower()
        assert norm(identity['userData']) == norm(visual['profile']) and norm(identity['vaultPath']) == norm(visual['vault']) and identity['communityPlugins'] == []
        assert native['compatibilityIdentity']['appVersion'] == '1.6.7' and 'theme-' + theme in native['compatibilityIdentity']['themeClass'].split()
        assert prep['runtime_sha256'] == audit.RUNTIME_SHA
        assert all(native['compatibilityIdentity']['files'][key] == {k: spec['runtime'][key][k] for k in ('sha256', 'byte_length')} for key in ('executable', 'app_asar', 'obsidian_asar'))
        assert native['compatibilityRuntime']['modules']['app_bundle']['status'] == 'passed'
        assert native['compatibilityRuntime']['modules']['app_bundle']['member'] == spec['runtime']['app_bundle']['member']
        script = tool.FORMAT_COLLECTOR.replace('__RUNTIME_HELPERS__', tool.RUNTIME_HELPERS).replace('__FORMAT_TEXT__', tool.FORMAT_TEXT).encode('utf8')
        assert script == (prepared / 'capture_visual.mjs').read_bytes() == (capture / 'capture-script.mjs').read_bytes()
        assert audit.pin(script) == prep['derived_collector'] and audit.pin((prepared / 'capture-spec.json').read_bytes()) == prep['capture_spec']
        assert (prepared / 'capture-spec.json').read_bytes() == (capture / 'capture-spec.json').read_bytes()
        for name, expected in prep['source_pins'].items():
            assert audit.pin((prepared / 'selected-output' / name).read_bytes()) == audit.pin((capture / 'vault' / name).read_bytes()) == expected
        protected, cleanup = read(capture / 'protection-cleanup.json'), read(capture / 'cleanup-result.json')
        assert all(protected[k] for k in ('selected_originals_unchanged', 'copied_originals_unchanged', 'capture_spec_unchanged'))
        assert cleanup == protected['cleanup'] and cleanup['profileProcessCount'] == 0 and cleanup['ownedPidAbsent'] and cleanup['ownedListenerAbsent'] and not cleanup['emergencyTerminated']
        assert len(native['clicks']) == 2 and all(c['status'] == 'passed' and c['actualClick'] and c['finalObservation']['visible'] for c in native['clicks'])
        actual = native['notes'][0]['actual']
        expected = spec['format_expected']['semantic_source']['frontmatter']
        properties = {p['key']: p for p in actual['properties']}
        assert properties['title']['titleValue'] == expected['title'] and properties['reviewed']['checkboxChecked'] == expected['reviewed'] and properties['aliases']['chips'] == expected['aliases']
        assert execution['model_calls'] == prep['model_requests'] == 0
        launch = read(capture / 'launch-receipt.json')
        records.append({'case_id': 'hand-authored-format-probe', 'theme': theme, 'feature': case['feature'], 'representative_kind': case['representative_kind'], 'status': 'passed', 'stage': 'h',
                        'archive': 'format-identity-originals.zip', 'result_member': 'h/captures/' + tag + '/result.json', 'result_pin': audit.pin((capture / 'result.json').read_bytes()),
                        'cleanup_member': 'h/captures/' + tag + '/cleanup-result.json', 'cleanup_pin': audit.pin((capture / 'cleanup-result.json').read_bytes()),
                        'input_member': 'h/captures/' + tag + '/visual-input.json', 'input_pin': audit.pin((capture / 'visual-input.json').read_bytes()),
                        'identity': identity, 'compatibilityIdentity': native['compatibilityIdentity'], 'derived_collector': audit.pin(script),
                        'fixture_sha256': prep['fixture_sha256'], 'runtime_sha256': prep['runtime_sha256'], 'actual_click_count': 2, 'actual_attachment_click_count': 0,
                        'pid': launch['pid'], 'port': launch['port'], 'source_file_count': len(prep['source_pins']), 'screenshot_count': len(list(capture.glob('*.png'))),
                        'required_mathjax': spec['require_mathjax_runtime'], 'required_mermaid': spec['require_mermaid_runtime'],
                        'raw_identity_serialized': True, 'previous_success_retained': 'native-originals.zip:g/captures/' + tag + '/result.json'})
    if args.build:
        entries = []
        for path in sorted(ROOT.rglob('*')):
            if path.is_file() and audit.included_capture_file(path.relative_to(ROOT)):
                entries.append(('h/' + path.relative_to(ROOT).as_posix(), path))
        for path in sorted(SOURCE.rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                entries.append(('source-h/' + path.relative_to(SOURCE).as_posix(), path))
        audit.archive(HERE / 'format-identity-originals.zip', entries)
    archive = audit.verify_archive(HERE / 'format-identity-originals.zip')
    (HERE / 'format-identity-override.json').write_text(json.dumps({'status': 'passed', 'producer': 'minimum_format_raw_identity_override', 'records': records, 'archive': archive, 'model_calls': 0, 'old_originals_preserved': True}, indent=2, ensure_ascii=False) + '\n')
    previous = read(HERE / 'audit-result.json')
    assert previous['status'] == 'passed' and previous['selected_matrix']['passed_captures'] == 32
    final = []
    with zipfile.ZipFile(HERE / 'native-originals.zip') as z:
        for record in previous['records']:
            if record['case_id'] == 'hand-authored-format-probe':
                final.append(next(r for r in records if r['theme'] == record['theme']))
                continue
            native = json.loads(z.read(record['result_member']))
            assert audit.pin(z.read(record['result_member'])) == record['result_pin'] and native['status'] == 'passed'
            assert native['identity']['communityPlugins'] == []
            final.append({**record, 'archive': 'native-originals.zip', 'identity': native['identity'], 'compatibilityIdentity': native['compatibilityIdentity']})
    assert len(final) == len({(r['case_id'], r['theme']) for r in final}) == 32
    assert sum(r['actual_click_count'] for r in final) == 94 and sum(r['actual_attachment_click_count'] for r in final) == 18
    result = {'status': 'passed', 'producer': 'minimum_final_identity_override_index', 'final_selected_captures': 32, 'product_cases': 15, 'hand_authored_reader_probe_cases': 1,
              'themes': ['light', 'dark'], 'original_selected_archive': previous['archive_checks'], 'format_override_archive': archive,
              'final_format_preparation_sha256': TOOL_SHA, 'only_format_change': 'serialize previously asserted raw identity; semantic/UI/link gates byte unchanged',
              'actual_native_attempts_including_history': 44, 'actual_native_passed_including_superseded': 40, 'actual_native_failed_preserved': 4, 'preparation_only_failures_preserved': 1,
              'records': final, 'actual_attachment_clicks': 18, 'actual_total_clicks': 94, 'model_calls': 0,
              'source_reuse_scope': 'final-collector-scope.json verifies current-only image ink additions; old images retain their original executed b7 script identity',
              'limits': 'Windows minimum1.6.7 only; scientific quality not_run; production Vault not written; no old archive rewritten'}
    (HERE / 'final-index.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    print(json.dumps({k: result[k] for k in ('status', 'final_selected_captures', 'format_override_archive', 'actual_native_attempts_including_history', 'actual_native_passed_including_superseded', 'actual_native_failed_preserved', 'actual_attachment_clicks', 'actual_total_clicks')}))


if __name__ == '__main__':
    main()
