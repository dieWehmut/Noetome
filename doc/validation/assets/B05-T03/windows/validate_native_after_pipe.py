from __future__ import annotations
import hashlib,json,os,subprocess,sys,time
from pathlib import Path
from datetime import datetime,timezone
root=Path(sys.argv[1]); cfg=json.loads((root/'root.json').read_bytes());repo=Path(cfg['repo_native']);python=cfg['python_native']
env=os.environ.copy();env.update(NOETOME_TEST_PYTHON=cfg['fake_python_native'],NOETOME_PDF_PYTHON=python,NOETOME_TEST_BINARY=str(repo/'target/debug/noetome.exe'),PYTHONUTF8='1')
manifest=json.loads((root/'snapshot.json').read_bytes());commands=json.loads((root/'native-commands.json').read_bytes())
def fp(p):
 b=p.read_bytes();return {'sha256':hashlib.sha256(b).hexdigest(),'byte_length':len(b)}
def inventory():return {name:fp(repo/name) for name in manifest['files']}
def write(name,value):(root/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
protected_paths=[Path(os.environ['LOCALAPPDATA'])/'Noetome'/x for x in ['model-services/tangmao-deepseek.json','model-services/tangmao-deepseek.usage.json','credentials/tangmao-deepseek.dpapi']]
protected={str(p):fp(p) if p.exists() else None for p in protected_paths}
before=inventory();assert before==manifest['files'];write('native-protected-before.json',protected)
write('native-runtime-after-pipe.json',{'python':sys.version,'executable':sys.executable,'platform':sys.platform,'selected_env':{k:env[k] for k in ['NOETOME_TEST_PYTHON','NOETOME_PDF_PYTHON','NOETOME_TEST_BINARY']}})
def command(name,argv):
 print('START',name,flush=True);out=root/(name+'.stdout.bin');err=root/(name+'.stderr.bin');start=time.monotonic();begin=datetime.now(timezone.utc).isoformat();cli=repo/'target/debug/noetome.exe';pin=fp(cli) if cli.exists() else None
 with out.open('wb') as o,err.open('wb') as e:r=subprocess.run(argv,cwd=repo,env=env,stdout=o,stderr=e,timeout=1800)
 after=inventory();assert after==before,'snapshot files changed'
 current={str(p):fp(p) if p.exists() else None for p in protected_paths};assert current==protected,'selected production config changed'
 receipt={'name':name,'argv':argv,'cwd':str(repo),'started_utc':begin,'elapsed_seconds':time.monotonic()-start,'exit_code':r.returncode,'stdout':{'path':str(out),**fp(out)},'stderr':{'path':str(err),**fp(err)},'tracked_before_after_match':True,'selected_production_before_after_match':True,'cli_before':pin,'cli_after':fp(cli) if cli.exists() else None}
 commands.append(receipt);write(name+'.json',receipt);write('native-commands.json',commands);print('END',name,r.returncode,round(receipt['elapsed_seconds'],2),flush=True)
 return r.returncode
ps=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(repo/'scripts/cargo_local.ps1')]
for name,args in [('workspace-tests-after-pipe',['test','--workspace','--locked','--offline','--','--test-threads=4'])]:
 if command(name,ps+args):sys.exit(1)
assert (repo/'target/debug/noetome.exe').is_file()
if command('python-full',[python,'-X','utf8','-m','unittest','discover','-s','tests','-p','test_*.py']):sys.exit(1)
if command('audit-v3',[python,'-X','utf8','scripts/verify_knowledge_coordination.py','audit-v3',str(repo/'target/debug/noetome.exe'),str(root/'audit-v3')]):sys.exit(1)
write('native-final.json',{'head':manifest['head'],'commands':commands,'tracked_files':len(before),'tracked_before_after_match':True,'protected_before':protected,'protected_after':protected,'cli':fp(repo/'target/debug/noetome.exe')});print('ALL_NATIVE_CHECKS_PASSED',flush=True)
