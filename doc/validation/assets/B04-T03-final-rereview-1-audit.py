"""Scoped R1 evidence audit. No product import, CLI/test execution or extraction."""
import ast
import base64
import hashlib
import json
import re
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

REPO = Path('D:/Noetome')
SDK = REPO / '.superpowers/sdd/B04-T03_REVIEW_PLAN'
ASSETS = REPO / 'doc/validation/assets'

def fp(raw):
    return {'sha256': hashlib.sha256(raw).hexdigest(), 'byte_length': len(raw)}

def read_json(path):
    return json.loads(path.read_bytes())

evidence = {'schema_version': 1, 'created_utc': datetime.now(timezone.utc).isoformat(),
            'scope': 'R1 and fix-introduced breakage only',
            'new_test_runs': 0, 'new_cli_calls': 0, 'new_model_calls': 0,
            'new_network_calls': 0, 'git_commands': 0, 'checks': [], 'files': {}}
package = read_json(SDK / 'final-fix-1-rereview-package.json')
assert package['base'] == 'cccd784f607d506d0b6c8b0a12aea7973fadeb57'
assert package['head'] == '02e8ab24d29814d427a45ac12aeb1fbffd001dd4'
evidence.update(base=package['base'], head=package['head'])
for item in package['files']:
    actual = fp((REPO / item['path']).read_bytes())
    assert actual == {k: item[k] for k in actual}, item['path']
    evidence['files'][item['path']] = actual
for key in ('full_package', 'source_package'):
    item = package[key]
    actual = fp((SDK / item['path']).read_bytes())
    assert actual == {k: item[k] for k in actual}
    evidence['files'][item['path']] = actual
evidence['checks'].append('All 8 supplied fix files and 2 immutable diff packages match supplied SHA256/size; no Git command.')

code = (REPO / 'scripts/verify_extraction_knowledge.py').read_bytes()
tree = ast.parse(code)
function = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'verify_report')
assignment = next(n for n in function.body if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'count' for t in n.targets))
assert ast.unparse(assignment.value) == 'len(raw_atoms) if isinstance(raw_atoms, list) else None'
test_tree = ast.parse((REPO / 'tests/test_extraction_knowledge_verifier.py').read_bytes())
test_names = [n.name for n in ast.walk(test_tree) if isinstance(n, ast.FunctionDef) and n.name.startswith('test_')]
assert len(test_names) == 32 and len([n for n in test_names if n.startswith('test_final_fix_')]) == 4
evidence['code'] = {'count_expression': ast.unparse(assignment.value), 'line': assignment.lineno, 'test_methods': 32, 'new_boundary_methods': 4}

manifest = read_json(ASSETS / 'B04-T03-final-fix-1-manifest.json')
archive_path = ASSETS / manifest['archive']
archive_fp = fp(archive_path.read_bytes())
assert archive_fp == {'sha256': manifest['archive_sha256'], 'byte_length': manifest['archive_byte_length']}
with zipfile.ZipFile(archive_path) as archive:
    names = archive.namelist()
    assert len(names) == len(set(names)) == manifest['entry_count'] == len(manifest['files']) == 884
    indexed = {item['member']: item for item in manifest['files']}
    assert set(names) == set(indexed)
    blobs = {}
    for name in names:
        assert not PurePosixPath(name).is_absolute() and '..' not in PurePosixPath(name).parts
        raw = archive.read(name)
        item = indexed[name]
        assert fp(raw) == {k: item[k] for k in ('sha256', 'byte_length')}, name
        # Only explicit paths in this scoped fix manifest; never directory discovery.
        assert Path(item['path']).read_bytes() == raw, item['path']
        blobs[name] = raw
evidence['archive'] = {**archive_fp, 'members': 884, 'member_hashes_sizes_checked': 884, 'manifest_listed_readable_paths_compared': 884, 'unique_safe_member_names': True}
evidence['checks'].append('Read every ZIP member and every explicit manifest path; SHA256/size/member inventory/CRC and readable-byte equality all hold.')

def archived_json(name):
    return json.loads(blobs[name])

