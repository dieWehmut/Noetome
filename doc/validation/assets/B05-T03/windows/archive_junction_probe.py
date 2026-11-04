import json,hashlib,zipfile,stat
from pathlib import Path
root=Path(__file__).parent
probe=root/'junction-probe-e03d84dfd1694c1a9a3b25b2f38abfb8'
summary=json.loads((probe/'probe-summary.json').read_bytes())
folder=Path(summary['selected_root']);repo=root/'repo'
entries={}
def add(name,path):
 st=path.lstat();assert stat.S_ISREG(st.st_mode) and not getattr(st,'st_file_attributes',0)&0x400
 raw=path.read_bytes();entries[name]=(raw,{'sha256':hashlib.sha256(raw).hexdigest(),'byte_length':len(raw)})
# Deliberately inspect only ordinary top-level probe files; never follow the junction.
for path in probe.iterdir():
 st=path.lstat()
 if stat.S_ISREG(st.st_mode) and not getattr(st,'st_file_attributes',0)&0x400:add('probe/'+path.name,path)
for name in ['windows_junction_probe.py','make_junction_probe.ps1','launch_junction_probe.ps1']:add('drivers/'+name,root/name)
protected=json.loads((probe/'protected-before.json').read_bytes())
for label,base in [('project',folder/'project'),('case',folder/'case'),('output',folder/'output'),('fixtures',repo/'fixtures/knowledge-coordination-v3')]:
 for name,item in protected[str(base)].items():
  if item['directory']:continue
  path=base/name
  assert hashlib.sha256(path.read_bytes()).hexdigest()==item['sha256']
  add('originals/'+label+'/'+name.replace('\\','/'),path)
add('originals/expected.json',folder/'expected.json')
archive=probe/'junction-probe-originals.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:
 for name,(raw,pin) in entries.items():z.writestr(name,raw)
with zipfile.ZipFile(archive) as z:
 assert z.testzip() is None
 for name,(raw,pin) in entries.items():assert z.read(name)==raw
receipt={'archive':str(archive),'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'byte_length':archive.stat().st_size,'members':{name:pin for name,(raw,pin) in entries.items()},'all_crc_and_member_bytes_verified':True,'junction_not_followed':True}
(probe/'junction-probe-archive.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'archive':str(archive),'members':len(entries),'byte_length':archive.stat().st_size}))
