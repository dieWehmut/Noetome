"""Independent offline review: mocked transport and synthetic credentials only."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests"))
from test_responses_adapter import adapter, credential_frame, live_test, MockConnection, provider_response, response_raw, wire_request

def forbidden_network(*args, **kwargs):
    raise AssertionError("independent_review_forbids_real_network")

adapter.http.client.HTTPSConnection = forbidden_network
SECRET = b"SYNTHETIC_SAFE_RAW_REVIEW_ONLY_20261102"
ESCAPED = "".join("\\u%04x" % byte for byte in SECRET)
source = (ROOT / "sidecars/responses_api/adapter.py").read_bytes()
old_source = subprocess.check_output(["git", "show", "HEAD:sidecars/responses_api/adapter.py"], cwd=ROOT)
baseline = {"__file__": str(ROOT / "sidecars/responses_api/adapter.py"), "__name__": "independent_offline_baseline"}
exec(compile(old_source, baseline["__file__"], "exec"), baseline)
wire = wire_request()
assert adapter.build_http_body(wire) == baseline["build_http_body"](wire), "HTTP body changed"
assert adapter.OBSERVATION_KEYS == baseline["OBSERVATION_KEYS"], "Observation schema changed"

cases = []
def add(name, raw, safe, status=200, failure=None):
    cases.append((name, raw, safe, status, failure or ("invalid_response" if status == 200 else adapter.status_failure(status))))

for field in ("model", "id", "usage", "output"):
    value = provider_response()
    del value[field]
    add("safe_missing_" + field, response_raw(value), True)
for field, value in (("unknown_vendor", {"nested": ["natural \\alpha", 1, True, None]}),
                     ("temperature", "wrong type"), ("output", {"wrong": "type"}),
                     ("usage", {"input_tokens": True, "output_tokens": 30, "total_tokens": 31})):
    response = provider_response()
    response[field] = value
    add("safe_bad_envelope_" + field, response_raw(response), True)
for name, raw in (("null", b"null"), ("bool", b"true"), ("array", b"[]"),
                  ("scalar", b'"natural text"'), ("unknown_finite", b'{"future":1.25}')):
    add("safe_root_" + name, raw, True)
for status in (302, 401, 429, 408):
    add("safe_unknown_status_" + str(status), b'{"future":"synthetic diagnostic"}', True, status)
for kind, text in (("scalar", '"' + ESCAPED + '"'),
                   ("unclosed", '{"value":"' + ESCAPED),
                   ("codefence", '```json\n{"value":"' + ESCAPED + '"}\n```'),
                   ("nested", json.dumps(json.dumps('"' + ESCAPED + '"')))):
    add("unsafe_unknown_escaped_" + kind, response_raw({"future": text}), False)
add("unsafe_literal", response_raw({"future": SECRET.decode()}), False)
add("unsafe_escaped_key", ('{"' + ESCAPED + '":"x"}').encode(), False)
for name, raw in (("duplicate", b'{"future":1,"future":2}'),
                  ("nested_duplicate", b'{"future":{"x":1,"\\u0078":2}}'),
                  ("nonfinite", b'{"future":1e999}'), ("nan", b'{"future":NaN}'),
                  ("truncated", b'{"future":1'), ("invalid_utf8", b'{"future":"\xff"}'),
                  ("surrogate", b'{"future":"\\ud800"}'), ("empty", b"")):
    add("unsafe_" + name, raw, False)
add("unsafe_oversize", b"x" * 524289, False, failure="response_limit")

rows = []
for name, raw, safe, status, failure in cases:
    connection = MockConnection(raw=raw, status=status)
    outcome, files, connection = live_test(frame=credential_frame(secret=SECRET), connection=connection)
    obs = json.loads(files["http-observation.json"])
    assert outcome is None, name
    assert obs["raw_response_available"] == safe, name
    assert ("http-response.bin" in files) == safe, name
    if safe:
        assert files["http-response.bin"] == raw, name
    assert not obs["canonical_response_available"], name
    assert obs["failure"] == failure, name
    assert set(obs) == set(adapter.OBSERVATION_KEYS), name
    for field in ("response_id", "actual_model", "usage_input_tokens", "usage_output_tokens", "usage_total_tokens"):
        assert obs[field] is None, (name, field)
    assert len(connection.requests) == 1, name
    assert connection.events == ["connect", "request", "response", "close"], name
    assert files["http-request.json"] == adapter.build_http_body(wire), name
    rows.append({"case": name, "raw_available": safe, "canonical_available": False,
                 "failure": obs["failure"], "post_count": len(connection.requests),
                 "facts_null": True})

# A valid supported envelope/candidate remains accepted; the change did not turn
# invalid diagnostic envelopes into candidates or modify the positive contract.
outcome, files, connection = live_test(frame=credential_frame(secret=SECRET), connection=MockConnection())
assert outcome is not None
assert json.loads(files["http-observation.json"])["canonical_response_available"]
assert len(connection.requests) == 1
assert source == (ROOT / "sidecars/responses_api/adapter.py").read_bytes(), "Review target changed during probe"
result = {"synthetic_only": True, "real_network_forbidden": True,
          "adapter_sha256": hashlib.sha256(source).hexdigest(),
          "baseline_adapter_sha256": hashlib.sha256(old_source).hexdigest(),
          "body_unchanged": True, "observation_schema_unchanged": True,
          "cases_passed": len(rows), "valid_supported_response_still_accepted": True,
          "cases": rows}
output = Path(__file__).with_name("safe-raw-results.json")
with output.open("x", encoding="utf-8") as handle:
    json.dump(result, handle, indent=2)
    handle.write("\n")
print(json.dumps({"cases_passed": len(rows), "positive_passed": True,
                  "adapter_sha256": result["adapter_sha256"], "output": str(output)}))
