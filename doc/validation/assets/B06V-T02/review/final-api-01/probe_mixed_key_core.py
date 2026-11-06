"""One synthetic numeric-loopback scan to check literal-escape credentials."""
from pathlib import Path
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
import shutil
import subprocess
import threading

REPO = Path(__file__).resolve().parents[6]
OUT = Path(__file__).resolve().parent / "mixed-key-core-red"
OUT.mkdir()
case = OUT / "selected-case"
shutil.copytree(REPO / "fixtures/scan-formula/cases/clear", case)
KEY = r"noetome-test-\n-abcdef"
suffix = "".join("\\u%04x" % ord(c) for c in "abcdef")
for _ in range(4):
    suffix = suffix.replace("\\", "\\\\")
echo = r"noetome-test-\n-" + suffix
response = json.dumps({"id": "synthetic-mixed-credential-only", "object": "chat.completion",
                      "created": 1, "model": echo,
                      "choices": [{"index": 0, "finish_reason": "stop", "message": {
                          "role": "assistant", "content": json.dumps({"schema_version": 1,
                          "recognized_tex": r"x_i = -\frac{a_i}{b_i}", "uncertainties": []})}}],
                      "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20}}).encode()
captured = []


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        assert self.path == "/v1/chat/completions"
        assert self.headers["Authorization"] == "Bearer " + KEY
        body = self.rfile.read(int(self.headers["Content-Length"]))
        captured.append(body)
        self.send_response(200)
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, *_):
        pass


server = HTTPServer(("127.0.0.1", 0), Handler)
server.timeout = 10
thread = threading.Thread(target=server.handle_request)
thread.start()
config = json.loads((REPO / "fixtures/scan-formula/api-profiles/authorized-stop.json").read_bytes())
config.update(transport_mode="local_http_test", endpoint="http://127.0.0.1:%d/v1/chat/completions" % server.server_port,
              model="noetome-test-mixed-key", credential_env="NOETOME_TEST_SCAN_CREDENTIAL")
config["budget"].update(max_requests=1, max_total_tokens=65536)
api = OUT / "api.json"
api.write_text(json.dumps(config, indent=2) + "\n")
binary = REPO / ".artifacts/b06v-t02-scan-api-final-01/noetome-linux"
argv = [str(binary), "knowledge", "scan-formula", str(case), str(api),
        str(REPO / "fixtures/scan-formula/profiles/native.json"), str(OUT / "new-vault")]
env = dict(os.environ)
env.pop("NOETOME_TANGMAO_API_KEY", None)
env["NOETOME_TEST_SCAN_CREDENTIAL"] = KEY
result = subprocess.run(argv, capture_output=True, env=env, timeout=130)
thread.join(timeout=12)
server.server_close()
assert not thread.is_alive() and len(captured) == 1
(OUT / "stdout.bin").write_bytes(result.stdout)
(OUT / "stderr.bin").write_bytes(result.stderr)
(OUT / "server-original-response.bin").write_bytes(response)
(OUT / "server-original-request-body.bin").write_bytes(captured[0])
report_path = OUT / "new-vault/scan.json"
report = json.loads(report_path.read_bytes()) if report_path.exists() else None
saved_response = OUT / "new-vault/response.bin"
receipt = {"schema_version": 1, "purpose": "public_fake_key_mixed_literal_escape_review",
           "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "argv": argv,
           "public_test_key": KEY, "encoded_echo_model_field": echo, "actual_exit_code": result.returncode,
           "required_issue_code": "SCAN_API_CREDENTIAL_ECHO", "actual_issue_codes": report["decision"]["issue_codes"] if report else None,
           "actual_status": report["status"] if report else None,
           "original_echo_response_persisted": saved_response.exists() and saved_response.read_bytes() == response,
           "actual_model": report["api"]["actual_model"] if report else None,
           "synthetic_server_post_count": len(captured), "real_model_calls": 0, "paid_calls": 0,
           "real_key_read": False, "current_ledger_read": False,
           "core_source_sha256": hashlib.sha256((REPO / "crates/noetome-cli/src/extraction_api.rs").read_bytes()).hexdigest()}
(OUT / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps({k: receipt[k] for k in ("actual_exit_code", "actual_status", "original_echo_response_persisted", "actual_issue_codes", "synthetic_server_post_count", "real_model_calls")}))
