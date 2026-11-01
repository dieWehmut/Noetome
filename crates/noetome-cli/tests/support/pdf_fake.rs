//! Test-only Python failure adapters; extra_resource invokes the real parser.
use std::fs;
use std::path::{Path, PathBuf};

pub fn write_adapter(root: &Path, scenario: &str, real: Option<&Path>) -> PathBuf {
    let path = root.join(format!("test-{scenario}.py"));
    let body=match scenario {
        "exit_nonzero"=>"import sys\nsys.stdin.buffer.read()\nsys.stderr.write('PDF_TEST_NONZERO: actual test adapter failure\\n')\nsys.exit(7)\n".to_owned(),
        "extra_stdout"=>"import sys\nsys.stdin.buffer.read()\nsys.stdout.write('{} extra stdout\\n')\n".to_owned(),
        "stdout_limit"=>"import sys\nsys.stdin.buffer.read()\nsys.stdout.buffer.write(b'x'*100000)\nsys.stdout.buffer.flush()\n".to_owned(),
        "sleep"=>"import json,os,sys,time\nfrom pathlib import Path\nsys.stdin.buffer.read()\nPath('artifacts/handshake.bin').write_text(json.dumps({'pid':os.getpid()}),encoding='utf-8')\nsys.stderr.write('PDF_TEST_HANDSHAKE\\n')\nsys.stderr.flush()\ntime.sleep(60)\n".to_owned(),
        "extra_resource"=>{
            let real=serde_json::to_string(real.expect("real adapter path").to_str().unwrap()).unwrap();
            format!("# Test fault after genuine pdfplumber parsing, not a fake PDF response.\nimport runpy,sys\nfrom pathlib import Path\napi=runpy.run_path({real})\nrequest=api['read_request']()\nidentity=api['runtime_identity']()\nraw=api['read_input'](request)\nresponse,outputs=api['parse_document'](request,identity,raw)\nserialized=api['write_outputs'](request,response,outputs)\nPath('artifacts/unlisted.bin').write_bytes(b'actual undeclared file')\nsys.stdout.buffer.write(serialized+b'\\n')\nsys.stdout.buffer.flush()\n")
        },
        "resource_hardlink"=>{
            let real=serde_json::to_string(real.expect("real adapter path").to_str().unwrap()).unwrap();
            format!("# Test fault after genuine PDF parsing: only emitted resources need one link.\nimport os,runpy,sys\nfrom pathlib import Path\napi=runpy.run_path({real})\nrequest=api['read_request']()\nidentity=api['runtime_identity']()\nraw=api['read_input'](request)\nresponse,outputs=api['parse_document'](request,identity,raw)\nserialized=api['write_outputs'](request,response,outputs)\nos.link('artifacts/P0_text.txt',Path.cwd().parent/'resource-hardlink-alias.txt')\nsys.stdout.buffer.write(serialized+b'\\n')\nsys.stdout.buffer.flush()\n")
        },
        "runtime_mismatch_configured"=>{
            let real=serde_json::to_string(real.expect("real adapter path").to_str().unwrap()).unwrap();
            format!("# Test-only expected lock mismatch; invokes the genuine native runtime checker.\nimport runpy,sys\napi=runpy.run_path({real})\napi['RUNTIME']['python']='0.0.0'\nsys.exit(api['main']())\n")
        },
        _=>panic!("unknown test-only adapter scenario"),
    };
    assert!(!path.exists());
    fs::write(&path, body).unwrap();
    path
}
