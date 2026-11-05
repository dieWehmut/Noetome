from __future__ import annotations
import argparse, hashlib, json, os, socket, stat, subprocess, sys, time, uuid
from datetime import datetime, timezone
from pathlib import Path

def pin(path):
    data=path.read_bytes()
    return {'sha256':hashlib.sha256(data).hexdigest(),'byte_length':len(data)}

def write(path,value):
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def inventory(path):
    result={}
    def visit(item):
        assert not item.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,'no reparse traversal'
        if item.is_file():result[str(item)]=pin(item)
        else:
            for child in item.iterdir():visit(child)
    visit(path);return result

parser=argparse.ArgumentParser()
parser.add_argument('root',type=Path);parser.add_argument('case_name')
args=parser.parse_args();root=args.root
assert sys.platform=='win32' and sys.version_info[:3]==(3,12,14)
cfg=json.loads((root/'root.json').read_text(encoding='utf-8'))
final=json.loads((root/'native-final.json').read_text(encoding='utf-8'))
assert all(v==0 for v in final['required_latest_exits'].values())
assert json.loads((root/'native-ntfs-pointer.json').read_text(encoding='utf-8'))['status']=='passed'
base=Path(final['presentation_audit_root'])/args.case_name
assert base.parent==Path(final['presentation_audit_root']) and base.is_dir()
source=base/'preview-Vault'
plan=json.loads((source/'plan.json').read_text(encoding='utf-8'))
expected=json.loads((base/'expected.json').read_text(encoding='utf-8'))
report=json.loads((source/'presentation.json').read_text(encoding='utf-8'))
assert expected['expected_status']=='passed' and expected['expect_successful_preview']
assert plan['knowledge_status']=='pending' and plan['scientific_quality']=='not_run' and not plan['published']
before=inventory(base)
work=root/('obsidian-'+args.case_name+'-'+uuid.uuid4().hex[:12]);work.mkdir()
vault=work/'vault';profile=work/'profile';vault.mkdir();profile.mkdir()
notes={}
for note in plan['notes']:
    name=note['output_path']
    assert Path(name).name==name and name.endswith('.md') and not name.startswith('.')
    original=source/name;copied=vault/name;copied.write_bytes(original.read_bytes())
    assert pin(original)==pin(copied)
    notes[name]={'original':str(original),'copy':str(copied),**pin(original)}
(vault/'.obsidian').mkdir()
write(vault/'.obsidian/app.json',{'defaultViewMode':'preview'})
write(vault/'.obsidian/community-plugins.json',[])
vault_id=uuid.uuid4().hex[:16]
write(profile/'obsidian.json',{'updateDisabled':True,'vaults':{vault_id:{'path':str(vault),'ts':int(time.time()*1000),'open':True}}})
sock=socket.socket();sock.bind(('127.0.0.1',0));port=sock.getsockname()[1];sock.close()
argv=[cfg['obsidian_native'],'--user-data-dir='+str(profile),'--remote-debugging-port='+str(port),'--remote-debugging-address=127.0.0.1']
with (work/'obsidian.stdout.bin').open('xb') as out,(work/'obsidian.stderr.bin').open('xb') as err:
    proc=subprocess.Popen(argv,cwd=work,stdout=out,stderr=err)
assert inventory(base)==before
write(work/'original-protected-before.json',before)
write(work/'visual-input.json',{'case_name':args.case_name,'snapshot_head':final['head'],'plan':plan,'expected':expected,'presentation_report':report,'compiled_source':str(source),'compiled_source_notes':notes,'vault':str(vault),'profile':str(profile),'work':str(work),'wsl_work':cfg['wsl']+'/'+work.name,'port':port,'launcher':{'path':cfg['obsidian_native'],**pin(Path(cfg['obsidian_native']))},'scope':'actual generated notes copied to a new owned Vault; actual DOM/click/screenshot checks still pending'})
receipt={'pid':proc.pid,'argv':argv,'profile':str(profile),'vault':str(vault),'port':port,'work':str(work),'case_name':args.case_name,'started_utc':datetime.now(timezone.utc).isoformat(),'scope':'new owned native Obsidian launched; PID is a locator and not a completion claim'}
write(work/'launch.json',receipt);write(root/'active-obsidian.json',receipt)
print(json.dumps(receipt,ensure_ascii=False,indent=2),flush=True)
