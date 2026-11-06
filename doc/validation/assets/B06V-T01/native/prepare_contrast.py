"""Re-run the explicitly selected independent contrast counterexample.

This is supplementary to the immutable twelve-author benchmark. It uses the
prior independent closure's frozen source pins and the same note audit, without
adding an expected to the fixed twelve-author roster or bypassing its gate.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess
import sys


def pin(path):
    raw=path.read_bytes();return {"sha256":hashlib.sha256(raw).hexdigest(),"byte_length":len(raw)}


def save(path,value):
    with path.open("x",encoding="utf8") as f:json.dump(value,f,ensure_ascii=False,indent=2);f.write("\n")


p=argparse.ArgumentParser(description=__doc__)
for name in ("cli","checker","probe","closure","new_root"):p.add_argument(name,type=Path)
a=p.parse_args()
root=a.new_root;root.mkdir();source=root/"source"
shutil.copytree(a.probe/"source",source)
profile=root/"profile.json";profile.write_bytes((a.probe/"profile.json").read_bytes())
closure=json.loads(a.closure.read_bytes())
assert closure["status"]=="passed" and closure["independent_classification"]=="original_image/IMAGE_COLOR_CONTRAST"
assert {name:pin(source/name)["sha256"] for name in closure["source_pins"]}==closure["source_pins"]
(root/"prior-independent-closure.json").write_bytes(a.closure.read_bytes())
cli=root/a.cli.name;shutil.copyfile(a.cli,cli);cli.chmod(0o755)
checker_pin=pin(a.checker)
sys.path.insert(0,str(a.checker.resolve().parent))
import verify_image as v
case,semantics,saved_source=v.selected_source(source)
decision={"representation":"original_image","fallback_reasons":["IMAGE_COLOR_CONTRAST"],"issue_codes":[]}
output=root/"generated-Vault"
argv=[str(cli),"knowledge","image",str(source),str(profile),str(output)]
run=subprocess.run(argv,capture_output=True,timeout=120)
(root/"cli.stdout.bin").write_bytes(run.stdout);(root/"cli.stderr.bin").write_bytes(run.stderr)
save(root/"cli-receipt.json",{"argv":argv,"exit_code":run.returncode,"cli":pin(cli),"checker":checker_pin,"stdout":pin(root/"cli.stdout.bin"),"stderr":pin(root/"cli.stderr.bin")})
assert run.returncode==0,run.stderr.decode("utf8","replace")
actual=v.inventory(output);report=v.strict_json(actual["image.json"])
v.fields(report,v.REPORT_FIELDS)
for key,value in v.COMMON.items():v.exact(report[key],value,"supplementary manual source scope "+key)
v.uuid_string(report["request_id"])
for key,value in (("case",case),("semantics",semantics),("profile",v.strict_json(profile.read_bytes())),("decision",decision),("status","passed"),("preview_status","rendered")):
    v.exact(report[key],value,"independently selected contrast source and original fallback "+key)
saved_source["profile.json"]=profile.read_bytes()
for name,raw in saved_source.items():v.exact(actual[name],raw,"exact supplementary source bytes "+name)
v.exact(report["inputs"],v.pins(saved_source),"all supplementary source inputs")
v.exact(v.strict_json(actual[".owner.json"]),{"schema_version":1,"producer":"knowledge_image","request_id":report["request_id"],"purpose":"unpublished_image"},"supplementary owned output")
v.exact(set(actual),set(saved_source)|{".owner.json","image.json","image.md","plan.json","图像预览.md"},"no fake SVG or extra output")
v.exact(report["checks"],{key:"passed" for key in ("source_preservation","author_declaration","geometry_review","semantic_review","note_plan","rendering","saved_output")},"truthful supplementary checks")
plan=v.strict_json(actual["plan.json"])
v.exact(plan,{"schema_version":1,"producer":"knowledge_image","scope":"selected_image_fidelity_v1","request_id":report["request_id"],"case":case,"semantics":semantics,"decision":decision},"actual complete plan source meaning")
v.exact(report["plan_fingerprint"],v.fingerprint(actual["plan.json"]),"actual supplementary plan bytes")
assert len(report["files"])==1
f=report["files"][0];v.exact(f["relative_path"],"图像预览.md","original supplementary preview")
v.exact(f["content_sha256"],v.fingerprint(actual[f["relative_path"]])["sha256"],"supplementary actual note SHA")
v.exact(f["byte_length"],len(actual[f["relative_path"]]),"supplementary actual note length")
v.verify_note(actual["图像预览.md"],case,semantics,decision,f)
v.exact(report["artifacts"],v.pins({k:raw for k,raw in actual.items() if k not in ("image.json","image.md")}),"all supplementary saved bytes")
payload=re.findall(r'```json\n(.*?)\n```',actual["image.md"].decode(),re.S)
assert len(payload)==1
v.exact(v.strict_json(payload[0].encode()),report,"actual supplementary dual report equality")
assert pin(a.checker)==checker_pin
spec={"case":"contrast_original_dark","theme":"dark","scope":"supplementary frozen source contrast counterexample; actual final CLI original-image fallback; not part of twelve-author benchmark",
    "require_mathjax_runtime":False,"representation":"original_image","cli":pin(cli),"checker":checker_pin,"source_pins":{n:pin(source/n) for n in closure["source_pins"]},
    "notes":[{"file":"图像预览.md","math_trees":[],"required_texts":[case["title"],semantics["caption"],"Payload","mg","Dark","Selected payload"],"images":[{"file":"original.png","width":400,"height":250}],"link_destinations":["original.png"],"click_attachments":["original.png"]}],
    "prior_independent_closure":pin(a.closure),"supplementary_source_review":"passed"}
save(root/"capture-spec.json",spec)
save(root/"prepared.json",{"status":"passed","copied_cli":str(cli),"cli":pin(cli),"checker":checker_pin,"ordinary_cli_cases":1,"native_presentations":1,"cases":[{"case":spec["case"],"output":str(output),"capture_spec":str(root/"capture-spec.json"),"representation":"original_image","exit_code":0}]})
print(json.dumps({"status":"passed","root":str(root),"supplementary_source_review":"passed","cli":pin(cli)}))
