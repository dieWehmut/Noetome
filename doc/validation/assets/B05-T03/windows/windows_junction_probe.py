"""Actual native NTFS ancestor-junction admission checks; owns only this probe."""
from __future__ import annotations
import hashlib,json,os,stat,subprocess,sys,time,uuid
from datetime import datetime,timezone
from pathlib import Path
root=Path(__file__).parent
repo=root/'repo'
python=Path(r'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe')
cli=repo/'target/debug/noetome.exe'
probe=root/('junction-probe-'+uuid.uuid4().hex)
probe.mkdir()

def encoded(value):return (json.dumps(value,ensure_ascii=False,indent=2)+'\n').encode('utf-8')
def write(name,value):(probe/name).write_bytes(encoded(value))
def fp(path):
    raw=path.read_bytes();return {'sha256':hashlib.sha256(raw).hexdigest(),'byte_length':len(raw)}
def tree(path):
    result={}
    for p in sorted(path.rglob('*')):
        st=p.lstat()
        assert not stat.S_ISLNK(st.st_mode) and not getattr(st,'st_file_attributes',0)&0x400,'original root contains reparse'
        item={'directory':p.is_dir(),'mtime_ns':st.st_mtime_ns,'identity':[st.st_dev,st.st_ino]}
        if p.is_file():item.update(fp(p))
        result[str(p.relative_to(path))]=item
    return result
# Completion gate is a terminal receipt from the real native full Rust process.
workspace=json.loads((root/'workspace-tests-after-pipe.json').read_bytes());assert workspace['exit_code']==0
commands=json.loads((root/'native-commands.json').read_bytes());assert any(c['name']=='python-full' and c['exit_code']==0 for c in commands)
selected=[]
for folder in sorted((repo/'target/compiler-temp').glob('conflicts-*')):
    authored=folder/'frozen-author.json'
    if authored.is_file():
        author=json.loads(authored.read_bytes())
        if author.get('name')=='counter_to_claim' and (folder/'output/coordination.json').is_file() and (folder/'expected.json').is_file():selected.append(folder)
assert selected,'completed native counter_to_claim missing'
folder=selected[-1]
project,case,output,gold=folder/'project',folder/'case',folder/'output',folder/'expected.json'
report=json.loads((output/'coordination.json').read_bytes())
assert report['schema_version']==3 and report['status']=='needs_review' and not (output/'coordination-ir.json').exists()
assert sorted(f['value'] for f in report['facts'])==['10','12']
occ=next(o for o in report['occurrences'] if o['input_id']=='a')
assert any(l['relation']=='refutes' for l in occ['original_atom']['atom']['evidence_links'])
counter=next(e for e in occ['original_evidence'] if e['evidence_id']=='a-refutes')
assert '10' in counter['quote'] and '不获本来源支持' in counter['quote']
fixtures=repo/'fixtures/knowledge-coordination-v3'
freeze=json.loads((fixtures/'frozen-sha256.json').read_bytes())
assert len(freeze['files'])==44
for name,pin in freeze['files'].items():assert fp(fixtures/name)==pin
protected={str(p):tree(p) for p in [project,case,output,fixtures]}
protected[str(gold)]=fp(gold)
cli_pin=fp(cli)
script_pins={str(repo/'scripts'/s):fp(repo/'scripts'/s) for s in ['verify_knowledge_coordination.py','verify_knowledge_conflicts.py','knowledge_conflicts_audit.py']}
write('protected-before.json',protected)
write('selected-input.json',{'selected_root':str(folder),'selected_gold':str(gold),'source_fixture_files':len(freeze['files']),'source_fixture_pin':fp(fixtures/'frozen-sha256.json'),'original_quote':counter['quote'],'cli':cli_pin,'script_pins':script_pins,'rust_gate_receipt':fp(root/'workspace-tests-after-pipe.json')})
rows=[]
def call(name,args,expected,contains=None):
    begin=datetime.now(timezone.utc).isoformat();start=time.monotonic()
    run=subprocess.run([str(a) for a in args],cwd=repo,capture_output=True,timeout=40)
    out=probe/(name+'.stdout.bin');err=probe/(name+'.stderr.bin');out.write_bytes(run.stdout);err.write_bytes(run.stderr)
    receipt={'name':name,'argv':[str(a) for a in args],'cwd':str(repo),'started_utc':begin,'elapsed_seconds':time.monotonic()-start,'exit_code':run.returncode,'expected_exit':expected,'stdout':{'path':str(out),**fp(out)},'stderr':{'path':str(err),**fp(err)},'cli_before_after_match':fp(cli)==cli_pin}
    rows.append(receipt);write(name+'.json',receipt);write('commands.json',rows)
    assert run.returncode==expected,(name,run.returncode,run.stdout.decode('utf-8','replace'),run.stderr.decode('utf-8','replace'))
    if contains:assert contains in run.stderr.decode('utf-8','replace'),(name,'missing diagnostic',run.stderr)
    assert fp(cli)==cli_pin,'CLI changed'
    return run
