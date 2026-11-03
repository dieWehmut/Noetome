"""Read-only retained-byte audit. Only writes final-review-evidence.json."""
import hashlib, json, re, subprocess, zipfile, io, importlib.util, copy, base64
from pathlib import Path
from datetime import datetime, timezone
ROOT=Path('D:/Noetome'); SDK=ROOT/'.superpowers/sdd/B04-T03_REVIEW_PLAN'; AS=ROOT/'doc/validation/assets'
E={'schema_version':1,'started_at':datetime.now(timezone.utc).isoformat(),'method':'static exact-range review plus retained ZIP/receipt byte audit; no suites, builds, CLI, network or private input access','checks':[],'fingerprints':{},'archives':{},'summaries':{}}
E['audit_setup_failures']=['First audit attempt stopped before evidence output: core archive uses asset-relative path, not repository-relative; corrected resolver, no product execution.']
E['audit_setup_failures'].append('Second audit found two apparent dual-report mismatches in historical reviewer forgery artifacts: helper regex did not normalize CRLF. Final gate report audit narrowed to the actual original06 matrix outputs; historical probes remain fully byte-fingerprinted, not mistaken for product results.')
def fp(b): return {'sha256':hashlib.sha256(b).hexdigest(),'byte_length':len(b)}
def read(p):
    p=Path(p); b=p.read_bytes(); E['fingerprints'][str(p)]=fp(b); return b
def j(p): return json.loads(read(p))
def ck(name,ok): E['checks'].append({'check':name,'passed':bool(ok)})
def git(*args): return subprocess.check_output(['git',*args],cwd=ROOT).decode().strip()
E['head_before']=git('rev-parse','HEAD'); E['status_before']=git('status','--short')
pkg=j(SDK/'final-review-package.json'); E['range']={'base':pkg['base'],'head':pkg['head']}
for k in ('full_package','source_doc'):
    name=pkg['full_package' if k=='full_package' else 'source_doc_package']; b=read(SDK/name)
    ck(k+' fingerprint', fp(b)=={'sha256':pkg[k+'_sha256'],'byte_length':pkg[k+'_bytes']})
    sections=re.findall(rb'^diff --git a/(.*?) b/(.*?)\r?$',b,re.M)
    E['summaries'][k+'_section_count']=len(sections)
    if k=='full_package':
        for _,p in sections:
            path=ROOT/p.decode(); read(path)