log_records = {}
for name, count, failures in (('red', 4, 7), ('green-focused', 4, 0), ('green-module', 32, 0)):
    filename = 'final-fix-1-' + name + '.log'
    raw = (SDK / filename).read_bytes()
    assert raw == blobs['logs/' + filename] == (ASSETS / ('B04-T03-' + filename)).read_bytes()
    text = raw.decode('utf-8')
    assert re.search(r'Ran ' + str(count) + r' tests in [0-9.]+s', text)
    assert ('FAILED (failures=7)' in text) if failures else text.rstrip().endswith('OK')
    if failures:
        assert text.count('\nFAIL: ') == 7
        assert text.count('AssertionError: actual candidate count') == 6
        assert text.count('AssertionError: AssertionError not raised') == 1
        assert '\nERROR:' not in text
    log_records[name] = {'tests': count, 'failures': failures, **fp(raw)}
evidence['retained_runs'] = log_records

binary_fp = fp((REPO / 'target/debug/noetome.exe').read_bytes())
assert binary_fp['sha256'] == '7a1eb3cae1fef5e9485848304d59dfd5c7804299190465a0aa126acfd6b56241'
for log in ('final-fix-1-red-sha.txt', 'final-fix-1-final-sha.txt'):
    raw = (SDK / log).read_bytes()
    assert raw == blobs['logs/' + log]
    assert binary_fp['sha256'].upper() in raw.decode()
    assert evidence['files']['tests/test_extraction_knowledge_verifier.py']['sha256'].upper() in raw.decode()
    expected = '6f9760ef8379f5cba459a6989d0544f760200ff24c47493f91d6c3b1411a0b43' if 'red-sha' in log else fp(code)['sha256']
    assert expected.upper() in raw.decode()
evidence['binary'] = binary_fp
evidence['retained_red_source_sha256'] = '6f9760ef8379f5cba459a6989d0544f760200ff24c47493f91d6c3b1411a0b43'

missing = {'missing-payload', 'missing-atoms', 'null-payload', 'null-atoms', 'object-atoms', 'string-atoms', 'forged-zero'}
calls = []
boundaries = []
for declared in manifest['actual_calls']:
    root = Path(declared['root']).name
    setup = archived_json('runs/' + root + '/setup-calls.json')
    assert len(setup) == 3 and all(r['exit'] == 0 for r in setup)
    assert [r['argv'][1:3] for r in setup] == [['project', 'init'], ['source', 'import'], ['extract', 'verify']]
    receipts = [n for n in names if n.startswith('runs/' + root + '/') and n.endswith('-cli-receipt.json')]
    new = [n for n in receipts if '/test_final_fix_' in n]
    older = [n for n in receipts if n not in new]
    assert len(new) == 10 and len(older) == declared['existing_negative_calls']
    for name in receipts:
        receipt = archived_json(name)
        assert receipt['exit'] == 3 and receipt['argv'][1:3] == ['extract', 'verify']
        stdout = json.loads(receipt['stdout'])
        assert stdout['network_calls'] == stdout['paid_calls'] == 0
        if name not in new:
            continue
        assert receipt['binary'] == binary_fp
        assert receipt['source_before'] == receipt['source_after']
        assert receipt['sql_before'] == receipt['sql_after']
        case = PurePosixPath(name).name.removesuffix('-cli-receipt.json')
        prefix = name.rsplit('/', 1)[0] + '/' + case
        output = prefix + '-output/'
        candidate = blobs[prefix + '-inputs/candidates.json']
        assert candidate == blobs[output + 'candidates-original.json']
        report = archived_json(output + 'verification.json')
        assert report['origin']['input_fingerprints']['candidates.json'] == fp(candidate)
        assert report['artifacts']['candidates-original.json'] == fp(candidate)
        assert output + 'knowledge-ir.json' not in blobs
        assert stdout['status'] == report['status'] == ('unprocessed' if case == 'valid-empty' else 'failed')
        assert len(stdout['candidate_results']) == len(report['candidate_results'])
        for artifact, fingerprint in report['artifacts'].items():
            assert fp(blobs[output + artifact]) == fingerprint
        if case == 'forged-zero':
            coherent = json.loads(json.dumps(stdout))
            coherent['totals'].update(candidate_atoms=0, unexpected_atoms=0)
            assert coherent == report
            md = blobs[output + 'verification.md'].decode().replace('\r\n', '\n')
            assert json.loads(md.split('```json\n', 1)[1].split('\n```', 1)[0]) == report
        else:
            assert stdout == report
        totals = stdout['totals']
        expected = (None, None, None, 0) if case in missing else ((2, 1, None, 2) if case == 'bad-elements' else (0, 0, 0 if case == 'valid-empty' else None, 0))
        actual = (totals['candidate_atoms'], totals['unexpected_atoms'], totals['duplicate_atoms'], len(stdout['candidate_results']))
        assert actual == expected, (root, case, actual, expected)
        boundaries.append({'root': root, 'case': case, 'candidate_bytes': candidate.decode(), 'candidate_fingerprint': fp(candidate), 'actual_cli_totals': list(actual), 'retained_verifier_forged_mutation': case == 'forged-zero'})
    calls.append({'root': root, 'setup_calls': len(setup), 'boundary_calls': len(new), 'existing_negative_calls': len(older), 'total': len(setup) + len(receipts)})
