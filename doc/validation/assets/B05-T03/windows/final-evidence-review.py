import hashlib,json,pathlib,re,subprocess,zipfile
repo=pathlib.Path('/home/lyinn/project/Noetome')
p=repo/'doc/validation/assets/B05-T03/windows'
out={'scope':'read-only repository archived Windows receipt review','errors':[]}
def fp(b): return {'sha256':hashlib.sha256(b).hexdigest(),'byte_length':len(b)}
def check(x,msg):
 if not x: out['errors'].append(msg)
def j(n): return json.loads((p/n).read_text('utf-8-sig'))
def bn(s): return s.replace('\\','/').split('/')[-1]
def streams(c,base):
 for k in ['stdout','stderr']:
  f=base/bn(c[k]['path']);check(f.exists(),'missing stream '+str(f));
  if f.exists(): check(fp(f.read_bytes())=={x:c[k][x] for x in ['sha256','byte_length']},'stream mismatch '+str(f))
arc={}
for name,mname in [('audit-v3-originals.zip','audit-v3-archive.json'),('junction/junction-probe-originals.zip','junction/junction-probe-archive.json')]:
 m=j(mname); b=(p/name).read_bytes(); check(fp(b)=={k:m[k] for k in ['sha256','byte_length']},'archive bytes mismatch '+name)
 with zipfile.ZipFile(p/name) as z:
  ns=z.namelist();check(len(ns)==len(set(ns)),'duplicate archive names '+name);check(set(ns)==set(m['members']),'manifest inventory mismatch '+name);check(z.testzip() is None,'CRC error '+name)
  check(all('\\' not in n and not n.startswith('/') and '..' not in pathlib.PurePosixPath(n).parts for n in ns),'unsafe member name '+name)
  for n,pin in m['members'].items(): check(fp(z.read(n))==pin,'member bytes mismatch '+name+':'+n)
 arc[name]={'members':len(ns),**fp(b)}
out['archives']=arc
native=j('native-final.json');check(native['tracked_files']==1323,'tracked count');check(native['tracked_before_after_match'],'snapshot mutated')
check(native['protected_before']==native['protected_after'] and len(native['protected_before'])==3 and all(x is None for x in native['protected_before'].values()),'selected production absence mismatch')
out['selected_production_paths']={'count':3,'before_after_null_absent':True}
out['native_commands']={c['name']:c['exit_code'] for c in native['commands']}
for c in native['commands']:
 check(j(c['name']+'.json')==c,'native command separate receipt mismatch '+c['name']);streams(c,p)
 for k in ['tracked_before_after_match','selected_production_before_after_match']:check(c[k],'native protection mismatch '+c['name'])
