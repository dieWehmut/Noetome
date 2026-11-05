from __future__ import annotations
import argparse, hashlib, json, os, re, subprocess, sys, time
from datetime import datetime, timezone
from pathlib import Path

def pin_bytes(data):return {'sha256':hashlib.sha256(data).hexdigest(),'byte_length':len(data)}
def pin(path):return pin_bytes(path.read_bytes())
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

parser=argparse.ArgumentParser()
parser.add_argument('root',type=Path)
parser.add_argument('--from-stage',choices=['fmt','workspace-check','workspace-tests','python-full','presentation-audit','process-flow-audit'],default='fmt')
args=parser.parse_args();root=args.root
assert sys.platform=='win32' and sys.version_info[:3]==(3,12,14),'must execute actual selected native Windows Python'
cfg=json.loads((root/'root.json').read_text(encoding='utf-8'))
manifest=json.loads((root/'snapshot.json').read_text(encoding='utf-8'))
repo=Path(cfg['repo_native']);python=cfg['python_native'];cli=repo/'target/debug/noetome.exe'
assert cfg['snapshot_head']==manifest['head']
def inventory():return {n:pin(repo/n) for n in manifest['files']}
before=inventory();assert before==manifest['files'],'snapshot preflight mismatch'
compiler=repo/'target/compiler-temp';compiler.mkdir(parents=True,exist_ok=True)
# Credential tests intentionally reject any credential path inside the repo.
# Their synthetic external files need a temp directory outside the snapshot;
# cargo_local.ps1 separately selects its own compiler temporary directory.
runtime_temp=root/'runtime-temp';runtime_temp.mkdir(exist_ok=True)
env=os.environ.copy();env.update(NOETOME_TEST_PYTHON=cfg['fake_python_native'],NOETOME_PDF_PYTHON=python,NOETOME_TEST_BINARY=str(cli),PYTHONUTF8='1',TEMP=str(runtime_temp),TMP=str(runtime_temp))
write(root/'native-runtime.json',{'python':sys.version,'executable':sys.executable,'platform':sys.platform,'selected_env':{k:env[k] for k in ['NOETOME_TEST_PYTHON','NOETOME_PDF_PYTHON','NOETOME_TEST_BINARY','TEMP','TMP']},'head':manifest['head']})
external=root.parent/(root.name+'-external-sentinel.txt')
if not external.exists():external.write_bytes(b'self-owned external Windows validation sentinel\n')
external_before=pin(external)
log=root/'native-commands.json';commands=json.loads(log.read_text(encoding='utf-8')) if log.exists() else []
required=['fmt','workspace-check','workspace-tests','python-full','presentation-audit','process-flow-audit']

def command(name,argv):
    attempt=1+sum(c['name']==name for c in commands)
    stem=name+'-attempt-'+str(attempt)
    out=root/(stem+'.stdout.bin');err=root/(stem+'.stderr.bin')
    assert not out.exists() and not err.exists(),'never overwrite command originals'
    begin=datetime.now(timezone.utc).isoformat();started=time.monotonic();cli_before=pin(cli) if cli.exists() else None
    print('START',name,'attempt',attempt,flush=True)
    timed_out=False;kill_result=None
    with out.open('xb') as stdout,err.open('xb') as stderr:
        proc=subprocess.Popen(argv,cwd=repo,env=env,stdout=stdout,stderr=stderr)
        write(root/'native-running.json',{'name':name,'attempt':attempt,'pid':proc.pid,'argv':argv,'started_utc':begin,'scope':'running PID is a locator; inspect process for liveness'})
        try:code=proc.wait(timeout=1800)
        except subprocess.TimeoutExpired:
            timed_out=True
            killed=subprocess.run(['taskkill.exe','/PID',str(proc.pid),'/T','/F'],capture_output=True,timeout=15)
            (root/(stem+'.timeout-cleanup.stdout.bin')).write_bytes(killed.stdout)
            (root/(stem+'.timeout-cleanup.stderr.bin')).write_bytes(killed.stderr)
            kill_result={'exit_code':killed.returncode,'stdout':pin_bytes(killed.stdout),'stderr':pin_bytes(killed.stderr)}
            code=proc.wait(timeout=15)
    after=inventory();external_after=pin(external)
    receipt={'name':name,'attempt':attempt,'argv':argv,'cwd':str(repo),'pid':proc.pid,'started_utc':begin,'elapsed_seconds':time.monotonic()-started,'exit_code':code,'timed_out':timed_out,'timeout_cleanup':kill_result,'stdout':{'path':str(out),**pin(out)},'stderr':{'path':str(err),**pin(err)},'tracked_before_after_match':after==before,'external_sentinel_before_after_match':external_after==external_before,'cli_before':cli_before,'cli_after':pin(cli) if cli.exists() else None,'snapshot_head':manifest['head']}
    commands.append(receipt);write(root/(stem+'.json'),receipt);write(log,commands)
    write(root/'native-running.json',{'name':name,'attempt':attempt,'pid':proc.pid,'status':'terminal','exit_code':code})
    latest={c['name']:c for c in commands}
    write(root/'native-gates.json',{'head':manifest['head'],'gates':{n:latest[n]['exit_code'] if n in latest else 'not_run' for n in required},'all_required_passed':all(n in latest and latest[n]['exit_code']==0 and not latest[n]['timed_out'] and latest[n]['snapshot_head']==manifest['head'] for n in required),'scope':'native regression gates; NTFS and Obsidian evidence separately required'})
    print('END',name,code,round(receipt['elapsed_seconds'],2),flush=True)
    assert after==before,'snapshot source changed during command'
    assert external_after==external_before,'self-owned external sentinel changed'
    return code==0 and not timed_out


