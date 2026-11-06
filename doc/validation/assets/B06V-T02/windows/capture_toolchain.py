from pathlib import Path
import subprocess,json,hashlib,sys,platform
root=Path(__file__).resolve().parent
facts={"platform":sys.platform,"windows_release":platform.release(),"machine":platform.machine(),"python_executable":sys.executable,"python_version":sys.version,"commands":[]}
for label,args in [("cargo-version",["cargo","--version"]),("rustc-version",["rustc","-vV"]),("active-toolchain",["rustup","show","active-toolchain"])]:
 p=subprocess.run(args,cwd=root/"repo",capture_output=True,timeout=15)
 assert p.returncode==0
 for stream,raw in [("stdout",p.stdout),("stderr",p.stderr)]:
  path=root/(label+"."+stream+".bin")
  with path.open("xb") as handle:handle.write(raw)
 facts["commands"].append({"argv":args,"exit_code":p.returncode,"stdout":p.stdout.decode("utf8","replace"),"stderr":p.stderr.decode("utf8","replace")})
(root/"rust-toolchain-facts.json").write_text(json.dumps(facts,indent=2)+"\n")
print(json.dumps({"status":"captured","platform":facts["platform"]}))
