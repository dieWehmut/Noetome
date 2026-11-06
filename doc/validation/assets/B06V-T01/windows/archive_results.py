"""Archive actual pinned Windows source and recorded regression originals."""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
import zipfile


def fp(raw):
    return {"sha256":hashlib.sha256(raw).hexdigest(),"byte_length":len(raw)}


def save(path,value):
    with path.open("x",encoding="utf8") as f:
        json.dump(value,f,ensure_ascii=False,indent=2)
        f.write("\n")


def alias_target(path):
    try:return {"target":os.readlink(path),"read_via":"os.readlink in "+sys.platform+"; target not followed"}
    except OSError as error:
        if os.name=='nt':return {"target":None,"read_via":"Windows reparse metadata unavailable; target not followed","error":str(error)}
        # WSL readlink returns EIO for some real NTFS junctions exercised by
        # the Windows tests. Ask the same selected Windows runtime for only
        # this explicit reparse entry's metadata; never read target contents.
        selected_mount=re.fullmatch(r'/mnt/([A-Za-z])/(.*)',str(path))
        # Resolve only the lexical drive prefix: wslpath itself tries to follow
        # the dangling junction and can fail with the same EIO.
        native_path=(selected_mount[1].upper()+':\\'+selected_mount[2].replace('/','\\')) if selected_mount else subprocess.check_output(["wslpath","-w",str(path.parent)],text=True).strip()+'\\'+path.name
        probe=subprocess.run(["/mnt/d/Noetome/.artifacts/b03-t02-python312/Scripts/python.exe","-c","import os,sys,json;print(json.dumps({'target':os.readlink(sys.argv[1])}))",native_path],capture_output=True)
        if probe.returncode==0:return {**json.loads(probe.stdout),"read_via":"selected Windows readlink metadata; target not followed","wsl_error":str(error)}
        return {"target":None,"read_via":"not_run; neither runtime followed the target","wsl_error":str(error),"native_exit":probe.returncode,"native_stderr":probe.stderr.decode('utf8','replace')}


def archive(path,files,manifest_path,scope):
    manifest={}
    with zipfile.ZipFile(path,"x",zipfile.ZIP_DEFLATED) as z:
        for name,p in sorted(files.items()):
            assert not p.is_symlink() and not (hasattr(p,'is_junction') and p.is_junction()),"explicitly selected ordinary validation files"
            raw=p.read_bytes();manifest[name]=fp(raw);z.writestr(name,raw)
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None and set(z.namelist())==set(manifest)
        for name in z.namelist():assert fp(z.read(name))==manifest[name]
    save(manifest_path,{"status":"passed","archive":fp(path.read_bytes()),"scope":scope,"archive_runtime":{"platform":sys.platform,"executable":sys.executable,"argv":sys.argv},"member_count":len(manifest),"members":manifest})


parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("root",type=Path)
parser.add_argument("new_artifact_directory",type=Path)
args=parser.parse_args()
root=args.root;out=args.new_artifact_directory
out.mkdir()
snapshot=json.loads((root/"snapshot.json").read_bytes())
sources={name:root/"repo"/name for name in snapshot["files"]}
assert all(fp(p.read_bytes())==snapshot["files"][name] for name,p in sources.items())
sources["snapshot.json"]=root/"snapshot.json"
sources["root.json"]=root/"root.json"
archive(out/"source-originals.zip",sources,out/"source-originals-manifest.json","exact tested current-worktree product/test sources and explicit snapshot config")
originals={}
aliases={}
excluded_binary_fingerprints={}
binary_suffixes={".exe",".dll",".obj",".o",".pdb",".lib",".exp",".ilk"}
for p in root.iterdir():
    if p.is_file() and p.suffix.lower() not in binary_suffixes:
        originals[p.name]=p
    elif p.is_file():excluded_binary_fingerprints[p.relative_to(root).as_posix()]=fp(p.read_bytes())
directories=[directory for directory in root.iterdir() if directory.is_dir() and directory.name!="repo"]
compiler_temp=root/"repo/target/compiler-temp"
if compiler_temp.is_dir():directories.append(compiler_temp)
for directory in directories:
    if directory.is_dir() and directory.name!="repo":
        for location,directory_names,file_names in os.walk(directory,followlinks=False):
            for name in directory_names[:]:
                p=Path(location)/name
                if p.is_symlink() or (hasattr(p,'is_junction') and p.is_junction()):
                    aliases[p.relative_to(root).as_posix()]={"kind":"directory_alias",**alias_target(p)}
                    directory_names.remove(name)
                elif name=="__pycache__":directory_names.remove(name)
            for name in file_names:
                p=Path(location)/name
                if p.is_symlink() or (hasattr(p,'is_junction') and p.is_junction()):aliases[p.relative_to(root).as_posix()]={"kind":"reparse_alias",**alias_target(p)}
                elif p.is_file() and p.suffix.lower() not in binary_suffixes:
                    originals[p.relative_to(root).as_posix()]=p
                elif p.is_file():excluded_binary_fingerprints[p.relative_to(root).as_posix()]=fp(p.read_bytes())
if aliases:
    alias_file=out/"runtime-aliases.json"
    save(alias_file,{"scope":"recorded test aliases; targets were not read by archive","aliases":aliases})
    originals["runtime-aliases.json"]=alias_file
excluded_file=out/"excluded-binary-fingerprints.json"
save(excluded_file,{"scope":"reproducible executable and compiler object/debug artifact bytes excluded from ZIP; exact recorded fingerprints","files":excluded_binary_fingerprints})
originals["excluded-binary-fingerprints.json"]=excluded_file
archive(out/"regression-originals.zip",originals,out/"regression-originals-manifest.json","actual commands, stdout/stderr, receipts, snapshots, runtime-temp originals and table/flow/formula/image audits; reproducible executable bytes excluded and pins retained")
print(json.dumps({"status":"passed","source_members":len(sources),"regression_members":len(originals)}))
