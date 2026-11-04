import io,json,sys,unittest,shutil
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'scripts'))
from knowledge_conflicts_audit import ConflictsAudit
from verify_knowledge_coordination import strict_json,encoded,fingerprint,verify_report
cli=Path(sys.argv[1]);root=Path(sys.argv[2])
a=ConflictsAudit(cli,root)
folder,project,case,gold=a.prepare('baseline','true_conflict')
legacy=folder/'legacy-case';shutil.copytree(case,legacy)
for file in ('coordination.json','identity.json'):
    document=strict_json((legacy/file).read_bytes());document['schema_version']=2
    if file=='coordination.json':document['purpose']='explicit_identity_condition_time'
    (legacy/file).write_bytes(encoded(document))
expected=strict_json(gold);expected['schema_version']=2;expected['scope']='explicit_identity_condition_time_v2'
for key in ('true_conflicts','insufficient_evidence_relations','conflicting_occurrences','insufficient_evidence_occurrences','counter_evidence_links'):expected['totals'].pop(key)
for collection in ('facts','occurrences'):
    for obj in expected[collection]:obj['issue_codes']=['COORD_UNHANDLED_DIFFERENCE']
for pair in expected['relations']:pair.update(relation='needs_review',issue_code='COORD_UNHANDLED_DIFFERENCE')
legacy_gold=encoded(expected);(folder/'legacy-gold-before-cli.json').write_bytes(legacy_gold)
out=folder/'legacy-output'
run=a.call('legacy-coordinate',['knowledge','coordinate',project,legacy,out],3)
accepted=verify_report(project,legacy,out,legacy_gold)
report=strict_json(run.stdout)
counter_folder,counter_project,counter_case,counter_gold=a.prepare('counter-baseline','counter_support')
counter_legacy=counter_folder/'legacy-case';shutil.copytree(counter_case,counter_legacy)
for file in ('coordination.json','identity.json'):
    document=strict_json((counter_legacy/file).read_bytes());document['schema_version']=2
    if file=='coordination.json':document['purpose']='explicit_identity_condition_time'
    (counter_legacy/file).write_bytes(encoded(document))
ce=strict_json(counter_gold);ce['schema_version']=2;ce['scope']='explicit_identity_condition_time_v2'
for key in ('true_conflicts','insufficient_evidence_relations','conflicting_occurrences','insufficient_evidence_occurrences','counter_evidence_links'):ce['totals'].pop(key)
ce['totals']['evidence_links']-=1
for collection in ('facts','occurrences'):
    for obj in ce[collection]:obj['issue_codes']=['COORD_UNHANDLED_DIFFERENCE']
for pair in ce['relations']:pair.update(relation='needs_review',issue_code='COORD_UNHANDLED_DIFFERENCE')
counter_legacy_gold=encoded(ce);(counter_folder/'legacy-gold-before-cli.json').write_bytes(counter_legacy_gold)
counter_output=counter_folder/'legacy-output'
cr=a.call('counter-legacy-coordinate',['knowledge','coordinate',counter_project,counter_legacy,counter_output],3)
counter_accepted=verify_report(counter_project,counter_legacy,counter_output,counter_legacy_gold)
counter_report=strict_json(cr.stdout)
class RequiredNewVerdict(unittest.TestCase):
    def test_real_same_condition_differing_values_are_classified_as_true_conflict(self):
        self.assertEqual(report['relations'][0]['relation'],'true_conflict')
    def test_all_linked_counter_evidence_must_be_retained(self):
        occurrence=next(o for o in counter_report['occurrences'] if o['input_id']=='a')
        self.assertEqual([e['evidence_id'] for e in occurrence['original_evidence']],['a-support','a-refutes'])
stream=io.StringIO();result=unittest.TextTestRunner(stream=stream,verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(RequiredNewVerdict))
(root/'required-new-verdict-red.txt').write_text(stream.getvalue())
(root/'independent-baseline-red.json').write_bytes(encoded(dict(schema_version=1,kind='real_v2_semantic_baseline_red',old_independent_check=accepted,counter_old_independent_check=counter_accepted,required='true_conflict',actual=report['relations'][0]['relation'],failures=len(result.failures),errors=len(result.errors),source_fixture_pins=a.authors and {k:fingerprint(v) for k,v in a.authors.items()},legacy_gold_before_cli=fingerprint(legacy_gold),calls=a.calls,protection=a.check_protected())))
print(stream.getvalue());print(root/'independent-baseline-red.json')
assert len(result.failures)==2 and not result.errors
