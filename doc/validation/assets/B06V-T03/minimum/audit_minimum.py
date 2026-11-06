"""Read original minimum-version captures and byte-check their final selection.

--build makes new archives from the explicitly owned capture/source directories.
Ordinary execution verifies the archives and all selected original records.
No application is launched and no model, credential or production Vault is read.
"""
import argparse
import datetime
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import zipfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
WORK = Path('/mnt/d/Noetome/.artifacts')
STAGES = ('a', 'b', 'c', 'd', 'e', 'f', 'g')
SOURCE_STAGES = ('a', 'c', 'd', 'e', 'g')
FINAL_TOOL_SHA = 'b7c97beceb536a437f5fa76b542b6299b47feb93d715d1a1430b60c3b935409f'
RUNTIME_SHA = '88e00bc3481311959e6e13f0fdaf57cf16f13cae937058d541f1eb4a46dabfa5'


def require(value, message):
    if not value:
        raise AssertionError(message)


def pin(raw):
    return {'sha256': hashlib.sha256(raw).hexdigest(), 'byte_length': len(raw)}


def read(path):
    return json.loads(path.read_bytes())


def stage_root(stage):
    return WORK / ('b06v-t03-minimum-20261106-' + stage)


def source_root(stage):
    return REPO / '.artifacts' / ('b06v-t03-minimum-capture-20261106-' + stage)


def included_capture_file(relative):
    # Keep actual profile configuration. Exclude only the owned browser caches.
    if 'profile' in relative.parts:
        after = relative.parts[relative.parts.index('profile') + 1:]
        return len(after) == 1 and after[0] in ('obsidian.json', 'Preferences', 'Local State')
    return '__pycache__' not in relative.parts and relative.suffix != '.pyc'


def archive(path, entries):
    require(not path.exists(), 'archive already exists; original archives must not be overwritten')
    manifest = {}
    with zipfile.ZipFile(path, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as out:
        for name, origin in entries:
            require(name not in manifest, 'duplicate archive member')
            raw = origin.read_bytes() if isinstance(origin, Path) else origin
            out.writestr(name, raw)
            manifest[name] = {**pin(raw), 'source': str(origin) if isinstance(origin, Path) else 'exact installed ASAR member'}
    receipt = {'schema_version': 1, 'archive': path.name, **pin(path.read_bytes()), 'members': manifest,
               'excluded': 'owned profile browser caches and Python bytecode only; all capture logs, scripts, JSON, DOM, PNG, prepared originals and Vault configuration retained'}
    (HERE / (path.stem + '-manifest.json')).write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + '\n')
    verify_archive(path)


def verify_archive(path):
    receipt = read(HERE / (path.stem + '-manifest.json'))
    require(pin(path.read_bytes()) == {k: receipt[k] for k in ('sha256', 'byte_length')}, 'archive fingerprint')
    with zipfile.ZipFile(path) as z:
        require(len(z.namelist()) == len(set(z.namelist())), 'unique archive members')
        require(set(z.namelist()) == set(receipt['members']), 'complete archive membership')
        require(z.testzip() is None, 'complete ZIP CRC readback')
        for name, expected in receipt['members'].items():
            require(pin(z.read(name)) == {k: expected[k] for k in ('sha256', 'byte_length')}, 'raw archive member fingerprint: ' + name)
    return {k: receipt[k] for k in ('archive', 'sha256', 'byte_length')} | {'member_count': len(receipt['members'])}


