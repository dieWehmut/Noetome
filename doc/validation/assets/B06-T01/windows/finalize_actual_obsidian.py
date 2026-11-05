from __future__ import annotations
import argparse,hashlib,json,stat,subprocess,sys
from pathlib import Path
def pin(path):
    body=path.read_bytes();return {'sha256':hashlib.sha256(body).hexdigest(),'byte_length':len(body)}
def inventory(root):
    result={}
    def visit(path):
        assert not path.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
        if path.is_file():result[str(path)]=pin(path)
        else:
            for child in path.iterdir():visit(child)
    visit(root);return result
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
parser=argparse.ArgumentParser();parser.add_argument('root',type=Path);parser.add_argument('work',type=Path)
args=parser.parse_args();root=args.root;work=args.work
assert sys.platform=='win32'
input=json.loads((work/'visual-input.json').read_text(encoding='utf-8'))
base=Path(input['compiled_source']).parent
before=json.loads((work/'original-protected-before.json').read_text(encoding='utf-8'))
after=inventory(base);assert after==before,'original compiled/author/source evidence changed'
copied={}
for name,metadata in input['compiled_source_notes'].items():
    expected={k:metadata[k] for k in ['sha256','byte_length']}
    copied[name]=pin(Path(metadata['copy']));assert copied[name]==expected
argv=['powershell.exe','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',str(root/'probe-own-obsidian-cleanup.ps1'),'-Work',str(work)]
value=subprocess.run(argv,capture_output=True,timeout=30)
(work/'cleanup-native.stdout.bin').write_bytes(value.stdout);(work/'cleanup-native.stderr.bin').write_bytes(value.stderr)
write(work/'cleanup-native-receipt.json',{'argv':argv,'exit_code':value.returncode,'stdout':pin(work/'cleanup-native.stdout.bin'),'stderr':pin(work/'cleanup-native.stderr.bin')})
assert value.returncode==0,'own Obsidian process/listener not confirmed closed; inspect actual raw probe and retry only read-only check'
cleanup=json.loads((work/'cleanup-native.json').read_text(encoding='utf-8'))
assert cleanup['owned_pid_absent'] and cleanup['owned_listener_absent']
write(work/'original-protected-after.json',after)
write(work/'final-protection-cleanup.json',{'snapshot_head':input['snapshot_head'],'case_name':input['case_name'],'original_protected_before_after_match':True,'copied_note_byte_pins_after':copied,'cleanup':cleanup,'scope':'Bytes and actual owned process/listener cleanup passed; rendered DOM/click tool originals required separately'})
print('OWN_OBSIDIAN_CLEANUP_AND_BYTES_PASSED',work)