ps=['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',str(repo/'scripts/cargo_local.ps1')]
stages=[('fmt',ps+['fmt','--all','--check']),('workspace-check',ps+['check','--workspace','--locked','--offline']),('workspace-tests',ps+['test','--workspace','--locked','--offline','--','--test-threads=4','--nocapture']),('python-full',[python,'-X','utf8','-m','unittest','discover','-s','tests','-p','test_*.py'])]
start_index=required.index(args.from_stage)
for index,(name,argv) in enumerate(stages):
    if index<start_index:continue
    if not command(name,argv):sys.exit(1)
assert cli.is_file(),'fresh native CLI missing'
audit_outputs={}
for index,(name,script) in enumerate([('presentation-audit','verify_knowledge_presentation.py'),('process-flow-audit','verify_process_flow.py')],start=4):
    if index<start_index:continue
    attempt=1+sum(c['name']==name for c in commands)
    audit_root=root/(name+'-'+str(attempt))
    assert not audit_root.exists(),'audit requires a fresh output root'
    audit_cli=root/(name+'-cli-'+str(attempt)+'.exe')
    with audit_cli.open('xb') as target:target.write(cli.read_bytes())
    assert pin(audit_cli)==pin(cli) and audit_cli.stat().st_nlink==1
    write(root/(name+'-cli-'+str(attempt)+'.json'),{'built_cli':str(cli),'audit_cli':str(audit_cli),'fingerprint':pin(cli),'copied_bytes_equal':True,'link_count':audit_cli.stat().st_nlink})
    if not command(name,[python,'-X','utf8','scripts/'+script,'audit',str(audit_cli),str(audit_root)]):sys.exit(1)
    audit_outputs[name]=str(audit_root)
latest={c['name']:c for c in commands}
assert all(n in latest and latest[n]['exit_code']==0 and not latest[n]['timed_out'] and latest[n]['snapshot_head']==manifest['head'] for n in required),'all six gates must have passed on this exact source snapshot'
assert all(latest[n]['cli_after']==pin(cli) for n in ['workspace-tests','python-full','presentation-audit','process-flow-audit']),'tests and audits must use the final same native executable'
rust_text=Path(latest['workspace-tests']['stdout']['path']).read_text(encoding='utf-8',errors='strict')
summaries=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',rust_text)]
assert summaries and all(failed==0 for passed,failed,ignored in summaries)
python_text=Path(latest['python-full']['stderr']['path']).read_text(encoding='utf-8',errors='strict')
audits={}
for name in ['presentation-audit','process-flow-audit']:
    result=json.loads(Path(latest[name]['stdout']['path']).read_text(encoding='utf-8'))
    assert result['status']=='passed'
    audits[name]={'result':result,'root':audit_outputs.get(name,str(root/(name+'-'+str(latest[name]['attempt']))))}
write(root/'native-final.json',{'head':manifest['head'],'commands':commands,'required_latest_exits':{n:latest[n]['exit_code'] for n in required},'tracked_files':len(before),'tracked_before_after_match':True,'external_sentinel_before':external_before,'external_sentinel_after':pin(external),'cli':pin(cli),'rust_suite_summaries':summaries,'python_summary':python_text,'audits':audits,'scope':'six exact-source native regression/audit gates passed; targeted live-handle/NTFS protection and actual Obsidian output validation are separate and not implied'})
print('ALL_SIX_EXACT_SOURCE_NATIVE_GATES_PASSED',flush=True)