def final_module():
    path = source_root('g') / 'repo/scripts/prepare_visual_compatibility.py'
    require(pin(path.read_bytes())['sha256'] == FINAL_TOOL_SHA, 'final frozen preparation tool')
    spec = importlib.util.spec_from_file_location('minimum_final_preparation', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_archives(module, runtime):
    entries = []
    for stage in STAGES:
        root = stage_root(stage)
        for path in sorted(root.rglob('*')):
            if path.is_file() and included_capture_file(path.relative_to(root)):
                entries.append((stage + '/' + path.relative_to(root).as_posix(), path))
    archive(HERE / 'native-originals.zip', entries)
    entries = []
    for stage in SOURCE_STAGES:
        root = source_root(stage)
        for path in sorted(root.rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                entries.append((stage + '/' + path.relative_to(root).as_posix(), path))
    entries.append(('runtime/minimum-runtime-observed.json', HERE.parent / 'runtime/minimum-runtime-observed.json'))
    installed = Path('/mnt/c/Program Files/Obsidian/resources/obsidian.asar')
    raw = installed.read_bytes()
    require(pin(raw) == {k: runtime['obsidian_asar'][k] for k in ('sha256', 'byte_length')}, 'installed ASAR after all actual captures')
    for key in ('mathjax', 'mermaid', 'app_bundle'):
        expected = runtime[key]['member']
        data = module.member(raw, expected['path'])
        require(pin(data) == {k: expected[k] for k in ('sha256', 'byte_length')}, 'installed raw module fingerprint')
        entries.append(('runtime/installed-members/' + expected['path'], data))
    archive(HERE / 'source-originals.zip', entries)


def source_checks():
    checked = 0
    for stage in SOURCE_STAGES:
        root = source_root(stage)
        snapshot = read(root / 'source-snapshot.json')
        entries = snapshot['files'] if 'files' in snapshot else {'repo/' + name: value for name, value in snapshot['sources'].items()}
        for name, expected in entries.items():
            if '__pycache__' in Path(name).parts or Path(name).suffix == '.pyc':
                continue
            path = root / name
            require(pin(path.read_bytes()) == {k: expected[k] for k in ('sha256', 'byte_length')}, 'frozen source unchanged: ' + str(path))
            checked += 1
    return checked


def selected_record(case, theme, module, runtime, archived_native, installed_members):
    name, kind = case['case_id'], case['verification']['kind']
    stage = 'g' if kind in ('format', 'image') else 'f' if name in ('scan-sign', 'scan-subscript') else 'c'
    root = stage_root(stage)
    tag = name + '-' + theme
    capture, prepared = root / 'captures' / tag, root / 'prepared' / tag
    execution = read(root / (tag + '-execution') / 'result.json')
    prep, spec, actual = read(prepared / 'prepared.json'), read(capture / 'capture-spec.json'), read(capture / 'result.json')
    require(execution['status'] == actual['status'] == 'passed', 'actual complete capture passed: ' + tag)
    require(prep['status'] == 'prepared' and prep['source_unchanged'], 'independent input preparation and preservation')
    require(execution['model_calls'] == prep['model_requests'] == 0, 'no new model calls')
    require(execution['runtime_sha256'] == prep['runtime_sha256'] == RUNTIME_SHA, 'independent runtime input anchor')
    require(all(c['exit_code'] == 0 for c in execution['commands']), 'real prepare and native command exit codes')
    require(read(capture / 'capture-receipt.json')['exit_code'] == 0, 'actual Node native exit')
    require(pin((prepared / 'capture-spec.json').read_bytes()) == prep['capture_spec'], 'prepared spec pin')
    require((capture / 'capture-spec.json').read_bytes() == (prepared / 'capture-spec.json').read_bytes(), 'launcher uses exact prepared spec')
    script = (prepared / 'capture_visual.mjs').read_bytes()
    if kind == 'format':
        final = module.FORMAT_COLLECTOR.replace('__RUNTIME_HELPERS__', module.RUNTIME_HELPERS).replace('__FORMAT_TEXT__', module.FORMAT_TEXT).encode('utf8')
    else:
        final = module.derived_collector(module.P.read(module.COLLECTORS[kind]), kind)
    require(script == final == (capture / 'capture-script.mjs').read_bytes(), 'final frozen derived script byte identity: ' + tag)
    require(pin(script) == prep['derived_collector'], 'derived script fingerprint')
    source_pins = prep['source_pins']
    for name, expected in source_pins.items():
        require(pin((prepared / 'selected-output' / name).read_bytes()) == expected, 'original selected output byte preservation')
        if name != '.owner.json':
            require(pin((capture / 'vault' / name).read_bytes()) == expected, 'copied Vault output byte preservation')
    protection = read(capture / 'protection-cleanup.json')
    require(all(protection[k] is True for k in ('selected_originals_unchanged', 'copied_originals_unchanged', 'capture_spec_unchanged')), 'actual protected original/spec checks')
    cleanup = read(capture / 'cleanup-result.json')
    require(cleanup == protection['cleanup'], 'actual cleanup original and protection receipt agree')
    require(cleanup['profileProcessCount'] == 0 and cleanup['ownedPidAbsent'] and cleanup['ownedListenerAbsent'] and not cleanup['emergencyTerminated'], 'normal owned PID/profile/port cleanup')
    launch = read(capture / 'launch-receipt.json')
    require(launch['pid'] == cleanup['ownedPid'] and launch['port'] == cleanup['ownedPort'], 'cleanup refers to actual launched PID and port')
    identity = actual['compatibilityIdentity']
    require(identity['appVersion'] == identity['appPackageVersion'] == identity['obsidianPackageVersion'] == '1.6.7', 'actual installed application and bundle versions')
    require('theme-' + theme in identity['themeClass'].split(), 'actual selected theme')
    norm = lambda value: value.replace('\\', '/').lower()
    require(norm(identity['execPath']) == norm(runtime['executable']['windows_path']), 'actual explicitly selected executable path')
    for key in ('executable', 'app_asar', 'obsidian_asar'):
        require(identity['files'][key] == {k: runtime[key][k] for k in ('sha256', 'byte_length')}, 'actual executable/ASAR file pin')
    observed = actual['compatibilityRuntime']
    for key, needed in [('mathjax', spec['require_mathjax_runtime']), ('mermaid', spec['require_mermaid_runtime']), ('app_bundle', True)]:
        value = observed['modules'][key]
        if needed:
            require(value['status'] == 'passed' and value['member'] == runtime[key]['member'], 'actual required CDP executed member')
            suffix = value['sourceUrlComment'].encode('utf8')
            require(value['sourceUrlComment'] in ('', '\n//# sourceURL=' + value['url'], '\n//# sourceURL=' + value['url'] + '\n'), 'only an exact explicit sourceURL suffix is allowed')
            require(value['rawByteLength'] == value['member']['byte_length'] + len(suffix), 'exact raw member and explicit sourceURL suffix length')
            require(value['rawSha256'] == pin(installed_members[key] + suffix)['sha256'], 'actual complete executed source SHA reconstructed from authenticated raw member and explicit sourceURL suffix')
            if key == 'mathjax':
                require(observed['globals']['mathjaxVersion'] == value['observedVersion'] == '3.2.2', 'actual MathJax global version')
            if key == 'mermaid':
                require(value['declaredVersion'] == '10.6.1' and value['versionBinding']['version'] == '10.6.1', 'Mermaid original source literal and executed script binding')
        else:
            require(value['status'] == 'not_loaded_or_not_required', 'explicit unused module status')
    for absent in spec.get('blocked_absent', []):
        require(not (capture / 'vault' / absent).exists(), 'critical unknown output remains absent')
    clicks = actual.get('clicks', [])
    expected_attachments = {target for note in spec.get('notes', []) if isinstance(note, dict) for target in note.get('click_attachments', [])}
    for target in expected_attachments:
        dimensions = spec['attachment_images'][target]
        matching = [c for c in clicks if c.get('target') == target and c.get('actual_click') is True]
        require(len(matching) == 1, 'exact actual attachment click')
        image = matching[0]['actual']['image']
        require(image['status'] == 'passed' and image['actual']['activeFile'] == target and image['actual']['viewType'] == 'image' and image['actual']['complete'], 'native clicked local image view')
        require((image['actual']['naturalWidth'], image['actual']['naturalHeight']) == (dimensions['width'], dimensions['height']), 'actual attachment natural dimensions')
    if kind == 'format':
        expected = spec['format_expected']['semantic_source']['frontmatter']
        properties = {p['key']: p for p in actual['notes'][0]['actual']['properties']}
        require(properties['title']['titleValue'] == expected['title'] and properties['reviewed']['checkboxChecked'] == expected['reviewed'] and properties['aliases']['chips'] == expected['aliases'], 'actual visible Properties editor, checkbox and alias chips')
        require(len(clicks) == 2 and all(c['actualClick'] and c['status'] == 'passed' and c['finalObservation']['visible'] for c in clicks), 'actual heading and block clicks with visible destination')
    prefixes = [stage + '/captures/' + tag + '/', stage + '/prepared/' + tag + '/', stage + '/' + tag + '-execution/']
    for member_name, original in archived_native.items():
        if any(member_name.startswith(prefix) for prefix in prefixes):
            require(pin(Path(original['source']).read_bytes()) == {k: original[k] for k in ('sha256', 'byte_length')}, 'selected raw original remains identical to fully authenticated archive: ' + member_name)
    return {'case_id': case['case_id'], 'theme': theme, 'feature': case['feature'], 'representative_kind': case['representative_kind'], 'status': 'passed', 'stage': stage,
            'fixture_sha256': prep['fixture_sha256'], 'runtime_sha256': prep['runtime_sha256'], 'derived_collector': pin(script),
            'result_member': stage + '/captures/' + tag + '/result.json', 'result_pin': pin((capture / 'result.json').read_bytes()),
            'cleanup_member': stage + '/captures/' + tag + '/cleanup-result.json', 'pid': launch['pid'], 'port': launch['port'],
            'source_file_count': len(source_pins), 'actual_click_count': len(clicks), 'actual_attachment_click_count': len(expected_attachments),
            'screenshot_count': len(list(capture.glob('*.png'))), 'required_mathjax': spec['require_mathjax_runtime'], 'required_mermaid': spec['require_mermaid_runtime']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--build', action='store_true')
    args = parser.parse_args()
    module = final_module()
    runtime_path = HERE.parent / 'runtime/minimum-runtime-observed.json'
    require(pin(runtime_path.read_bytes())['sha256'] == RUNTIME_SHA, 'observed runtime record anchor')
    runtime = read(runtime_path)
    if args.build:
        build_archives(module, runtime)
    archives = [verify_archive(HERE / name) for name in ('native-originals.zip', 'source-originals.zip')]
    sources = source_checks()
    archived_native = read(HERE / 'native-originals-manifest.json')['members']
    with zipfile.ZipFile(HERE / 'source-originals.zip') as z:
        installed_members = {key: z.read('runtime/installed-members/' + runtime[key]['member']['path']) for key in ('mathjax', 'mermaid', 'app_bundle')}
    cases = read(REPO / 'fixtures/visual-compatibility/cases.json')['cases'] + read(REPO / 'fixtures/visual-compatibility-flow-extensions/cases.json')['cases']
    records = [selected_record(case, theme, module, runtime, archived_native, installed_members) for case in cases for theme in ('light', 'dark')]
    require(len(records) == len({(r['case_id'], r['theme']) for r in records}) == 32, 'complete unique 16-by-2 minimum matrix')
    attempts = []
    for stage in STAGES:
        for capture in sorted((stage_root(stage) / 'captures').glob('*')):
            if not capture.is_dir():
                continue
            receipt = read(capture / 'capture-receipt.json')
            attempts.append({'stage': stage, 'case_theme': capture.name, 'native_exit_code': receipt['exit_code'], 'actual_result_status': read(capture / 'result.json').get('status') if (capture / 'result.json').exists() else 'no_result'})
    summary = {'schema_version': 1, 'producer': 'independent_minimum_original_record_audit', 'status': 'passed', 'checked_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'runtime': {'app_version': '1.6.7', 'mathjax_version': '3.2.2', 'mermaid_version': '10.6.1', 'mermaid_runtime_exposed_version': None},
               'selected_matrix': {'distinct_cases': 16, 'product_cases': 15, 'hand_authored_reader_probes': 1, 'themes': ['light', 'dark'], 'passed_captures': 32},
               'actual_attempts': attempts, 'actual_native_attempt_count': len(attempts), 'actual_native_success_count': sum(a['native_exit_code'] == 0 for a in attempts),
               'actual_native_failed_count': sum(a['native_exit_code'] != 0 for a in attempts), 'preparation_only_failures': ['e/scan-sign-light-execution/result.json'],
               'archive_checks': archives, 'frozen_source_files_checked': sources, 'records': records, 'actual_attachment_clicks': sum(r['actual_attachment_click_count'] for r in records),
               'actual_total_clicks': sum(r['actual_click_count'] for r in records), 'model_calls': 0, 'scientific_quality': 'not_run', 'published': False,
               'limits': ['Windows 1.6.7 only; current stable owned by separate stream', 'new explicitly owned Vault/profile per attempt; no production Vault writes', 'freeze/output bytes do not establish scientific accuracy or general OCR quality']}
    (HERE / 'audit-result.json').write_text(json.dumps(summary, indent=2, ensure_ascii=False) + '\n')
    print(json.dumps({k: summary[k] for k in ('status', 'selected_matrix', 'actual_native_attempt_count', 'actual_native_success_count', 'actual_native_failed_count', 'actual_attachment_clicks', 'actual_total_clicks', 'frozen_source_files_checked', 'archive_checks')}, ensure_ascii=False))


if __name__ == '__main__':
    main()
