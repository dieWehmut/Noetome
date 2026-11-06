"""Link recorded native image captures to the final tested Windows program."""
import argparse
import hashlib
import json
from pathlib import Path


def read(p):return json.loads(Path(p).read_bytes())
def pin(p):
    raw=Path(p).read_bytes();return {"sha256":hashlib.sha256(raw).hexdigest(),"byte_length":len(raw)}


p=argparse.ArgumentParser(description=__doc__)
p.add_argument("windows_root",type=Path);p.add_argument("roots",type=Path);p.add_argument("new_summary",type=Path)
a=p.parse_args();final=read(a.windows_root/"native-final.json");roots=read(a.roots)
cli=final["cli"];rows=[];clicks=0;images=0;svg=[]
for label,value in roots["inputs"].items():
    prepared=read(Path(value)/"prepared.json")
    assert prepared["status"]=="passed" and prepared["cli"]==cli
    binary=Path(value)/Path(prepared["copied_cli"].replace('\\','/')).name
    assert pin(binary)==cli,"native input CLI bytes equal tested final PE"
for label,value in roots["captures"].items():
    root=Path(value);suite=read(root/"suite.json")
    assert suite["status"]=="passed"
    for entry in suite["cases"]:
        work=root/entry["case"];r=read(work/"result.json");protection=read(work/"protection-cleanup.json");spec=read(work/"capture-spec.json")
        assert entry["exit_code"]==0 and r["status"]=="passed"
        identity=r["identity"]
        assert identity["appVersion"]=='1.6.7' and identity["communityPlugins"]==[]
        assert protection["selected_originals_unchanged"] and protection["copied_originals_unchanged"] and protection["capture_spec_unchanged"]
        c=protection["cleanup"];assert c["ownedPidAbsent"] and c["ownedListenerAbsent"] and c["profileProcessCount"]==0
        clicks+=len(r["clicks"])
        count=sum(len(n["actual"]["images"]) for n in r["notes"]);images+=count
        row={"case":entry["case"],"status":"passed","theme":spec["theme"],"representation":spec["representation"],"embedded_images":count,"actual_original_png_clicks":len(r["clicks"]),"identity":identity,"owned_cleanup":c,"result":pin(work/"result.json"),"scope":spec["scope"]}
        for note in r["notes"]:
            if note.get("svgEvidence"):
                evidence=note["svgEvidence"];review=note["svgSourceReview"]
                assert evidence["status"]=='passed' and review["status"]=='passed'
                assert evidence["native"]["transparentPixelCount"]==0 and evidence["native"]["cornerRgba"]==[255,255,255,255]
                assert evidence["native"]["decodedRgbaSha256"]==evidence["auxiliaryRgbaSha256"]
                row["svg"]={"decoded_rgba_sha256":evidence["native"]["decodedRgbaSha256"],"source_svg_sha256":evidence["native"]["svgSha256"],"transparent_pixels":0,"semantic_review":review}
                svg.append(row)
        rows.append(row)
assert len(rows)==17 and images==12 and clicks==12 and len(svg)==6
for name in ['signed_arrows','spatial','escaped_labels']:
    pair=[r for r in svg if r["case"] in (name,name+'_dark')]
    assert len(pair)==2 and pair[0]["svg"]["decoded_rgba_sha256"]==pair[1]["svg"]["decoded_rgba_sha256"],"same exact SVG pixels under actual light and dark themes"
summary={"status":"passed","scope":"final ordinary Windows CLI plus frozen twelve-author independent checks; actual clean Obsidian 1.6.7; manual declarations, scientific quality not_run; supplementary contrast case kept separate",
    "tested_windows_cli":cli,"source_snapshot":pin(a.windows_root/"snapshot.json"),"ordinary_author_cli_cases":12,"supplementary_cli_cases":1,"native_presentations":17,
    "svg_presentations":6,"png_presentations":6,"blocked_presentations":5,"actual_original_png_clicks":clicks,"all_owned_processes_and_ports_absent":True,"all_selected_and_copied_originals_unchanged":True,
    "light_dark_svg_pixels_equal":True,"minimum_svg_label_contrast":min(r["svg"]["semantic_review"]["contrastReview"]["minimumContrastRatio"] for r in svg),"captures":rows,
    "not_run":["current stable/minimum version compatibility matrix","arbitrary image recognition/OCR","scientific accuracy","production Vault"]}
with a.new_summary.open('x',encoding='utf8') as f:json.dump(summary,f,ensure_ascii=False,indent=2);f.write('\n')
print(json.dumps({"status":"passed","native_presentations":17,"svg":6,"png":6,"blocked":5,"clicks":12,"cli":cli}))
