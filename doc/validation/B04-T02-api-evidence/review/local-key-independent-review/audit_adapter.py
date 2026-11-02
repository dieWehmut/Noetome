"""Independent offline review probe. Every credential and response is synthetic."""
import importlib.util
import json
import os
from pathlib import Path
import socket
import struct
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("independent_adapter", ROOT / "sidecars/responses_api/adapter.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
fixture = ROOT / "tests/fixtures/responses-api-synthetic-v1"
WIRE = (fixture / "request.json").read_bytes()
BASE = json.loads((fixture / "http-response.json").read_bytes())
SECRET = b"SYNTHETIC_REVIEW_KEY_20261102"
ESCAPED = "".join("\\u%04x" % char for char in SECRET)
FRAME = b"NOETOME-CREDENTIAL-STDIN\0" + struct.pack("<III", 1, len(SECRET), len(WIRE)) + SECRET + WIRE
ARTIFACTS = Path(__file__).resolve().parent


class Response:
    def __init__(self, raw, status=200):
        self.raw, self.status, self.length = raw, status, len(raw)

    def read1(self, maximum):
        result, self.raw = self.raw[:maximum], self.raw[maximum:]
        self.length -= len(result)
        return result


class Connection:
    def __init__(self, raw, status=200, error_stage=None):
        self.response = Response(raw, status)
        self.events, self.requests = [], []
        self.sock = self
        self.error_stage = error_stage

    def connect(self):
        self.events.append("connect")
        if self.error_stage == "connect":
            raise socket.timeout("synthetic transport detail suppressed")

    def settimeout(self, value):
        assert 0 < value <= 30

    def request(self, method, path, body=None, headers=None):
        self.events.append("request")
        self.requests.append((method, path, body))
        if self.error_stage == "send":
            raise socket.timeout("synthetic transport detail suppressed")

    def getresponse(self):
        self.events.append("response")
        if self.error_stage == "read":
            raise socket.timeout("synthetic transport detail suppressed")
        return self.response

    def close(self):
        self.events.append("close")


def run(raw, name, status=200, error_stage=None, frame=FRAME):
    connection = Connection(raw, status, error_stage)
    with tempfile.TemporaryDirectory(prefix="synthetic-", dir=ARTIFACTS) as directory:
        previous = os.getcwd()
        try:
            os.chdir(directory)
            outcome = adapter.run_live(frame, connection_factory=lambda **unused: connection)
            files = {p.name: p.read_bytes() for p in Path(directory).iterdir()}
        finally:
            os.chdir(previous)
    observation = json.loads(files["http-observation.json"])
    if name.startswith("quoted_scalar") or name.endswith("unclosed_token"):
        (ARTIFACTS / (name + ".synthetic.json")).write_bytes(raw)
    return {"case": name, "raw_saved": "http-response.bin" in files,
            "canonical": outcome is not None, "http_state": observation["http_state"],
            "failure": observation["failure"], "usage_retained": observation["usage_total_tokens"] is not None,
            "request_count": len(connection.requests)}


cases = {
    "literal_secret_control": SECRET.decode(),
    "object_closed_control": '{"value":"' + ESCAPED + '"}',
    "quoted_scalar_unicode": '"' + ESCAPED + '"',
    "quoted_scalar_nested": json.dumps('"' + ESCAPED + '"'),
    "object_unclosed_token": '{"value":"' + ESCAPED,
    "scalar_unclosed_token": '"' + ESCAPED,
    "prefixed_object": '```json\n{"value":"' + ESCAPED + '"}\n```',
    "secret_before_invalid_trailing_escape": '{"value":"' + ESCAPED + "\\u12",
}
rows = []
for name, text in cases.items():
    response = json.loads(json.dumps(BASE))
    response["output"][-1]["content"][0]["text"] = text
    raw = json.dumps(response, ensure_ascii=False, separators=(",", ":")).encode()
    rows.append(run(raw, name))
rows.append(run(json.dumps(BASE).encode()[:-1], "outer_truncated_control"))
for stage in ("connect", "send", "read"):
    rows.append(run(json.dumps(BASE).encode(), "timeout_" + stage, error_stage=stage))
for status in (401, 429, 302):
    rows.append(run(b'{"error":{"message":"synthetic status","code":null}}', "http_" + str(status), status=status))
for name, note in (("natural_quoted_path", r'"C:\vault\notes" is a quoted local path.'),
                   ("natural_inner_quotes", 'He said "hi" and then left.'),
                   ("natural_invalid_escape", r'Quoted "literal \q and \z" text.')):
    response = json.loads(json.dumps(BASE))
    response["metadata"] = {"note": note}
    rows.append(run(json.dumps(response, ensure_ascii=False).encode(), name))
emoji_secret = "SYNTHETIC_REVIEW_KEY_😀_20261102".encode()
utf16 = emoji_secret.decode().encode("utf-16-be")
emoji_escaped = "".join("\\u%04x" % int.from_bytes(utf16[n:n+2], "big") for n in range(0, len(utf16), 2))
response = json.loads(json.dumps(BASE))
response["output"][-1]["content"][0]["text"] = '"' + emoji_escaped + '"'
emoji_frame = b"NOETOME-CREDENTIAL-STDIN\0" + struct.pack("<III", 1, len(emoji_secret), len(WIRE)) + emoji_secret + WIRE
rows.append(run(json.dumps(response).encode(), "surrogate_pair_secret", frame=emoji_frame))
deep = '"' + ESCAPED + '"'
for unused in range(9):
    deep = json.dumps(deep)
response = json.loads(json.dumps(BASE))
response["output"][-1]["content"][0]["text"] = deep
rows.append(run(json.dumps(response).encode(), "over_depth_escape"))
(ARTIFACTS / "results.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(rows, ensure_ascii=False))
