"""Independent CLI audit; author-written local HTTP fake, zero live model calls."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

REPO = Path(__file__).resolve().parents[3]
ART = Path(__file__).resolve().parent
CLI = REPO / "target/debug/noetome"
PUBLIC_TEST_KEY = "noetome-test-public-credential-never-real"

def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

def digest(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}

def run(name, args, env=None):
    proc = subprocess.run([str(CLI), *map(str, args)], capture_output=True, env=env, timeout=30)
    (ART / f"{name}.stdout.bin").write_bytes(proc.stdout)
    (ART / f"{name}.stderr.bin").write_bytes(proc.stderr)
    dump(ART / f"{name}.command.json", {"command": [str(CLI), *map(str, args)], "exit_code": proc.returncode})
    return proc

def initial():
    root = Path(tempfile.mkdtemp(prefix="noetome-preview-audit-"))
    project = root / "author 中文 source"
    assert run("init", ["project", "init", project]).returncode == 0
    source = root / "作者恶意语法测试.txt"
    source.write_text("作者测试原文：10 m/s；条件是温度 20 ℃、仅适用样例 A。 %%SOURCE-HIDDEN%% <script>source</script> [[outside/path]] ![image](https://invalid.example/img.png) $2+2$ ~~strike~~ ==highlight==", encoding="utf-8")
    imported = run("import", ["source", "import", project, source])
    assert imported.returncode == 0, imported.stderr
    import_id = json.loads(imported.stdout)["import_id"]
    captured = []
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            body = self.rfile.read(int(self.headers["Content-Length"]))
            (ART / "actual-http-request.bin").write_bytes(body)
            assert self.headers["Authorization"] == f"Bearer {PUBLIC_TEST_KEY}"
            inp = json.loads(json.loads(body)["input"])
            unit = inp["units"][0]
            candidate = json.loads((REPO / "fixtures/extraction-gateway-v1/response.json").read_bytes())
            candidate.pop("producer")
            candidate["request_id"] = inp["request_id"]
            candidate["binding"] = inp["binding"]
            candidate["concepts"][0]["canonical_name"] = "作者测试概念\n- 注入列表\n---"
            candidate["concepts"][0]["aliases"] = ["别名\n第二行"]
            candidate["atoms"][0]["atom"]["content"] = "模型原样文本 $2+2$ ~~strike~~ ==highlight==\n~~~\nUNTRUSTED-FENCE"
            candidate["evidence"][0].update(unit_id=unit["unit_id"], excerpt_start_byte=unit["start_byte"], excerpt_end_byte=unit["end_byte"], excerpt_sha256=digest(unit["text"].encode())["sha256"], quote=unit["text"])
            response = {"id":"resp_noetome_audit","object":"response","model":"noetome-test-model","status":"completed","error":None,"incomplete_details":None,"service_tier":"default","output":[{"type":"message","role":"assistant","status":"completed","id":"msg_noetome_audit","content":[{"type":"output_text","text":json.dumps(candidate, ensure_ascii=False),"annotations":[],"logprobs":[]}]}],"usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":600,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":700}}
            data = json.dumps(response, ensure_ascii=False).encode()
            (ART / "author-local-http-response.bin").write_bytes(data)
            dump(ART / "author-candidate.json", candidate)
            captured.append(1)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        def log_message(self, *args):
            pass
    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.handle_request, daemon=True)
    config = root / "local-fake-config.json"
    dump(config, {"schema_version":1,"provider":"openai_responses","transport_mode":"local_http_test","endpoint":f"http://127.0.0.1:{server.server_port}/v1/responses","model":"noetome-test-model","credential_env":"NOETOME_TEST_CREDENTIAL","required_input":"text","capabilities":{"text":True,"structured_output":True,"vision":False},"selection":[{"kind":"text","ordinal":0}],"limits":{"max_units":1,"max_input_bytes":4096,"max_response_bytes":65536,"max_output_tokens":2048,"timeout_ms":10000},"budget":{"max_requests":1,"max_total_tokens":65536,"max_cost_usd":None}})
    api = root / "new api audit"
    thread.start()
    env = dict(os.environ, NOETOME_TEST_CREDENTIAL=PUBLIC_TEST_KEY)
    api_run = run("actual-local-http-api", ["extract", "api", project, import_id, config, api], env)
    thread.join(timeout=15)
    server.server_close()
    assert not thread.is_alive() and len(captured) == 1 and api_run.returncode == 0, api_run.stderr
    fixture = {"root":str(root),"project":str(project),"import_id":import_id,"api":str(api),"source":str(source),"live_paid_calls":0,"author_local_http_requests":1,"input_fingerprints":{name:digest((api/name).read_bytes()) for name in sorted(p.name for p in api.iterdir())}}
    dump(ART / "fixture.json", fixture)
    return fixture

def audit_existing():
    paths = json.loads((REPO / ".artifacts/B15-T02/core-preview/generated-outputs.json").read_bytes())["paths"]
    results = []
    for p in paths:
        p = Path(p)
        saved = json.loads((p / "reports/candidates.json").read_bytes())["payload"]
        request = json.loads((p / "reports/request.json").read_bytes())
        report = json.loads((p / "reports/preview.json").read_bytes())
        concept = (p / "vault/concept-0001.md").read_text(encoding="utf-8")
        evidence = (p / "vault/evidence-0001.md").read_text(encoding="utf-8")
        atom = saved["atoms"][0]
        assert saved["concepts"][0]["canonical_name"] in concept
        assert atom["atom"]["content"] in concept
        assert all(q in concept for q in atom["atom"]["qualifiers"])
        assert all(c in concept for c in atom["measurement"]["conditions"])
        assert saved["evidence"][0]["quote"] in evidence
        assert request["units"] == report["units"]
        assert report["published"] is False and report["scientific_quality"] == "not_run"
        attachment = p / "vault/attachments/source-0001.txt"
        assert digest(attachment.read_bytes())["sha256"] == request["binding"]["content_sha256"]
        if atom["measurement"]["value_status"] == "unknown":
            assert atom["measurement"]["value"] is None and atom["measurement"]["unit"] is None
            assert "数值：未知 / 不适用（null）" in concept and "单位：未知 / 不适用（null）" in concept
        else:
            assert f'数值：{atom["measurement"]["value"]}' in concept
            assert f'单位：{atom["measurement"]["unit"]}' in concept
        results.append({"path":str(p),"status":"passed","measurement":atom["measurement"],"attachment":digest(attachment.read_bytes()),"note_files":[q.name for q in (p/"vault").iterdir() if q.is_file()],"live_paid_calls":0})
    dump(ART / "known-unknown-semantics.json", results)

def export_probe(fixture, phase):
    api = Path(fixture["api"])
    assert fixture["input_fingerprints"] == {name:digest((api/name).read_bytes()) for name in sorted(p.name for p in api.iterdir())}, "changed original saved API fixture"
    before = {str(p.relative_to(fixture["project"])):digest(p.read_bytes()) for p in Path(fixture["project"]).rglob("*") if p.is_file()}
    dest = Path(fixture["root"]) / f"{phase} preview"
    out = run(phase, ["preview", "export", fixture["project"], fixture["import_id"], api, dest])
    assert out.returncode == 0, out.stderr
    note = (dest / "vault/concept-0001.md").read_text(encoding="utf-8")
    evidence = (dest / "vault/evidence-0001.md").read_text(encoding="utf-8")
    overview = (dest / "vault/待核验笔记.md").read_text(encoding="utf-8")
    faults = {"unescaped_model_fence":"\n~~~\n" in note,"unescaped_inline_math":"$2+2$" in note,"unescaped_strikethrough":"~~strike~~" in note,"unescaped_highlight":"==highlight==" in note,"multiline_generated_wiki_alias":"[[concept-0001.md|作者测试概念\n" in overview,"unescaped_source_hidden_comment":"%%SOURCE-HIDDEN%%" in evidence}
    after = {str(p.relative_to(fixture["project"])):digest(p.read_bytes()) for p in Path(fixture["project"]).rglob("*") if p.is_file()}
    assert before == after, "preview rewrote source project"
    assert (dest / "vault/attachments/source-0001.txt").read_bytes() == Path(fixture["source"]).read_bytes()
    assert "<script>" not in evidence and "[[outside/path]]" not in evidence and "![image]" not in evidence
    result = {"phase":phase,"cli_exit_code":out.returncode,"status":"RED" if any(faults.values()) else "GREEN","findings":faults,"output_root":str(dest),"cli_fingerprint":digest(CLI.read_bytes()),"same_saved_api_fingerprints":fixture["input_fingerprints"],"source_project_unchanged":True,"attachment_byte_identical":True,"live_paid_calls":0,"model":"explicit_author_local_http_fake"}
    dump(ART / f"{phase}.result.json", result)
    print(json.dumps(result, ensure_ascii=False))

if __name__ == "__main__":
    phase = sys.argv[1] if len(sys.argv) > 1 else "red"
    fixture = initial() if phase == "red" else json.loads((ART / "fixture.json").read_bytes())
    audit_existing()
    export_probe(fixture, phase)