assert [r['total'] for r in calls] == [13, 13, 17]
evidence['retained_cli_calls'] = {'runs': calls, 'total': sum(r['total'] for r in calls), 'boundary_receipts_audited': 30, 'other_negative_receipts_audited': 4, 'setup_receipts_audited': 9, 'source_inventory_and_sql_equal_receipts': 30, 'per_receipt_binary_fingerprints_checked': 30, 'legacy_receipt_limit': 'The 4 existing negative receipts contain argv/exit/stdout/stderr only; binary equality for the complete run is supported by retained RED/final SHA logs, not invented per-receipt fields.'}
evidence['boundary_evidence'] = boundaries

original_review = SDK / 'final-review-evidence.json'
original_fp = fp(original_review.read_bytes())
original_probe = read_json(original_review)['summaries']['missing_array_probe']
outcomes = read_json(SDK / 'final-fix-1-probe-outcomes.json')
assert outcomes == archived_json('logs/final-fix-1-probe-outcomes.json') == read_json(ASSETS / 'B04-T03-final-fix-1-probe-outcomes.json')
assert original_fp == outcomes['review_evidence_before'] == outcomes['review_evidence_after']
assert outcomes['source'] == fp(code)
assert outcomes['ordinary_cli_calls'] == outcomes['new_model_calls'] == outcomes['network_calls'] == 0
original_files = {n: base64.b64decode(b) for n, b in original_probe['unknown_count_files_base64'].items()}
assert original_files['candidates-original.json'] == b'{}'
for outcome in outcomes['outcomes']:
    truthful = outcome['name'] == 'null-count'
    expected_files = dict(original_files)
    if not truthful:
        expected_files['verification.json'] = base64.b64decode(original_probe['zero_count_report_base64'])
        expected_files['verification.md'] = b'```json\n' + expected_files['verification.json'] + b'\n```\n'
    for name, raw in expected_files.items():
        assert (Path(outcome['output']) / name).read_bytes() == raw
        assert blobs['probes/final-fix-1-probe-' + outcome['name'] + '/' + name] == raw
        assert fp(raw) == outcome['fingerprints'][name]
    report = json.loads(expected_files['verification.json'])
    assert report['origin']['input_fingerprints']['candidates.json'] == fp(b'{}')
    assert report['artifacts']['candidates-original.json'] == fp(b'{}')
    assert outcome['accepted'] is truthful
    if truthful:
        assert outcome['verification']['candidate_atoms'] is None and not outcome['verification']['independently_passed']
    else:
        assert outcome['error'] == 'actual candidate count'
null_report = json.loads(original_files['verification.json'])
zero_report = json.loads(base64.b64decode(original_probe['zero_count_report_base64']))
null_report['totals'].update(candidate_atoms=0, unexpected_atoms=0)
assert null_report == zero_report
assert original_probe['incorrect_zero_accepted']['candidate_atoms'] == 0 and original_probe['result'] == 'actual candidate count'
evidence['saved_original_probes'] = {'original_review_fingerprint': original_fp, 'candidate_fingerprint': fp(b'{}'), 'exact_original_null_files_compared': len(original_files), 'exact_original_zero_report_compared': True, 'only_two_total_values_differ': True, 'retained_fixed_outcomes': [{'name': r['name'], 'accepted': r['accepted'], 'verification': r.get('verification'), 'error': r.get('error')} for r in outcomes['outcomes']], 'new_probe_execution': False}

