from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,sys,time,zipfile
ROOT=Path('/home/lyinn/project/Noetome')
MODE=sys.argv[1]
BASE=(Path('/mnt/d/Noetome/.artifacts/b06v-t02-continuation-final-20261106') if MODE=='windows' else ROOT/'.artifacts/b06v-t02-continuation-linux-final-02-20261106')
BASE.mkdir()
SNAP=BASE/'repo'; SNAP.mkdir()
paths=list(json.loads(Path('/mnt/d/Noetome/.artifacts/b06v-t02-python-live-final-20261106/source-before.json').read_bytes()))
def pin(p):
 b=p.read_bytes();return dict(sha256=hashlib.sha256(b).hexdigest(),byte_length=len(b))
def save(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
before={p:pin(ROOT/p) for p in paths}
for p in paths:
 out=SNAP/p;out.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/p,out)
assert {p:pin(SNAP/p) for p in paths}==before
save(BASE/'source-before.json',before)
with zipfile.ZipFile(BASE/'source.zip','x',zipfile.ZIP_DEFLATED) as z:
 for p in paths:z.write(SNAP/p,p)
script=(ROOT/'.artifacts/b06v-t02-continuation-tools/run_regression_retry.py').read_bytes();(BASE/'orchestrator.py').write_bytes(script)
cli=(Path('/mnt/d/Noetome/.artifacts/b06v-t02-live-scan-20261106/noetome-selected.exe') if MODE=='windows' else ROOT/'.artifacts/b06v-t02-scan-api-echo-fixed-final/noetome-linux')
cli_copy=SNAP/'target/debug'/('noetome.exe' if MODE=='windows' else 'noetome');cli_copy.parent.mkdir(parents=True);shutil.copyfile(cli,cli_copy);cli_copy.chmod(0o755)
cli_before=pin(cli_copy)
if MODE=='linux':
 legacy_cli=cli_copy.with_name('noetome.exe');shutil.copyfile(cli_copy,legacy_cli);legacy_cli.chmod(0o755);assert pin(legacy_cli)==cli_before
if MODE=='windows':
 winroot='D:\\Noetome\\.artifacts\\b06v-t02-continuation-final-20261106'
 wincli=winroot+'\\repo\\target\\debug\\noetome.exe'
 python='D:\\Noetome\\.artifacts\\b03-t02-python312\\Scripts\\python.exe'
 ps=BASE/'run-python.ps1'
 ps.write_text("$ErrorActionPreference='Stop'\n$env:PSExecutionPolicyPreference='Bypass'\n$env:PATH='C:\\Users\\Lyinn\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\native\\powershell;C:\\WINDOWS\\System32\\WindowsPowerShell\\v1.0;'+$env:PATH\n$env:NOETOME_PDF_PYTHON='"+python+"'\n$env:NOETOME_TEST_PYTHON='C:\\Users\\Lyinn\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\python.exe'\n$env:NOETOME_TEST_BINARY='"+wincli+"'\nRemove-Item Env:NOETOME_TANGMAO_API_KEY -ErrorAction SilentlyContinue\nSet-Location '"+winroot+"\\repo'\n& '"+python+"' -X utf8 -m unittest discover -s tests -p 'test_*.py' -v\nexit $LASTEXITCODE\n",encoding='utf-8')
 argv=['/mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',winroot+'\\run-python.ps1']
 env=None
else:
 argv=['/tmp/noetome-b05-t03-python/bin/python','-m','unittest','discover','-s','tests','-p','test_*.py','-v'];env=os.environ.copy();env['NOETOME_TEST_BINARY']=str(cli_copy);env.pop('NOETOME_TANGMAO_API_KEY',None)
start=time.monotonic()
with (BASE/'python.stdout.bin').open('xb') as out,(BASE/'python.stderr.bin').open('xb') as err:
 process=subprocess.Popen(argv,cwd=SNAP,env=env,stdout=out,stderr=err)
 save(BASE/'launch.json',dict(argv=argv,pid=process.pid,platform=MODE,selected_cli=cli_before,source_count=len(paths)))
 print(json.dumps(dict(platform=MODE,pid=process.pid,base=str(BASE))),flush=True)
 code=process.wait(timeout=900)
after={p:pin(SNAP/p) for p in paths};save(BASE/'source-after.json',after)
original_after={p:pin(ROOT/p) for p in paths};save(BASE/'worktree-after.json',original_after)
raw=(BASE/'python.stderr.bin').read_bytes();text=raw.decode('utf-8','strict');match=re.search(r'Ran (\d+) tests in ([\d.]+)s',text)
result=dict(platform=MODE,argv=argv,exit_code=code,wall_seconds=time.monotonic()-start,tests_run=int(match[1]) if match else None,summary=text[-1000:],skips=[line for line in text.splitlines() if ' ... skipped ' in line],source_count=len(paths),snapshot_unchanged=before==after,worktree_unchanged=before==original_after,cli_unchanged=cli_before==pin(cli_copy),cli=cli_before,stdout=pin(BASE/'python.stdout.bin'),stderr=pin(BASE/'python.stderr.bin'),source_archive=pin(BASE/'source.zip'),real_model_calls=0,scope='Frozen current source full Python; selected ordinary CLI, public synthetic local HTTP and synthetic DPAPI tests only')
save(BASE/'result.json',result);print(json.dumps(result,ensure_ascii=False),flush=True)
assert code==0 and result['snapshot_unchanged'] and result['worktree_unchanged'] and result['cli_unchanged']
if MODE=='windows':
 argv=['/mnt/d/Noetome/.artifacts/b03-t02-python312/Scripts/python.exe','-X','utf8',winroot+'\\repo\\scripts\\verify_scan_formula.py','--cli',wincli,'--output',winroot+'\\scan-audit']
 start=time.monotonic()
 with (BASE/'scan.stdout.bin').open('xb') as out,(BASE/'scan.stderr.bin').open('xb') as err: code=subprocess.run(argv,stdout=out,stderr=err,timeout=180).returncode
 scan=dict(argv=argv,exit_code=code,wall_seconds=time.monotonic()-start,stdout=pin(BASE/'scan.stdout.bin'),stderr=pin(BASE/'scan.stderr.bin'),real_model_calls=0)
 save(BASE/'scan-result.json',scan);print(json.dumps(scan),flush=True);assert code==0