for name in ('core-assets','task-2','task-2-fix-1','root-evidence-01','root-evidence-02'):
    m=j(AS/('B04-T03-'+name+'-manifest.json'))
    if name=='core-assets':
        print('CORE_KEYS',list(m)); print('CORE_ARCHIVE',m.get('archive')); print('CORE_COPIES',str(m.get('readable_copies',m.get('copies')))[:800])
    archive=m.get('archive')
    if isinstance(archive,str): p=AS/archive; expected={'sha256':m['archive_sha256'],'byte_length':m['archive_byte_length']}
    elif isinstance(archive,dict):
        rel=archive.get('path',archive.get('file','')); p=(ROOT if '/' in rel else AS)/rel; expected={k:archive[k] for k in ('sha256','byte_length')}
    else: continue
    raw=read(p); ck(name+' archive fingerprint',fp(raw)==expected)
    z=zipfile.ZipFile(io.BytesIO(raw)); names=z.namelist(); ck(name+' unique members',len(names)==len(set(names)))
    entries=m.get('entries',m.get('source_entries'))
    if entries is None: entries={x['member']:{k:x[k] for k in ('sha256','byte_length')} for x in m['files']}
    ck(name+' member inventory',set(names)==set(entries))
    for member,expected in entries.items(): ck(name+' member '+member,fp(z.read(member))=={k:expected[k] for k in ('sha256','byte_length')})
    copies=m.get('readable_copies',{})
    if isinstance(copies,dict):
        for path,info in copies.items():
            member=info.get('archive_member',info.get('source'))
            if member: ck(name+' copy '+path,read((ROOT if '/' in path else AS)/path)==z.read(member))
    E['archives'][name]={'fingerprint':fp(raw),'members':len(names),'copy_count':len(copies),'entries':entries}
    if name in ('root-evidence-01','root-evidence-02'):
        print(name,'ROOT_MEMBERS',[n for n in names if not n.startswith('original06/') and not n.startswith('final-original06/')])
    if name=='root-evidence-01':
        a=json.loads(z.read('original06/audit.json'))
        E['summaries']['root_matrix']={k:a[k] for k in ('scenario_count','report_count','passed_knowledge_samples','error','network_calls','paid_calls')}
        E['summaries']['root_matrix']['calls']=len(a['calls']); ck('root matrix counts',len(a['rows'])==53 and len(a['calls'])==61 and a['error'] is None)
        for n in names:
            if n.startswith('original06/') and n.endswith('/verification.json'):
                r=json.loads(z.read(n)); prefix=n[:-len('verification.json')]
                md=z.read(prefix+'verification.md').decode(); embedded=re.findall(r'```json\n(.*?)\n```',md,re.S)
                ck(n+' dual facts',len(embedded)==1 and json.loads(embedded[0])==r)
                ck(n+' partition',r['totals']['expected_atoms']==1 and sum(r['totals'][s] for s in ('passed','failed','needs_review','unprocessed'))==1)
                for file,expected in r['artifacts'].items(): ck(n+' artifact '+file,fp(z.read(prefix+file))==expected)
                ck(n+' IR state',(prefix+'knowledge-ir.json' in names)==(r['status']=='passed'))
    if name=='root-evidence-02':
        a=json.loads(z.read('final-original06/audit.json')); ck('final original inputs before after',a['original_input_facts_before']==a['original_input_facts_after'])
        E['summaries']['final_original']={k:a[k] for k in ('exit_code','binary','verification','source_candidate_gold_response_config_budget_unchanged','new_model_calls','paid_calls','ordinary_cli_calls')}
        E['summaries']['budget']=a['original_input_facts_before']['budget']
        E['summaries']['final_original']['preserved_input_counts']={k:len(v) for k,v in a['original_input_facts_before'].items() if isinstance(v,dict)}
        r=json.loads(z.read('final-original06/output/verification.json')); c=json.loads(z.read('final-original06/case/candidates.json')); g=json.loads(z.read('final-original06/case/gold.json'))
        ck('final candidate original SHA',fp(z.read('final-original06/case/candidates.json'))['sha256']=='92fed9123fa6f711ce76df524e4ff79a36aada79a7c4f5fb72f64f68027915fa')
        ck('final gold original SHA',fp(z.read('final-original06/case/gold.json'))['sha256']=='7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d')
        u=r['scope']['units'][0]; ev=c['payload']['evidence'][0]; atom=c['payload']['atoms'][0]
        ck('final evidence real slice',u['text'].encode()[ev['excerpt_start_byte']-u['start_byte']:ev['excerpt_end_byte']-u['start_byte']]==ev['quote'].encode())
        ck('final evidence gold',g['quote'] in ev['quote'] and fp(ev['quote'].encode())['sha256']==ev['excerpt_sha256'])
        ck('final value unit conditions',atom['measurement']['value']==g['value'] and atom['measurement']['unit']==g['unit'] and set(g['conditions'])<=set(atom['measurement']['conditions']))
        E['summaries']['final_fact']={'content':atom['atom']['content'],'measurement':atom['measurement'],'gold':g,'origin':r['origin'],'quality':r['quality']}
for label in ('full-rust-fmt','full-rust-check','full-rust-tests','full-python-tests-external-temp','desktop-build','desktop-install'):
    r=j(AS/('B04-T03-root-'+label+'-command.json'))
    E['summaries'][label]={k:r[k] for k in ('exit_code','passed','code_unchanged','binary_after') if k in r}
    for path,f in r.get('code_after',{}).items():
        if path.startswith(('crates/','apps/','scripts/','tests/')): ck(label+' final source '+path,fp(read(ROOT/path))=={k:f[k] for k in ('sha256','byte_length')})
    for path,f in r.get('log_fingerprints',{}).items(): ck(label+' log '+path,fp(read(AS/('B04-T03-'+path)))=={k:f[k] for k in ('sha256','byte_length')})