for script in ['verify_knowledge_coordination.py','verify_knowledge_conflicts.py']:
    stem='common' if script=='verify_knowledge_coordination.py' else 'dedicated'
    run=call('direct-'+stem,[python,'-X','utf8',repo/'scripts'/script,'check',project,case,output,gold],0)
    data=json.loads(run.stdout);assert data['independently_verified'] is True
alias=probe/'ancestor-junction'
call('create-junction',['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',root/'make_junction_probe.ps1','-Alias',alias,'-Target',folder],0)
st=alias.lstat();assert getattr(st,'st_file_attributes',0)&0x400,'junction is not actual reparse'
write('junction-metadata.json',{'path':str(alias),'target':str(folder),'st_file_attributes':getattr(st,'st_file_attributes',0),'st_mode':st.st_mode,'is_reparse':True})
for role in ['project','case','output','gold']:
    roots={'project':project,'case':case,'output':output,'gold':gold}
    roots[role]=alias/('expected.json' if role=='gold' else role)
    for script in ['verify_knowledge_coordination.py','verify_knowledge_conflicts.py']:
        stem='common' if script=='verify_knowledge_coordination.py' else 'dedicated'
        call('ancestor-'+role+'-'+stem,[python,'-X','utf8',repo/'scripts'/script,'check',roots['project'],roots['case'],roots['output'],roots['gold']],1,'ancestor link/reparse path')
for role,expected in [('project',3),('case',2),('output',2)]:
    roots={'project':project,'case':case,'output':probe/('unwritten-'+role)}
    roots[role]=alias/('unwritten-output' if role=='output' else role)
    call('rust-ancestor-'+role,[cli,'knowledge','coordinate',roots['project'],roots['case'],roots['output']],expected,'PROJECT_UNSAFE_PATH')
    assert not roots['output'].exists(),'rejected output created'
# Windows lexical '..' must not hide a reparse component.
call('lexical-parent-after-junction',[python,'-X','utf8',repo/'scripts/verify_knowledge_coordination.py','check',alias/'..'/folder.name/'project',case,output,gold],1,'ancestor link/reparse path')
after={str(p):tree(p) for p in [project,case,output,fixtures]};after[str(gold)]=fp(gold)
write('protected-after.json',after);assert after==protected,'original bytes/inventory changed'
assert {s:fp(Path(s)) for s in script_pins}==script_pins
summary={'kind':'native_windows_ancestor_junction_probe','platform':sys.platform,'python':sys.version,'python_executable':sys.executable,'selected_root':str(folder),'probe_root':str(probe),'command_count':len(rows),'commands':rows,'native_workspace_receipt_exit':workspace['exit_code'],'actual_ntfs_reparse':True,'frozen_author_files':len(freeze['files']),'protected_before_after_match':True,'cli_before_after_match':True,'script_before_after_match':True,'cli':cli_pin,'no_new_rejected_output':True,'junction_left_in_self_owned_probe':True,'limitations':'No scientific judgment or productionVault; this proves native path admission/retention only.'}
write('probe-summary.json',summary)
print(json.dumps({'probe':str(probe),'commands':len(rows),'protected':True,'cli_sha256':cli_pin['sha256']},ensure_ascii=False))
