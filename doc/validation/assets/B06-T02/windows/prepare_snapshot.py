from __future__ import annotations
import argparse, hashlib, io, json, subprocess, tarfile
from pathlib import Path, PurePosixPath

def pin(data):
    return {'sha256': hashlib.sha256(data).hexdigest(), 'byte_length': len(data)}

def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')

parser=argparse.ArgumentParser()
parser.add_argument('head')
parser.add_argument('root', type=Path)
args=parser.parse_args()
root=args.root
cfg=json.loads((root/'root.json').read_text())
head=subprocess.check_output(['git','rev-parse','--verify',args.head+'^{commit}']).decode().strip()
assert subprocess.check_output(['git','rev-parse','HEAD']).decode().strip()==head, 'stable selected commit must be current HEAD'
raw_tree=subprocess.check_output(['git','ls-tree','-r','-z',head])
files={}
for entry in raw_tree.split(b'\0'):
    if not entry: continue
    details,name=entry.split(b'\t',1)
    mode,kind,blob=details.decode().split()
    name=name.decode('utf-8')
    assert mode in ('100644','100755') and kind=='blob', (mode,kind,name)
    data=subprocess.check_output(['git','cat-file','blob',blob])
    files[name]=pin(data)
required=['crates/noetome-cli/src/process_parser.rs','crates/noetome-cli/src/process_model.rs','crates/noetome-cli/src/process_flow.rs','crates/noetome-cli/tests/process_flow.rs','scripts/verify_process_flow.py','fixtures/process-flow-v1/authors.json']
assert all(name in files for name in required), 'process/flow source and all authors must be committed before final snapshot'
product_prefixes=('crates/','scripts/','sidecars/','tests/','fixtures/','apps/','schemas/')
current_deltas=[]
for name, expected in files.items():
    p=Path(name)
    if not p.is_file() or pin(p.read_bytes())!=expected:
        current_deltas.append(name)
assert not any(n.startswith(product_prefixes) or n in ('Cargo.toml','Cargo.lock','rust-toolchain.toml') for n in current_deltas), ('unstable product source',current_deltas)
repo=root/'repo'
assert not repo.exists(), 'never replace an existing snapshot'
repo.mkdir()
argv=['git','archive','--format=tar',head]
archive=subprocess.check_output(argv)
(root/'snapshot-git.tar').write_bytes(archive)
with tarfile.open(fileobj=io.BytesIO(archive),mode='r:') as source:
    for member in source:
        path=PurePosixPath(member.name)
        assert not path.is_absolute() and '..' not in path.parts and '.git' not in path.parts
        target=repo.joinpath(*path.parts)
        if member.isdir():target.mkdir(exist_ok=True)
        else:
            assert member.isfile() and member.name in files, member.name
            target.parent.mkdir(parents=True,exist_ok=True)
            body=source.extractfile(member).read()
            assert pin(body)==files[member.name]
            with target.open('xb') as out:out.write(body)
assert all(pin((repo/name).read_bytes())==expected for name,expected in files.items())
manifest={'head':head,'files':files,'all_snapshot_git_blob_bytes_match':True,'current_checkout_nonproduct_deltas':current_deltas,'archive':{'argv':argv,**pin(archive)},'scope':'stable native source snapshot; no test results yet'}
write(root/'snapshot.json',manifest)
cfg['kind']='B06-T02 stable native validation snapshot'
cfg['snapshot_head']=head
write(root/'root.json',cfg)
print('EXACT_NATIVE_SNAPSHOT',head,len(files),flush=True)