rust=read(AS/'B04-T03-root-full-rust-tests-stdout.log').decode(); rows=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',rust)
E['summaries']['rust_actual_totals']=[sum(int(row[i]) for row in rows) for i in range(3)]; ck('Rust532 retained log',E['summaries']['rust_actual_totals']==[532,0,0])
py=read(AS/'B04-T03-root-full-python-tests-external-temp-stderr.log').decode(); ck('Python104 retained log',bool(re.search(r'Ran 104 tests',py)) and '\nOK' in py)
# One named unanswered artifact risk: no payload/atoms means unknown, not zero.
# Read authentic negative artifacts into a virtual path map; no new CLI or files.
spec=importlib.util.spec_from_file_location('final_review_oracle',ROOT/'scripts/verify_extraction_knowledge.py'); mod=importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
z=zipfile.ZipFile(AS/'B04-T03-root-evidence-01.zip'); prefix='original06/out-candidate-extra/'
files={n[len(prefix):]:z.read(n) for n in z.namelist() if n.startswith(prefix)}
report=json.loads(files['verification.json']); source={'binding':report['binding'],'units':report['scope']['units']}
inputs={name:files[copyname] for name,copyname in [('review.json','review-original.json'),('candidates.json','candidates-original.json'),('gold.json','gold-original.json'),('decision.json','decision-original.json')]}
class VPath:
    def __init__(self,p): self.p=p.p if isinstance(p,VPath) else str(p)
    def __truediv__(self,name): return VPath(self.p+'/'+name)
    @property
    def name(self): return self.p.split('/')[-1]
    def read_bytes(self): return files[self.name]
    def read_text(self,encoding=None): return self.read_bytes().decode(encoding or 'utf-8').replace('\r\n','\n')
    def exists(self): return self.name in files
mod.Path=VPath
baseline=mod.verify_report(VPath('virtual'),inputs['candidates.json'],inputs['gold.json'],source,inputs); ck('missing-array probe authentic baseline',baseline['status']=='failed')
raw=b'{}'; inputs['candidates.json']=raw; files['candidates-original.json']=raw
report['origin']['input_fingerprints']['candidates.json']=fp(raw); report['artifacts']['candidates-original.json']=fp(raw)
report['totals']['candidate_atoms']=None; report['totals']['unexpected_atoms']=None; report['candidate_results']=[]
files['verification.json']=json.dumps(report,ensure_ascii=False).encode(); files['verification.md']=('```json\n'+json.dumps(report,ensure_ascii=False)+'\n```\n').encode()
try:
    mod.verify_report(VPath('virtual'),raw,inputs['gold.json'],source,inputs); probe='accepted'
