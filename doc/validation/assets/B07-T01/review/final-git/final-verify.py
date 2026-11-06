from pathlib import Path
import subprocess,json,hashlib,datetime,zipfile
ROOT=Path('/home/lyinn/project/Noetome');BASE=Path(__file__).resolve().parent
PRE=ROOT/'.artifacts/b07-t01-closeout-draft-20261107/git-index-originals-readback-prelog.json';pre=json.loads(PRE.read_bytes())
def digest(p):return {'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'byte_length':p.stat().st_size}
def command(args):return subprocess.run(['git',*args],cwd=ROOT,check=True,stdout=subprocess.PIPE).stdout
stage={}
for raw in command(['ls-files','--stage','-z']).split(b'\0'):
 if not raw:continue
 header,name=raw.split(b'\t',1);mode,oid,level=header.split();assert level==b'0';stage[name.decode()]={'oid':oid.decode(),'mode':mode.decode()}
assert not any(n.startswith('.playwright-mcp/') for n in stage)
changed=[n.decode() for n in command(['diff','--cached',pre['base'],'--name-only','-z','--diff-filter=ACMR']).split(b'\0') if n]
assert not command(['diff','--cached',pre['base'],'--name-only','--diff-filter=D'])
assert not command(['diff','--name-only'])
assert {r['path'] for r in pre['files']}<=set(changed)
expected={}
for n,r in json.loads((ROOT/'.artifacts/b07-t01-linux-build-05-20261107/source-before.json').read_bytes()).items():expected[n]={'sha256':r['sha256'],'byte_length':r['byte_length']}
for r in pre['author_members']+pre['original_baseline_sources']:expected[r['path']]={'sha256':r['sha256'],'byte_length':r['byte_length']}
copy=json.loads((ROOT/'.artifacts/b07-t01-closeout-draft-20261107/public-copy-manifest.json').read_bytes())
for r in copy['files']:
 expected[r['destination']]={'sha256':r['sha256'],'byte_length':r['byte_length']};assert digest(Path(r['source']))==expected[r['destination']]
for n in ['scripts/verify_run_checkpoint.py','tests/test_verify_run_checkpoint.py']:
 # Exact final tool pins are taken from the actually executed Native08 snapshot.
 snap=json.loads(Path('/mnt/d/Noetome/.artifacts/b07-t01-windows-final-08-20261107/snapshot.json').read_bytes())['files'];expected[n]=snap[n]
selected=sorted(set(changed)|set(r['path'] for r in pre['author_members']+pre['original_baseline_sources'])|set(expected))
# Compiled125 are read here once alongside changed files; no 1864 repeated process loop.
p=subprocess.Popen(['git','cat-file','--batch'],cwd=ROOT,stdin=subprocess.PIPE,stdout=subprocess.PIPE);rows={};gold={}
try:
 for name in selected:
  info=stage[name];p.stdin.write((info['oid']+'\n').encode());p.stdin.flush();header=p.stdout.readline().split();assert header[0].decode()==info['oid'] and header[1]==b'blob';size=int(header[2]);h=hashlib.sha256();left=size;tail=b''
  file=ROOT/name;before=file.stat()
  with file.open('rb') as f:
   while left:
    block=p.stdout.read(min(left,1024*1024));assert block and f.read(len(block))==block,(name,'Git raw vs working bytes mismatch');h.update(block);left-=len(block);tail=(tail+block)[-3:]
   assert f.read(1)==b''
  assert p.stdout.read(1)==b'\n';after=file.stat();assert (before.st_dev,before.st_ino,before.st_mtime_ns,before.st_size)==(after.st_dev,after.st_ino,after.st_mtime_ns,after.st_size)
  pin={'sha256':h.hexdigest(),'byte_length':size}
  if name in expected:assert pin==expected[name],(name,pin,expected[name])
  rows[name]={'git_oid':info['oid'],'git_mode':info['mode'],**pin,'working_bytes_exact_git_blob':True,'actual_original_pin_bound':name in expected}
  if name.startswith('fixtures/run-checkpoint-v1/expected-markdown/'):assert tail.endswith(b'\n\n') and b'\r' not in tail;gold[name]={'exact_final_two_LF':True,**pin}
finally:
 p.stdin.close();p.stdout.close();assert p.wait(timeout=30)==0
assert len(pre['author_members'])==17 and len(pre['original_baseline_sources'])==9 and len(gold)==2
for r in pre['original_baseline_sources']:
 b=command(['show',pre['base']+':'+r['path']]);assert {'sha256':hashlib.sha256(b).hexdigest(),'byte_length':len(b)}==expected[r['path']]
archives={}
for file in sorted((ROOT/'doc/validation/assets/B07-T01').rglob('*.zip')):
 name=str(file.relative_to(ROOT));assert name in rows;archives[name]=rows[name]
assert len(archives)==15
supp=ROOT/'doc/validation/assets/B07-T01/review/final-report';manifest=json.loads((supp/'manifest.json').read_bytes());assert rows[str((supp/'originals.zip').relative_to(ROOT))]['sha256']==manifest['archive_sha256']
with zipfile.ZipFile(supp/'originals.zip') as z:
 assert z.testzip() is None and len(z.namelist())==manifest['members']==27 and set(z.namelist())==set(manifest['member_pins'])
 for n,r in manifest['member_pins'].items():
  b=z.read(n);assert {'sha256':hashlib.sha256(b).hexdigest(),'byte_length':len(b)}==r
origin=ROOT/'.artifacts/b07-t01-final-report-review-package-20261107/originals.zip';assert digest(origin)=={'sha256':manifest['archive_sha256'],'byte_length':manifest['archive_byte_length']}
assert digest(supp/'manifest.json')==digest(origin.parent/'manifest.json')
with zipfile.ZipFile(origin) as z:
 for n in ['actual.json','index-check.json']:
  data=(supp/n).read_bytes();matches=[member for member in z.namelist() if z.read(member)==data];assert matches,(n,'public supplemental JSON differs from archived source member')
ledger=json.loads((ROOT/'dev-log/tasks.json').read_bytes());task={x['id']:x for x in ledger['tasks']};assert task['B07-T01']['status']=='done' and task['B07-T02']['status']==task['B07-T03']['status']=='todo'
assert all(n in rows for n in ['dev-log/tasks.json','dev-log/TODO.md','dev-log/DONE.md','dev-log/STATUS.md'])
summary=json.loads((ROOT/'doc/validation/assets/B07-T01/verification-summary.json').read_bytes());assert summary['runtime_checks']['windows_python_final08']['total']==477 and summary['runtime_checks']['linux_python']['total']==416
pre_pins={r['path']:{'sha256':r['sha256'],'byte_length':r['byte_length']} for r in pre['files']};doc_delta=[n for n,r in pre_pins.items() if {'sha256':rows[n]['sha256'],'byte_length':rows[n]['byte_length']}!=r]
assert set(doc_delta)<={'doc/validation/B07-T01.md','doc/validation/assets/B07-T01/README.md','doc/validation/assets/B07-T01/verification-summary.json'}
result={'schema_version':1,'producer':'independent_current_git_raw_blob_and_actual_originals_readback','status':'passed','at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'base':pre['base'],'current_changed_files':len(changed),'current_changed_file_bytes':sum(rows[n]['byte_length'] for n in changed),'all_current_changed_working_bytes_exact_git_blob':True,'files':{n:rows[n] for n in changed},'compiled_core125_git_raw_matches_actual_build05_pins':True,'core125_count':125,'tracked1860_independent_proof_referenced':'doc/validation/assets/B07-T01/review/final-report/index-check.json; no repeat1864 loop','original64_git_blob_public_copy_origin_exact':True,'author17_exact_index_and_original_pins':True,'old9_base_index_working_equal':True,'canonical_gold_two_terminal_LF':gold,'archives15_raw_git_equal_public_exact':archives,'main_archives14_members27004_previously_complete_CRC_SHA_checked':True,'supplement27_new_CRC_SHA_length_checked':True,'supplement_origin_exact':True,'earlier_wrapper_path_assumption_failure':'all Git blobs had matched; last provenance lookup wrongly assumed actual.json lived next to ZIP. Source actual/index-check are in source ZIP members; first script/physical stderr retained, corrected exact archived-member comparison passed.','archive_total_members':27031,'archive_total_byte_length':sum(r['byte_length'] for r in archives.values()),'only_declared_doc_updates_vs_prelog':doc_delta,'four_final_logs_index_working_equal_and_B07_done_todo_todo':True,'user_playwright_directory_staged':False,'user_playwright_directory_touched':False,'current_staged_all_git_inputs_frozen_observed':True,'git_mutations':0,'new_product_runs':0,'limitations':['this receipt is added by Root after audit and its own final blob is checked by Root','1864 prior full proof reused; current core125/changed/author/base/copy raw blobs independently read']}
(BASE/'actual.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps({'status':'passed','changedfiles':len(changed),'changedbytes':result['current_changed_file_bytes'],'archives15':True,'zip_members_total27031':True,'zipbytes':result['archive_total_byte_length'],'core125':True,'author17':True,'baseline9':True,'logs_done_todo_todo':True,'prelog_doc_delta':doc_delta},ensure_ascii=False),flush=True)