final = outcomes['final_original06_read_only']
for name, expected in final['fingerprints'].items():
    assert fp((Path(final['output']) / name).read_bytes()) == expected
raw = (Path(final['output']) / 'verification.json').read_bytes()
assert raw == (ASSETS / 'B04-T03-root-final-original06-verification.json').read_bytes()
report = json.loads(raw)
assert report['status'] == final['verification']['status'] == 'passed'
assert report['totals']['expected_atoms'] == report['totals']['candidate_atoms'] == 1
assert report['quality']['exception']['expected'] == 0
assert final['verification']['independently_passed']
assert report['network_calls'] == report['paid_calls'] == 0
for name, fingerprint in report['artifacts'].items():
    assert fp((Path(final['output']) / name).read_bytes()) == fingerprint
evidence['saved_final_original06'] = {'output': final['output'], 'fingerprints': final['fingerprints'], 'report_equals_committed_copy': True, 'retained_fixed_verification': final['verification'], 'new_cli_calls': 0, 'new_verifier_probe_execution': False}
assert fp(original_review.read_bytes()) == original_fp
evidence['checks'].append('Original R1 base64 bytes, coherent JSON/Markdown/fingerprints, fixed saved outcomes and all 9 original06 output fingerprints verified; no probe execution was needed.')
evidence['limitations'] = ['Scoped fix review only; historical whole-branch WithFixes review remains historical.', 'No suite/module/matrix/CLI/Rust/build/install/UI/network/model/credential/private scan/Git/cleanup executed.', 'Covering test outcomes and probe acceptance are audited retained executions, not new reviewer test runs.', 'Supplied base/head/package metadata are SHA-bound; no live Git identity query.', 'Minor R2 remains accepted deferred; no UI re-review.', 'Controller owns final full Python regression, standards/ledger closure and push.', 'No new scientific quality/general NLP/PDF/OCR/performance claim.']
evidence['findings'] = [{'id': 'R1', 'severity': 'Important', 'verdict': 'ADDRESSED', 'file': 'scripts/verify_extraction_knowledge.py', 'line': 510}]
evidence['new_breakage'] = []
evidence['out_of_scope_observations'] = []
evidence['audit_helper_corrections'] = [{'initial_attempt_exit': 1, 'initial_traceback': 'final-fix-1-rereview-audit.py:133 AssertionError: assumed all negative CLI reports had status failed', 'read_only_resolution': 'Retained valid-empty stdout/report correctly have status unprocessed with CLI exit3; all other 9 boundary cases are failed. Helper assertion corrected without changing evidence or product.'}]
evidence['audit_helper_corrections'].append({'attempt_exit': 1, 'traceback': 'final-fix-1-rereview-audit.py:142 IndexError: Markdown fence split assumed LF', 'read_only_resolution': 'Windows write_text retained CRLF in coherent forged-zero Markdown. Normalize line endings for parsing only; exact bytes and fingerprints remain checked.'})
evidence['audit_helper_corrections'].append({'attempt_exit': 1, 'traceback': 'final-fix-1-rereview-audit.py:117 KeyError binary', 'read_only_resolution': 'The 4 unchanged legacy negative receipts have only argv/exit/stdout/stderr. Restrict per-receipt binary/source/SQL claims to 30 new boundary receipts and record this limitation.'})
evidence['fix_round_verdict'] = 'All findings addressed, no new Critical/Important breakage'
out = SDK / 'final-fix-1-rereview-evidence.json'
assert not out.exists(), 'Never overwrite a prior rereview artifact'
out.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'evidence': str(out), 'archive': evidence['archive'], 'retained_runs': log_records, 'retained_cli_calls': evidence['retained_cli_calls'], 'R1': 'ADDRESSED', 'new_test_runs': 0, 'new_cli_calls': 0}, ensure_ascii=False))