except AssertionError as exc: probe=str(exc)
E['summaries']['missing_array_probe']={'method':'single in-memory coherent schema-failure report based on retained out-candidate-extra; Path adapter prevents disk writes','candidate':'{}','candidate_fingerprint':fp(raw),'report_fingerprint':fp(files['verification.json']),'baseline':baseline,'result':probe,'expected_actual_count':None,'python_count_from_branch':0,'product_cli_calls':0}
ck('missing-array discrepancy reproduced',probe=='actual candidate count')
E['summaries']['missing_array_probe']['unknown_count_files_base64']={k:base64.b64encode(v).decode() for k,v in files.items()}
report['totals']['candidate_atoms']=0; report['totals']['unexpected_atoms']=0
files['verification.json']=json.dumps(report,ensure_ascii=False).encode(); files['verification.md']=('```json\n'+json.dumps(report,ensure_ascii=False)+'\n```\n').encode()
false_zero=mod.verify_report(VPath('virtual'),raw,inputs['gold.json'],source,inputs)
E['summaries']['missing_array_probe']['incorrect_zero_accepted']=false_zero
E['summaries']['missing_array_probe']['zero_count_report_base64']=base64.b64encode(files['verification.json']).decode()
ck('missing-array forged zero acceptance reproduced',false_zero['candidate_atoms']==0)
E['findings']=[{'id':'R1','severity':'Important','priority':2,'file':'scripts/verify_extraction_knowledge.py','line':510,'issue':'Syntactically valid candidate without an atoms array coerces unknown count to zero; truthful null-count rejection report fails while fabricated zero-count report passes.'},{'id':'R2','severity':'Minor','priority':3,'file':'apps/desktop/src/WindowControls.tsx','line':27,'issue':'A recovered resize state query does not clear its previous state-read error; next explicit window action clears it.'}]
E['verdict']={'spec':'one unknown-count acceptance gap','code_quality':'with fixes','ready_to_merge':'with fixes','critical':0,'important':1,'minor':1}
for name in ('actions-01','exit-settings','preference-restored'):
    r=j(AS/('B04-T03-ui-native-'+name+'.json')); E['summaries']['native-'+name]={'checks':[{k:c[k] for k in ('name','passed')} for c in r['checks']],'passed':r['passed']}
for name in ('title-band','crop','user-confirmation','exit-process','normal-launch','normal-window'):
    r=j(AS/('B04-T03-ui-native-'+name+'.json')); E['summaries']['native-'+name]=r
before=j(AS/'B04-T03-root-inputs-before.json'); after=j(AS/'B04-T03-root-inputs-after.json'); ck('root complete facts unchanged',before['facts']==after['facts'])
E['outside_diff_interfaces']=[{'path':'crates/noetome-desktop/src/main.rs','reason':'Verify new native close request continues through existing CloseRequested/refresh/hide-to-tray/exit path; inspected show and close handlers at lines143-223','fingerprint':fp(read(ROOT/'crates/noetome-desktop/src/main.rs'))}]
ui=j(AS/'B04-T03-ui-implementation-final-checks.json'); ck('retained frontend10',len(ui['results'])==10 and all(v['status']=='passed' for v in ui['results']))
for path,expected in ui['sources'].items(): ck('retained frontend source '+path,fp(read(ROOT/path))['sha256']==expected)
E['summaries']['ui_retained_tests']=10
E['limitations']=['No new suite/matrix/build/ordinary CLI/native app operation/network/private-folder inspection. Counts of test execution are read from retained receipts, not rerun.','The sole fresh probe is an in-memory coherent negative-report variant; it is not represented as an actual newly emitted Rust report. Candidate and report raw bytes are embedded in evidence.','Physical tray restore/drag rely on preserved direct user confirmation; no physical double-click/taskbar test or movement delta claimed.','Full-window crop viewed by reviewer; top crop y=0 verified from receipt; raw screen intentionally not accessed.','Controlled grammar/one-measurement gate only. No independent scientific or formal accuracy benchmark.','Body conditions containing the redundant allowed-context phrase expose a static Rust/Python language difference; contract explicitly permits this phrase in measurement/qualifiers but does not require it as an extra body condition. Not a confirmed blocking scope failure for the accepted inputs; future grammar changes should align both implementations.']
E['head_after']=git('rev-parse','HEAD'); E['status_after']=git('status','--short'); ck('HEAD fixed',E['head_after']==E['head_before']==pkg['head']); ck('user tree fixed',E['status_before']==E['status_after'])
E['helper']=fp(Path(__file__).read_bytes()); E['completed_at']=datetime.now(timezone.utc).isoformat(); E['assertions']={'executed':len(E['checks']),'passed':sum(x['passed'] for x in E['checks']),'failed':sum(not x['passed'] for x in E['checks'])}
(SDK/'final-review-evidence.json').write_text(json.dumps(E,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('SUMMARY',json.dumps(E['summaries'],ensure_ascii=False)[:7500]); print('COUNTS',E['assertions']); print('FAILURES',[x for x in E['checks'] if not x['passed']][:30])