for name in ['fmt','workspace-check','workspace-tests-after-pipe','python-full','audit-v3']:check(out['native_commands'][name]==0,'mandatory gate '+name)
check(out['native_commands']['workspace-tests']==101,'initial failure not retained')
out['native_streams_verified']=len(native['commands'])*2
rust=(p/'workspace-tests-after-pipe.stdout.bin').read_bytes().decode('utf-8')
rs=[tuple(map(int,a)) for a in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',rust)]
check(len(rs)==18 and sum(a[0] for a in rs)==607 and all(a[1:]==(0,0) for a in rs),'Rust count')
out['rust']={'suite_count':len(rs),'passed':sum(a[0] for a in rs),'failed':sum(a[1] for a in rs),'ignored':sum(a[2] for a in rs)}
py=(p/'python-full.stderr.bin').read_bytes().decode('utf-8');check('Ran 157 tests' in py and 'OK (skipped=1)' in py,'Python full count');out['python']={'run':157,'passed':156,'skipped':1}
first=(p/'workspace-tests.stdout.bin').read_bytes().decode('utf-8');check('392 passed; 1 failed' in first and 'review_broken_pipe_notifies_controller_and_reaps_live_tree' in first,'initial failure stream')
a=j('audit-final.json');check(a['scenario_count']==55 and a['command_count']==172,'audit totals');check(len(a['scenarios'])==55 and len(a['calls'])==172,'audit inventories')
check(a['source_before']==a['source_after'] and len(a['source_before'])==100,'audit source equality');check(a['protected_before']==a['protected_after'] and len(a['protected_before'])==100,'audit input protection')
for n,pin in a['source_before'].items():check(fp((repo/n.replace('\\','/')).read_bytes())==pin,'current source differs '+n)
out['current_product_source_pins_verified']=100
with zipfile.ZipFile(p/'audit-v3-originals.zip') as z:
 check(z.read('audit.json')==(p/'audit-final.json').read_bytes(),'copied audit-final differs original')
 check(json.loads(z.read('source-pins.json'))==a['source_before'],'source pins archive differs')
 check(json.loads(z.read('source-pins-after.json'))==a['source_after'],'source after archive differs')
 for c in a['calls']:
  check(c['exit_code']==c['expected_exit'],'audit command exit '+c['name']);check(c['cli_before']==c['cli_after']==native['cli'],'audit CLI drift '+c['name'])
  for k in ['stdout','stderr']: check(fp(z.read(bn(c[k]['path'])))=={x:c[k][x] for x in ['sha256','byte_length']},'audit stream mismatch '+c['name'])
 check(len(j('audit-v3-archive.json')['omitted_cumulative_snapshots'])==227,'omitted snapshot count')
 check(all(re.fullmatch('audit-\\d+\\.json',n) for n in j('audit-v3-archive.json')['omitted_cumulative_snapshots']),'essential archive omitted')
 check(not(set(z.namelist())&set(j('audit-v3-archive.json')['omitted_cumulative_snapshots'])),'omitted snapshots unexpectedly members')
out['audit']={'scenarios':55,'commands':172,'streams_verified':344,'protected_groups_equal':100,'only_cumulative_snapshots_omitted':227}
jc=j('junction/commands.json');check(len(jc)==15,'junction command count');summary=j('junction/probe-summary.json');check(summary['actual_ntfs_reparse'] and summary['frozen_author_files']==44,'junction metadata summary')
for c in jc:
 check(c['exit_code']==c['expected_exit'],'junction unexpected '+c['name']);streams(c,p/'junction');check(j('junction/'+c['name']+'.json')==c,'junction separate command receipt '+c['name'])
before=j('junction/protected-before.json');after=j('junction/protected-after.json');check(before==after,'junction original mutated')
with zipfile.ZipFile(p/'junction/junction-probe-originals.zip') as z:
 for n in z.namelist():
  if n.startswith('probe/'):
   f=p/'junction'/n[6:];check(f.exists() and f.read_bytes()==z.read(n),'junction standalone probe member mismatch '+n)
  if n.startswith('drivers/'):
   f=p/n[8:];check(f.exists() and f.read_bytes()==z.read(n),'junction standalone driver mismatch '+n)
  if n.startswith('originals/fixtures/'):
   f=repo/'fixtures/knowledge-coordination-v3'/n[len('originals/fixtures/'):];check(f.exists() and f.read_bytes()==z.read(n),'junction frozen fixture mismatch '+n)
 for root,tree in before.items():
  if isinstance(tree,dict) and 'sha256' not in tree:
   stem='fixtures' if bn(root)=='knowledge-coordination-v3' else bn(root)
   for n,pin in tree.items():
    if isinstance(pin,dict) and not pin.get('directory',True):
     m='originals/'+stem+'/'+n.replace('\\','/');check(m in z.namelist(),'missing selected original '+m)
     if m in z.namelist():check(fp(z.read(m))=={k:pin[k] for k in ['sha256','byte_length']},'selected original changed '+m)
  elif isinstance(tree,dict) and 'sha256' in tree:
   m='originals/'+bn(root);check(m in z.namelist() and fp(z.read(m))==tree,'selected gold changed '+m)
out['junction']={'commands':15,'streams_verified':30,'frozen_author_files':44,'actual_native_reparse':True,'protected_before_after_equal':True}
s=j('snapshot.json');check(len(s['files'])==1323,'snapshot inventory count');check(s['head']==native['head'],'snapshot HEAD')
# Read commit blobs in one read-only Git batch; do not open nativeRoot or selected production paths.
paths=list(s['files']);proc=subprocess.run(['git','cat-file','--batch'],cwd=repo,input=''.join(s['head']+':'+n+'\n' for n in paths).encode(),stdout=subprocess.PIPE,check=True)
b=proc.stdout;pos=0
for n in paths:
 end=b.index(b'\n',pos);h=b[pos:end].split();size=int(h[-1]);raw=b[end+1:end+1+size];pos=end+2+size;check(fp(raw)==s['files'][n],'snapshot does not pin Git blob '+n)
check(pos==len(b),'Git batch remaining bytes')
out['snapshot_git_commit_bytes_verified']=1323
out['snapshot_current_deltas']=[n for n,pin in s['files'].items() if not(repo/n).is_file() or fp((repo/n).read_bytes())!=pin]
# Verify current Git cleaning cannot alter copied archived receipts or streams.
files=[f for f in p.rglob('*') if f.is_file()];clean=[]
for f in files:
 rel=str(f.relative_to(repo));a1=subprocess.run(['git','hash-object','--path='+rel,rel],cwd=repo,stdout=subprocess.PIPE,check=True).stdout.strip();a2=subprocess.run(['git','hash-object','--no-filters',rel],cwd=repo,stdout=subprocess.PIPE,check=True).stdout.strip()
 if a1!=a2:clean.append(rel)
out['git_clean_filter_byte_mismatches']=clean;check(not clean,'Git clean filters change artifact bytes')
out['artifact_files_git_clean_checked']=len(files)
out['verdict']='pass' if not out['errors'] else 'needs_correction'
path=pathlib.Path('/tmp/b05-t03-windows-review-receipt.json');path.write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(out,ensure_ascii=False,indent=2))
