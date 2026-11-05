from __future__ import annotations
import argparse, hashlib, json, os, stat, subprocess, sys, time, uuid
from pathlib import Path

def pin(path):
    body=path.read_bytes()
    return {'sha256':hashlib.sha256(body).hexdigest(),'byte_length':len(body)}

def write(path,value):
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def inventory(root):
    result={}
    def visit(path):
        metadata=path.lstat()
        assert not metadata.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT, 'protected inventory never follows reparse points'
        if path.is_file():result[str(path)]=pin(path)
        else:
            assert path.is_dir()
            for child in path.iterdir():visit(child)
    visit(root)
    return result

parser=argparse.ArgumentParser()
parser.add_argument('root',type=Path)
args=parser.parse_args();root=args.root
assert sys.platform=='win32' and sys.version_info[:3]==(3,12,14)
cfg=json.loads((root/'root.json').read_text(encoding='utf-8'))
final=json.loads((root/'native-final.json').read_text(encoding='utf-8'))
assert all(v==0 for v in final['required_latest_exits'].values())
repo=Path(cfg['repo_native']);cli=repo/'target/debug/noetome.exe'
assert pin(cli)==final['cli']
audit=Path(final['presentation_audit_root']);base=audit/'two_objects'
paths=[base/'project',base/'case',base/'coordination-output',base/'preview-Vault',base/'expected.json']
assert all(p.exists() for p in paths)
work=root/('ntfs-probe-'+uuid.uuid4().hex[:12]);work.mkdir()
protected=inventory(base);protected.update({str(cli):pin(cli),str(repo/'scripts/verify_knowledge_presentation.py'):pin(repo/'scripts/verify_knowledge_presentation.py')})
write(work/'protected-before.json',protected)
env=os.environ.copy();env.update(NOETOME_TEST_PYTHON=cfg['fake_python_native'],NOETOME_PDF_PYTHON=cfg['python_native'],NOETOME_TEST_BINARY=str(cli),PYTHONUTF8='1',TEMP=str(repo/'target/compiler-temp'),TMP=str(repo/'target/compiler-temp'))
commands=[]

def unchanged():
    current=inventory(base)
    current.update({str(cli):pin(cli),str(repo/'scripts/verify_knowledge_presentation.py'):pin(repo/'scripts/verify_knowledge_presentation.py')})
    assert current==protected, 'original protected input/output/author/source bytes or inventory changed'

def call(name,argv,expected,code_text=None):
    start=time.monotonic()
    value=subprocess.run([str(a) for a in argv],cwd=repo,env=env,capture_output=True,timeout=90)
    out=work/(name+'.stdout.bin');err=work/(name+'.stderr.bin')
    out.write_bytes(value.stdout);err.write_bytes(value.stderr)
    unchanged()
    receipt={'name':name,'argv':[str(a) for a in argv],'cwd':str(repo),'exit_code':value.returncode,'expected_exit':expected,'stderr_contains':code_text,'elapsed_seconds':time.monotonic()-start,'stdout':{'path':str(out),**pin(out)},'stderr':{'path':str(err),**pin(err)},'protected_before_after_match':True}
    commands.append(receipt);write(work/(name+'.json'),receipt);write(work/'commands.json',commands)
    assert value.returncode==expected, (name,value.returncode,value.stderr.decode('utf-8',errors='replace'))
    if code_text:assert code_text.encode() in value.stderr,(name,value.stderr)
    return value

def verify(name,selected,expected):
    return call(name,[cfg['python_native'],'-X','utf8',repo/'scripts/verify_knowledge_presentation.py','check',*selected],expected)

alias=work/'case-ancestor-junction'
own_link=None
terminal=False
try:
    verify('baseline-direct-before',paths,0)
    mklink=call('mklink-junction',['cmd.exe','/d','/c','mklink','/J',alias,base],0)
    metadata=alias.lstat()
    junction={'path':str(alias),'target':str(base),'file_attributes':metadata.st_file_attributes,'reparse_attribute':bool(metadata.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT),'python_isjunction':os.path.isjunction(alias),'creation_stdout':mklink.stdout.decode('utf-8',errors='replace')}
    write(work/'junction.json',junction)
    assert junction['reparse_attribute'] and junction['python_isjunction']
    for i,role in enumerate(['project','case','coordination','preview','expected']):
        changed=list(paths);changed[i]=alias/paths[i].name
        verify('python-junction-'+role,changed,1)
    changed=list(paths);changed[0]=str(alias)+'\\..\\'+alias.name+'\\project'
    verify('python-lexical-parent',changed,1)
    for role,index,exit_code,diagnostic in [('project',0,3,'COORD_SOURCE_REVIEW'),('case',1,2,'COORD_INPUT_PATH'),('coordination',2,2,'COORD_INPUT_PATH')]:
        selected=list(paths[:3]);selected[index]=alias/paths[index].name
        output=work/('rejected-'+role+'-output')
        call('cli-junction-'+role,[cli,'knowledge','present',*selected,output],exit_code,diagnostic)
        assert not output.exists()
    output=alias/'rejected-output-ancestor'
    call('cli-junction-output',[cli,'knowledge','present',*paths[:3],output],2,'PROJECT_UNSAFE_PATH')
    assert not (base/output.name).exists()
    output=str(alias)+'\\..\\'+alias.name+'\\rejected-parent-output'
    call('cli-lexical-parent-output',[cli,'knowledge','present',*paths[:3],output],2,'PROJECT_UNSAFE_PATH')
    assert not (base/'rejected-parent-output').exists()
    golds=list(paths[1].glob('*/gold-original.json'))
    assert golds,'saved source gold missing'
    original=golds[0];assert original.stat().st_nlink==1
    own_link=work/'self-owned-gold-hardlink.json'
    os.link(original,own_link)
    write(work/'hardlink.json',{'original':str(original),'alias':str(own_link),'same_file':os.path.samefile(original,own_link),'native_link_count':original.stat().st_nlink,'original_pin':pin(original),'alias_pin':pin(own_link)})
    assert original.stat().st_nlink==2 and os.path.samefile(original,own_link)
    verify('python-hardlinked-source-gold',paths,1)
    output=work/'rejected-hardlink-output'
    call('cli-hardlinked-source-gold',[cli,'knowledge','present',*paths[:3],output],2,'COORD_INPUT_PATH')
    assert not output.exists()
    own_link.unlink();own_link=None
    assert original.stat().st_nlink==1
    verify('baseline-direct-after',paths,0)
    unchanged();write(work/'protected-after.json',protected)
    terminal=True
finally:
    cleanup={}
    if own_link is not None and own_link.exists():
        own_link.unlink();cleanup['self_owned_hardlink_removed']=True
    if os.path.isjunction(alias):
        alias.rmdir();cleanup['only_self_owned_junction_removed']=True
    cleanup['junction_absent']=not alias.exists()
    cleanup['original_case_still_exists']=base.is_dir()
    unchanged();cleanup['protected_before_after_match']=True
    write(work/'cleanup.json',cleanup)
    write(work/'final.json',{'status':'passed' if terminal else 'failed','snapshot_head':final['head'],'native_platform':sys.platform,'python':sys.version,'work':str(work),'commands':commands,'junction':str(alias),'junction_cleanup':cleanup,'scope':'actual NTFS ancestor junction and hardlink admission checks; held-period and Obsidian evidence are separate'})
    write(root/'native-ntfs-pointer.json',{'work':str(work),'status':'passed' if terminal else 'failed'})
print('ACTUAL_NTFS_ADMISSION_PASSED',work,flush=True)
