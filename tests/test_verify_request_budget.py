"""Independent accounting adversaries and real local HTTP/readonly I/O tests.

The event examples exercise the checker; they are not claimed product executions.
"""
import copy
import datetime as dt
import http.client
import json
import os
from pathlib import Path
from contextlib import closing
import sqlite3
import subprocess
import shutil
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_request_budget as audit

R = 8310
RUN_A = "10000000-0000-4000-8000-000000000001"
RUN_B = "10000000-0000-4000-8000-000000000002"
REQUEST_A = "20000000-0000-4000-8000-000000000001"
REQUEST_B = "20000000-0000-4000-8000-000000000002"
IDENTITY = {"platform": "unix", "device": 1, "inode": 2}
STAMP = {"identity": IDENTITY, "sha256": "a" * 64, "byte_length": 1, "modified": "1"}


def authority(tokens=65536, requests=16, opening=None):
    return {"schema_version": 1, "authority_id": "author-test", "global_limits": {"max_requests": requests, "max_total_tokens": tokens, "max_cost_usd": None}, "opening": opening or {"accounted_tokens": 0, "held_tokens": 0, "dispatched_requests": 0, "held_requests": 0}, "policy_sha256": "b" * 64}


def timestamp(milliseconds):
    value = dt.datetime(2026, 11, 7, tzinfo=dt.timezone.utc) + dt.timedelta(milliseconds=milliseconds)
    return value.isoformat(timespec="microseconds").replace("+00:00", "Z")


class Chain:
    def __init__(self, auth=None):
        self.authority = auth or authority()
        self.rows = []
        self.milliseconds = 0

    def append(self, event, milliseconds=None):
        if milliseconds is None:
            self.milliseconds += 1
        else:
            self.milliseconds = milliseconds
        previous = self.rows[-1][3] if self.rows else "0" * 64
        record = {"schema_version": 1, "authority_id": self.authority["authority_id"], "sequence": len(self.rows) + 1, "previous_sha256": previous, "at_utc": timestamp(self.milliseconds), "event": event}
        raw = audit.canonical(record)
        self.rows.append((len(self.rows) + 1, previous, raw, audit.sha(raw)))
        return self

    def register(self, run=RUN_A, tokens=65536, requests=16, units=("unit-a", "unit-b"), deadline=10000):
        spec = {"run_id": run, "run_root": str(Path(tempfile.gettempdir()).absolute() / ("checker-" + run)), "case_sha256": "c" * 64, "input_pins": {str(Path(tempfile.gettempdir()).absolute() / "author-input"): copy.deepcopy(STAMP)}, "run_limits": {"max_requests": requests, "max_total_tokens": tokens, "max_cost_usd": None}, "deadline_utc": timestamp(deadline), "retry_base_delay_ms": 100, "retry_max_delay_ms": 400, "units": [{"unit_id": u, "max_attempts": 3, "max_retry_tokens": 16620, "deadline_utc": timestamp(deadline)} for u in units]}
        return self.append({"kind": "run_registered", "spec": spec, "identity": {"root": IDENTITY, "parent": IDENTITY, "control": IDENTITY, "attempts": IDENTITY, "owner": copy.deepcopy(STAMP), "worker": {**copy.deepcopy(STAMP), "sha256": audit.sha(b""), "byte_length": 0}, "units": {u: IDENTITY for u in units}}})

    def prepare(self, request=REQUEST_A, unit="unit-a", run=RUN_A, attempt=1):
        key = {"run_id": run, "unit_id": unit, "attempt_no": attempt, "request_id": request}
        relative = f"attempts/{unit}/{attempt}"
        body = {"sha256": "d" * 64, "byte_length": 5238}
        metadata = {"sha256": "e" * 64, "byte_length": 77}
        plan = {"key": key, "attempt_root": relative, "request_body": body, "request_metadata": metadata, "input_reserved_tokens": 6262, "max_output_tokens": 2048, "reserved_tokens": R, "reservation_policy": "request_utf8_bytes_plus_1024_v1"}
        files = {".owner.json": copy.deepcopy(STAMP), "request-body.json": {**copy.deepcopy(STAMP), **body}, "request.json": {**copy.deepcopy(STAMP), **metadata}}
        identity = {"root": IDENTITY, "parent": IDENTITY, "directories": {"attempts": IDENTITY, f"attempts/{unit}": IDENTITY, relative: IDENTITY}}
        self.append({"kind": "prepared", "plan": plan, "identity": identity, "files": files})
        return key

    def may_send(self, key):
        self.append({"kind": "reserved", "key": key})
        return self.append({"kind": "may_send", "key": key})

    def settle(self, key, *, known=700, error="EXTRACT_RESPONSE_INVALID", status=200):
        transport = {"transport-receipt.json": copy.deepcopy(STAMP)}
        self.append({"kind": "transport_sealed", "key": key, "files": transport})
        receipt = {"report": {"sha256": "f" * 64, "byte_length": 33}, "response": {"sha256": "9" * 64, "byte_length": 123}, "network_attempts": 1, "http_status": status, "response_complete": True, "response_id": "same-response-id", "http_request_id": "same-http-id", "usage": {"input_tokens": 100, "output_tokens": known - 100, "total_tokens": known} if known is not None else None, "candidate_status": "failed" if error else "validated", "error_code": error, "elapsed_ms": 1, "price_usd": None}
        return self.append({"kind": "receipt_settled", "key": key, "receipt": receipt, "files": {"validation.json": {**copy.deepcopy(STAMP), **receipt["report"]}}})

    def ledger(self):
        return audit.reconstruct_events(self.authority, self.rows)


class IndependentLedgerTests(unittest.TestCase):
    def test_known700_local_timeout_cannot_be_retried_as_HTTP_timeout(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key)
        c.settle(key, known=700, error="EXTRACT_TIMEOUT")
        c.append({"kind": "retry_scheduled", "key": key, "schedule": {"error_code": "EXTRACT_TIMEOUT", "retry_at_utc": timestamp(1000)}})
        with self.assertRaisesRegex(audit.AuditError, "known usage or complete200"):
            c.ledger()

    def test_complete200_unknown_wire_cannot_use_timeout_flag_to_retry(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key)
        c.settle(key, known=None, error="EXTRACT_TIMEOUT", status=200)
        c.append({"kind": "retry_scheduled", "key": key, "schedule": {"error_code": "EXTRACT_TIMEOUT", "retry_at_utc": timestamp(1000)}})
        with self.assertRaisesRegex(audit.AuditError, "known usage or complete200"):
            c.ledger()

    def test_different_requests_same_response_sha_charge_independently(self):
        c = Chain().register()
        for request, unit in [(REQUEST_A, "unit-a"), (REQUEST_B, "unit-b")]:
            key = c.prepare(request=request, unit=unit)
            c.may_send(key); c.settle(key)
        total = c.ledger().totals()
        self.assertEqual(total["accounted_tokens"], 1400)
        self.assertEqual(total["dispatched_requests"], 2)
        self.assertEqual(total["held_tokens"], 0)

    def test_same_SHA_author_expected_requires_each_complete_response_and_charge(self):
        c = Chain().register()
        for request, unit in [(REQUEST_A, "unit-a"), (REQUEST_B, "unit-b")]:
            key = c.prepare(request=request, unit=unit); c.may_send(key); c.settle(key)
            c.append({"kind": "unit_finished", "run_id": RUN_A, "unit_id": unit, "outcome": "failed", "diagnostic": "fixed author invalid candidate"})
        c.append({"kind": "run_finished", "run_id": RUN_A, "outcome": "failed", "diagnostic": "isolated author failures"})
        case = next(case for case in audit.author_check()["cases"] if case["case_id"] == "same-response-sha-different-attempts")
        response = {"sha256": case["expected"]["same_response_sha256"], "byte_length": 850, "advertised_status": 200, "actual_response_flush": True}
        traffic = {"observed_HTTP_requests": 2, "response_originals": [copy.deepcopy(response), copy.deepcopy(response)]}
        audit.case_expected_check(case, c.ledger(), traffic)
        traffic["response_originals"][1]["sha256"] = "9" * 64
        with self.assertRaisesRegex(audit.AuditError, "identical complete response bytes"):
            audit.case_expected_check(case, c.ledger(), traffic)

    def test_unknown_retains_entire_original_reservation(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key)
        c.append({"kind": "unknown_retained", "key": key, "reason": "REQUEST_INTERRUPTED"})
        self.assertEqual(c.ledger().totals()["held_tokens"], R)
        self.assertEqual(c.ledger().totals()["dispatched_requests"], 1)

    def test_unknown_cannot_release_after_os_death(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key)
        c.append({"kind": "unknown_retained", "key": key, "reason": "REQUEST_INTERRUPTED"})
        c.append({"kind": "reserved_released", "key": key, "reason": "worker died"})
        with self.assertRaisesRegex(audit.AuditError, "before MaySend"): c.ledger()

    def test_reserved_not_sent_releases_slot_without_reusing_number(self):
        c = Chain().register(); key = c.prepare(); c.append({"kind": "reserved", "key": key})
        c.append({"kind": "reserved_released", "key": key, "reason": "proven never MaySend"})
        self.assertEqual(c.ledger().totals()["held_tokens"], 0)
        c.prepare(request=REQUEST_B, attempt=1)
        with self.assertRaisesRegex(audit.AuditError, "monotone"): c.ledger()

    def test_second_run_cannot_reset_global_opening(self):
        c = Chain(authority(tokens=9009, opening={"accounted_tokens": 700, "held_tokens": 0, "dispatched_requests": 1, "held_requests": 0})).register(run=RUN_A).register(run=RUN_B)
        key = c.prepare(run=RUN_B)
        c.append({"kind": "reserved", "key": key})
        with self.assertRaisesRegex(audit.AuditError, "global atomic"): c.ledger()

    def test_real_concurrent_logical_reservations_cannot_take_full_balance_twice(self):
        c = Chain(authority(tokens=9009)).register()
        a = c.prepare(); b = c.prepare(request=REQUEST_B, unit="unit-b")
        c.append({"kind": "reserved", "key": a})
        c.append({"kind": "reserved", "key": b})
        with self.assertRaisesRegex(audit.AuditError, "global atomic"): c.ledger()

    def test_known700_refund_leaves_R_minus_one_and_denies_next(self):
        c = Chain(authority(tokens=9009)).register()
        a = c.prepare(); c.may_send(a); c.settle(a)
        b = c.prepare(request=REQUEST_B, unit="unit-b")
        c.append({"kind": "reserved", "key": b})
        with self.assertRaisesRegex(audit.AuditError, "global atomic"): c.ledger()

    def test_success_does_not_refund_dispatched_request_count(self):
        c = Chain(authority(requests=1)).register()
        a = c.prepare(); c.may_send(a); c.settle(a)
        b = c.prepare(request=REQUEST_B, unit="unit-b")
        c.append({"kind": "reserved", "key": b})
        with self.assertRaisesRegex(audit.AuditError, "global atomic"): c.ledger()

    def test_run_limit_does_not_spend_other_independent_run_capacity(self):
        c = Chain().register(run=RUN_A, requests=1).register(run=RUN_B, units=("unit-b",))
        a = c.prepare(); c.may_send(a); c.settle(a)
        b = c.prepare(request=REQUEST_B, unit="unit-b", run=RUN_B); c.may_send(b)
        self.assertEqual(c.ledger().totals()["dispatched_requests"], 2)

    def test_same_run_second_request_is_refused_even_with_global_capacity(self):
        c = Chain().register(requests=1)
        a = c.prepare(); c.may_send(a); c.settle(a)
        b = c.prepare(request=REQUEST_B, unit="unit-b"); c.append({"kind": "reserved", "key": b})
        with self.assertRaisesRegex(audit.AuditError, "Run atomic"): c.ledger()

    def test_duplicate_settlement_rehashed_chain_still_rejected(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key)
        event = copy.deepcopy(audit.strict_json(c.rows[-1][2])["event"])
        c.append(event)
        with self.assertRaisesRegex(audit.AuditError, "duplicated"): c.ledger()

    def test_duplicate_may_send_rehashed_chain_still_rejected(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key)
        c.append({"kind": "may_send", "key": key})
        with self.assertRaisesRegex(audit.AuditError, "exactly once"): c.ledger()

    def test_bad_candidate_known_usage_remains_known(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key, error="EXTRACT_RESPONSE_INVALID")
        ledger = c.ledger()
        self.assertEqual(ledger.attempts[tuple(key.values())]["state"], "known")
        self.assertEqual(ledger.totals()["accounted_tokens"], 700)

    def test_success_requires_validated_last_candidate(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key)
        c.append({"kind": "unit_finished", "run_id": RUN_A, "unit_id": "unit-a", "outcome": "succeeded", "diagnostic": "false success"})
        with self.assertRaisesRegex(audit.AuditError, "success requires"): c.ledger()

    def test_retry_whitelist_does_not_include_candidate_failure(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key)
        c.append({"kind": "retry_scheduled", "key": key, "schedule": {"retry_at_utc": timestamp(1000), "error_code": "EXTRACT_RESPONSE_INVALID"}})
        with self.assertRaisesRegex(audit.AuditError, "fixed whitelist"): c.ledger()

    def test_http_retry_backoff_persists_and_cannot_be_advanced(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key, known=None, status=429, error="EXTRACT_API_HTTP_RATE_LIMIT")
        c.append({"kind": "retry_scheduled", "key": key, "schedule": {"retry_at_utc": timestamp(1000), "error_code": "EXTRACT_API_HTTP_RATE_LIMIT"}})
        retry = c.prepare(request=REQUEST_B, attempt=2)
        c.append({"kind": "reserved", "key": retry}, milliseconds=999)
        with self.assertRaisesRegex(audit.AuditError, "before durable backoff"): c.ledger()

    def test_explicit_interruption_only_without_final_report(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key, known=None, status=429, error="EXTRACT_API_HTTP_RATE_LIMIT")
        c.append({"kind": "retry_scheduled", "key": key, "schedule": {"retry_at_utc": timestamp(1000), "error_code": "REQUEST_INTERRUPTED"}})
        with self.assertRaisesRegex(audit.AuditError, "fixed whitelist"): c.ledger()

    def test_explicit_interruption_fresh_retry_keeps_original_R(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key)
        c.append({"kind": "unknown_retained", "key": key, "reason": "REQUEST_INTERRUPTED"})
        c.append({"kind": "retry_scheduled", "key": key, "schedule": {"retry_at_utc": timestamp(1000), "error_code": "REQUEST_INTERRUPTED"}})
        retry = c.prepare(request=REQUEST_B, attempt=2)
        c.append({"kind": "reserved", "key": retry}, milliseconds=1000)
        self.assertEqual(c.ledger().totals()["held_tokens"], 2 * R)

    def test_new_attempt_after_original_deadline_rejected(self):
        c = Chain().register(deadline=1000); key = c.prepare()
        c.append({"kind": "reserved", "key": key}, milliseconds=1000)
        with self.assertRaisesRegex(audit.AuditError, "durable deadline"): c.ledger()

    def test_unknown_schema_and_extra_event_fields_refused(self):
        c = Chain().register(); event = copy.deepcopy(audit.strict_json(c.rows[0][2]))
        event["schema_version"] = 2
        raw = audit.canonical(event)
        with self.assertRaises(audit.AuditError): audit.reconstruct_events(c.authority, [(1, "0" * 64, raw, audit.sha(raw))])
        c = Chain().register(); key = c.prepare(); c.append({"kind": "reserved", "key": key, "refunded": True})
        with self.assertRaisesRegex(audit.AuditError, "exact fields"): c.ledger()

    def test_original_raw_utf8_hash_is_not_reserialized_hash(self):
        c = Chain().register(); seq, prev, raw, hashed = c.rows[0]
        changed = raw.replace(b'"schema_version":1', b'"schema_version": 1', 1)
        with self.assertRaisesRegex(audit.AuditError, "original UTF8"): audit.reconstruct_events(c.authority, [(seq, prev, changed, hashed)])
        with self.assertRaisesRegex(audit.AuditError, "canonical event"): audit.reconstruct_events(c.authority, [(seq, prev, changed, audit.sha(changed))])

    def test_unknown_price_and_overflow_are_not_zero(self):
        a = authority(); a["global_limits"]["max_cost_usd"] = "1"
        with self.assertRaisesRegex(audit.AuditError, "null"): audit.Ledger(a)
        with self.assertRaises(audit.AuditError): audit.add(audit.U64, 1)
        with self.assertRaises(audit.AuditError): audit.natural(True, "bool counter")

    def test_actual_provider_overrun_recorded_exact_and_stops_new_dispatch(self):
        c = Chain().register(); key = c.prepare(); c.may_send(key); c.settle(key, known=9000)
        self.assertEqual(c.ledger().totals()["accounted_tokens"], 9000)
        other = c.prepare(request=REQUEST_B, unit="unit-b"); c.append({"kind": "reserved", "key": other})
        with self.assertRaisesRegex(audit.AuditError, "after stop"): c.ledger()


class WireAndLocalHttpTests(unittest.TestCase):
    def test_frozen_author_members_and_request_byte_windows(self):
        self.assertEqual(audit.author_check()["case_count"], 14)
        raw = (audit.FIXTURE / "originals/core05-request-body-a.json").read_bytes()
        self.assertEqual(audit.request_body_check(raw)["units"][0]["end_byte"], 140)
        changed = raw.replace(b'"store":false', b'"store":true ')
        with self.assertRaises(audit.AuditError): audit.request_body_check(changed)

    def test_complete_metadata_bad_candidate_still_known700(self):
        raw = (audit.FIXTURE / "wire/fixed-200-invalid-candidate.bin").read_bytes()
        self.assertEqual(audit.usage_from_wire(raw, status=200, complete=True)["total_tokens"], 700)

    def test_non200_and_incomplete_valid_json_do_not_free_reservation(self):
        raw = (audit.FIXTURE / "wire/fixed-200-invalid-candidate.bin").read_bytes()
        self.assertIsNone(audit.usage_from_wire(raw, status=503, complete=True))
        self.assertIsNone(audit.usage_from_wire(raw, status=200, complete=False))

    def test_invalid_usage_sum_duplicate_bool_and_trailing_json_unknown(self):
        raw = (audit.FIXTURE / "wire/fixed-200-invalid-candidate.bin").read_bytes()
        value = audit.strict_json(raw); value["usage"]["total_tokens"] = 1
        self.assertIsNone(audit.usage_from_wire(audit.canonical(value), status=200, complete=True))
        value = audit.strict_json(raw); value["usage"]["input_tokens"] = True
        self.assertIsNone(audit.usage_from_wire(audit.canonical(value), status=200, complete=True))
        self.assertIsNone(audit.usage_from_wire(raw + b" {}", status=200, complete=True))
        duplicate = raw.replace(b'"total_tokens":700', b'"total_tokens":700,"total_tokens":700')
        self.assertIsNone(audit.usage_from_wire(duplicate, status=200, complete=True))

    def test_false_framing_omission_and_missing_nullable_rejected(self):
        raw = b"{}"
        t = {"schema_version": 1, "producer": "synthetic_api_transport_receipt", "request_id": REQUEST_A, "http_status": 200, "http_request_id": None, "response_complete": True, "network_attempts": 1, "response": {"sha256": audit.sha(raw), "byte_length": len(raw)}, "response_file": "response.bin", "error_code": None, "omission_reason": None, "elapsed_ms": 1}
        self.assertEqual(audit.transport_check(t, raw, REQUEST_A)["network_attempts"], 1)
        t.pop("omission_reason")
        with self.assertRaises(audit.AuditError): audit.transport_check(t, raw, REQUEST_A)

    def test_real_http_fixed_response_preserves_frozen_bytes(self):
        raw = (audit.FIXTURE / "originals/core05-request-body-a.json").read_bytes()
        bound = audit.strict_json(audit.strict_json(raw)["input"])
        key = (RUN_A, "unit-a", 1, bound["request_id"])
        with tempfile.TemporaryDirectory(prefix="预算 HTTP 中文 ") as temporary:
            with audit.ArtificialHttp(Path(temporary), audit.FIXTURE, {bound["binding"]["import_id"]: "unit-a"}, {"unit-a": ["fixed-invalid-candidate"]}, lambda _: key) as server:
                connection = http.client.HTTPConnection("127.0.0.1", server.server.server_port, timeout=3)
                connection.request("POST", "/v1/responses", raw, {"Authorization": "Bearer " + audit.TEST_KEY, "Content-Type": "application/json"})
                response = connection.getresponse(); body = response.read(); connection.close()
                self.assertEqual(response.status, 200)
                self.assertEqual(body, (audit.FIXTURE / "wire/fixed-200-invalid-candidate.bin").read_bytes())
            self.assertFalse(server.errors)
            self.assertEqual(len(server.receipts), 1)
            self.assertTrue((Path(temporary) / "server-receipts.json").is_file())

    def test_real_http_success_uses_original_author_candidate(self):
        raw = (audit.FIXTURE / "originals/core05-request-body-a.json").read_bytes()
        bound = audit.strict_json(audit.strict_json(raw)["input"])
        with tempfile.TemporaryDirectory() as temporary:
            with audit.ArtificialHttp(Path(temporary), audit.FIXTURE, {bound["binding"]["import_id"]: "unit-a"}, {"unit-a": ["success"]}, lambda _: (RUN_A, "unit-a", 1, bound["request_id"])) as server:
                connection = http.client.HTTPConnection("127.0.0.1", server.server.server_port, timeout=3)
                connection.request("POST", "/v1/responses", raw, {"Authorization": "Bearer " + audit.TEST_KEY})
                response = connection.getresponse(); wire = audit.strict_json(response.read()); connection.close()
                audit.candidate_gold(audit.strict_json(wire["output"][0]["content"][0]["text"]), bound, audit.FIXTURE)
            self.assertFalse(server.errors)

    def test_real_incoming_HTTP_is_recorded_even_when_UUID_router_rejects_body(self):
        raw = (audit.FIXTURE / "originals/core05-request-body-a.json").read_bytes()
        with tempfile.TemporaryDirectory() as temporary:
            with audit.ArtificialHttp(Path(temporary), audit.FIXTURE, {}, {"unit-a": ["success"]}, lambda _: None) as server:
                connection = http.client.HTTPConnection("127.0.0.1", server.server.server_port, timeout=1)
                connection.request("POST", "/v1/responses", raw, {"Authorization": "Bearer " + audit.TEST_KEY})
                with self.assertRaises((http.client.RemoteDisconnected, TimeoutError)):
                    connection.getresponse()
                connection.close()
            self.assertEqual(len(server.incoming), 1)
            self.assertTrue(server.incoming[0]["body_complete"])
            self.assertTrue(server.errors)
            self.assertEqual(len(server.receipts), 0)
            record = audit.strict_json((Path(temporary) / "server-receipts.json").read_bytes())
            self.assertEqual(record["observed_HTTP_request_count"], 1)
            self.assertTrue(all(not thread.is_alive() for thread in server.handler_threads))

    def test_real_sqlite_uri_chinese_space_readonly_preserves_database(self):
        with tempfile.TemporaryDirectory(prefix="预算 中文 ") as temporary:
            root = Path(temporary); database = root / "预算 空格.sqlite"
            with closing(sqlite3.connect(database)) as db, db: db.execute("CREATE TABLE author(value INTEGER)"); db.execute("INSERT INTO author VALUES(7)")
            before = audit.observed_stamp(database)[0]
            with closing(sqlite3.connect(audit.paths.sqlite_file_uri(database), uri=True)) as db, db:
                self.assertEqual(db.execute("SELECT value FROM author").fetchone()[0], 7)
                with self.assertRaises(sqlite3.OperationalError): db.execute("INSERT INTO author VALUES(8)")
            self.assertEqual(audit.observed_stamp(database)[0], before)

    @unittest.skipUnless(os.name == "nt", "actual NTFS verbatim namespace requires native Windows")
    def test_native_windows_verbatim_uri_actual_same_identity(self):
        with tempfile.TemporaryDirectory(prefix="预算 中文 空格 ") as temporary:
            ordinary = Path(temporary) / "test.sqlite"
            with closing(sqlite3.connect(ordinary)) as db, db: db.execute("CREATE TABLE author(value INTEGER)")
            canonical = audit.paths.canonical_selected_path(ordinary)
            self.assertEqual(audit.paths.actual_identity(ordinary), audit.paths.actual_identity(canonical))
            with closing(sqlite3.connect(audit.paths.sqlite_file_uri(canonical), uri=True)) as db, db:
                self.assertEqual(db.execute("SELECT count(*) FROM author").fetchone()[0], 0)
                with self.assertRaises(sqlite3.OperationalError): db.execute("INSERT INTO author VALUES(1)")


class DriverDecoderTests(unittest.TestCase):
    def row(self):
        return {"producer": "request_run_checkpoint_test_handshake", "event": "api_run_created_commit", "unit": "", "attempt": 0, "run_id": RUN_A, "pid": os.getpid(), "compiler_fingerprint": "1" * 64, "thread_id": "ThreadId(2)", "sequence": 1, "authority_id": "example-authority", "root": str(Path(tempfile.gettempdir()).absolute()), "will_wait": True}

    def test_decoder_requires_exact_pid_fp_and_literal_fields(self):
        row = self.row()
        self.assertEqual(audit.validate_handshake(row, pid=os.getpid(), fingerprint="1" * 64), row)
        for mutate in [lambda r: r.update(pid=os.getpid() + 1), lambda r: r.update(compiler_fingerprint="2" * 64), lambda r: r.update(extra=True), lambda r: r.update(will_wait=1), lambda r: r.update(sequence=True), lambda r: r.update(thread_id="guessed-worker")]:
            bad = self.row(); mutate(bad)
            with self.assertRaises(audit.AuditError): audit.validate_handshake(bad, pid=os.getpid(), fingerprint="1" * 64)


class ActualSourceAndBudgetFileTests(unittest.TestCase):
    """Real source CLI + synthetic audit files, never a product budget pass claim."""
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get("NOETOME_TEST_BINARY") or str(audit.ROOT / "target/debug" / ("noetome.exe" if os.name == "nt" else "noetome"))
        cls.binary = Path(executable)
        if not cls.binary.is_file():
            raise unittest.SkipTest("actual ordinary source CLI unavailable")
        cls.temporary = tempfile.TemporaryDirectory(prefix="独立预算来源 ")
        cls.base = audit.paths.canonical_selected_path(cls.temporary.name)
        cls.project = cls.base / "project"
        commands = [[str(cls.binary), "project", "init", str(cls.project)], [str(cls.binary), "source", "import", str(cls.project), str(audit.FIXTURE / "source/source.txt")]]
        cls.source_calls = []
        for command in commands:
            result = subprocess.run(command, capture_output=True, env=audit.clean_environment(), timeout=20)
            cls.source_calls.append(result)
            if result.returncode != 0:
                cls.temporary.cleanup()
                raise AssertionError(result.stderr.decode(errors="replace"))
        cls.imported = audit.strict_json(cls.source_calls[-1].stdout)
        cls.project_before = audit.paths.inventory(cls.project)

    @classmethod
    def tearDownClass(cls):
        if hasattr(cls, "temporary"):
            if audit.paths.inventory(cls.project) != cls.project_before:
                raise AssertionError("ordinary source original changed during checker tests")
            cls.temporary.cleanup()

    def fixture(self):
        folder = Path(tempfile.mkdtemp(prefix="audit-files-", dir=self.base))
        self.addCleanup(lambda: shutil.rmtree(folder))
        folder = audit.paths.canonical_selected_path(folder)
        budget = folder / "budget"; budget.mkdir(); control = budget / "control"; control.mkdir()
        owner_path = control / "owner.json"; owner_path.write_bytes(b"")
        lock = control / "authority.lock"; lock.write_bytes(b"")
        database = control / "budget.sqlite"
        with closing(sqlite3.connect(database)) as db, db:
            db.executescript(";".join(audit.SCHEMA.values()))
            db.execute("PRAGMA user_version=1"); db.execute("PRAGMA application_id=0x4e425544")
        run_root = folder / "run"; run_root.mkdir()
        (run_root / "control").mkdir()
        (run_root / "control/owner.json").write_bytes(b"synthetic checker file; not a product run")
        (run_root / "control/worker.lock").write_bytes(b"")
        attempt = run_root / "attempts/unit-a/1"; attempt.mkdir(parents=True)
        api_path = folder / "api.json"
        api_path.write_bytes((audit.FIXTURE / "policies/single-request-api-template.json").read_bytes())
        config_path = folder / "run-config.json"
        config = audit.strict_json((audit.FIXTURE / "policies/run-template.json").read_bytes())
        config["units"] = [{"unit_id": "unit-a", "project": str(self.project), "import_id": self.imported["import_id"], "api_config": str(api_path), "max_attempts": 3, "max_retry_tokens": 16620, "max_elapsed_ms": 10000}]
        config_path.write_bytes(audit.canonical(config))
        body = audit.strict_json((audit.FIXTURE / "originals/core05-request-body-a.json").read_bytes())
        bound = audit.strict_json(body["input"])
        version = self.imported["version"]
        with closing(sqlite3.connect(audit.paths.sqlite_file_uri(self.project / "project.sqlite"), uri=True)) as db, db:
            project_id = db.execute("SELECT project_id FROM project_metadata").fetchone()[0]
        bound["request_id"] = REQUEST_A
        bound["binding"].update(project_id=project_id, import_id=self.imported["import_id"], source_id=version["source_id"], version_id=version["version_id"])
        bound["units"][0]["locator"]["span_id"] = self.imported["spans"][0]["span_id"]
        body["input"] = audit.canonical(bound).decode()
        body_raw = audit.canonical(body)
        self.assertEqual(len(body_raw), 5238)
        (attempt / "request-body.json").write_bytes(body_raw)
        metadata = audit.strict_json((audit.FIXTURE / "originals/preflight-api-a/request.json").read_bytes())
        metadata.update(request_id=REQUEST_A, binding=bound["binding"], units=bound["units"], limits=config["units"][0].get("limits", audit.strict_json(api_path.read_bytes())["limits"]), budget=audit.strict_json(api_path.read_bytes())["budget"])
        metadata["config"] = {"path": str(api_path), "fingerprint": {"sha256": audit.sha(api_path.read_bytes()), "byte_length": api_path.stat().st_size}}
        metadata["request_body"] = {"sha256": audit.sha(body_raw), "byte_length": len(body_raw)}
        (attempt / "request.json").write_bytes(audit.canonical(metadata))
        (attempt / ".owner.json").write_bytes(audit.canonical({"schema_version": 1, "producer": "api", "purpose": "unpublished_extraction_test", "request_id": REQUEST_A, "transport_mode": "local_http_test"}))
        response = (audit.FIXTURE / "wire/fixed-200-invalid-candidate.bin").read_bytes()
        (attempt / "response.bin").write_bytes(response)
        transport = {"schema_version": 1, "producer": "synthetic_api_transport_receipt", "request_id": REQUEST_A, "http_status": 200, "http_request_id": "req_noetome_test", "response_complete": True, "network_attempts": 1, "response": {"sha256": audit.sha(response), "byte_length": len(response)}, "response_file": "response.bin", "error_code": None, "omission_reason": None, "elapsed_ms": 1}
        (attempt / "transport-receipt.json").write_bytes(audit.canonical(transport))
        report = audit.strict_json((audit.FIXTURE / "originals/preflight-api-a/validation.json").read_bytes())
        report.update(actual_model="noetome-test-model", request_id=REQUEST_A, binding=bound["binding"], http_status=200, http_request_id="req_noetome_test", response_complete=True, response_id="resp_noetome_test")
        report["usage"].update(network_attempts=1, model_usage=audit.strict_json(response)["usage"])
        report["errors"] = [{"code": "EXTRACT_RESPONSE_INVALID", "message": "strict synthetic invalid candidate"}]
        report["artifacts"] = {p.name: {"sha256": audit.sha(p.read_bytes()), "byte_length": p.stat().st_size} for p in attempt.iterdir()}
        (attempt / "validation.json").write_bytes(audit.canonical(report))
        (attempt / "validation.md").write_bytes(("独立人工审计文件；不声称产品执行。\n```json\n" + json.dumps(report, ensure_ascii=False) + "\n```\n").encode("utf-8"))
        auth = authority()
        authority_file = folder / "authority-config.json"
        authority_file.write_bytes(audit.canonical({"schema_version": 1, "producer": "synthetic_api_budget", "global_budget": auth["global_limits"], "opening": auth["opening"]}))
        auth["policy_sha256"] = audit.sha(authority_file.read_bytes())
        (control / "policy.json").write_bytes(authority_file.read_bytes())
        owner = {"schema_version": 1, "producer": "synthetic_request_budget", "root": str(budget), "root_identity": audit.paths.actual_identity(budget), "parent_identity": audit.paths.actual_identity(folder), "control_identity": audit.paths.actual_identity(control), "database_identity": audit.paths.actual_identity(database), "owner_identity": audit.paths.actual_identity(owner_path), "lock_identity": audit.paths.actual_identity(lock), "policy_file": audit.observed_stamp(control / "policy.json")[0], "compiler_fingerprint": "1" * 64, "authority": auth}
        owner_raw = audit.canonical(owner); owner_path.write_bytes(owner_raw)
        c = Chain(auth).register(units=("unit-a",))
        registered = audit.strict_json(c.rows[0][2])["event"]
        registered["spec"].update(run_root=str(run_root), run_limits=config["run_budget"], case_sha256=audit.sha(config_path.read_bytes()), input_pins={str(p): audit.observed_stamp(p)[0] for p in [config_path, api_path, *self.project.rglob("*")] if p.is_file()})
        registered["identity"] = {"root": audit.paths.actual_identity(run_root), "parent": audit.paths.actual_identity(folder), "control": audit.paths.actual_identity(run_root / "control"), "attempts": audit.paths.actual_identity(run_root / "attempts"), "owner": audit.observed_stamp(run_root / "control/owner.json")[0], "worker": audit.observed_stamp(run_root / "control/worker.lock")[0], "units": {"unit-a": audit.paths.actual_identity(run_root / "attempts/unit-a")}}
        run_owner = {"schema_version": 1, "producer": "synthetic_api_run_owner", "run_id": registered["spec"]["run_id"], "root": str(run_root), "budget_root": str(budget), "budget_identity": audit.paths.actual_identity(budget), "root_identity": registered["identity"]["root"], "parent_identity": registered["identity"]["parent"], "control_identity": registered["identity"]["control"], "attempts_identity": registered["identity"]["attempts"], "owner_identity": registered["identity"]["owner"]["identity"], "lock_identity": registered["identity"]["worker"]["identity"], "config_path": str(config_path), "config_stamp": audit.observed_stamp(config_path)[0], "compiler_fingerprint": "1" * 64, "config": config, "spec": registered["spec"], "input_directories": {str(self.project): audit.paths.actual_identity(self.project)}, "unit_directories": {str(run_root / "attempts/unit-a"): registered["identity"]["units"]["unit-a"]}}
        (run_root / "control/owner.json").write_bytes(audit.canonical(run_owner))
        registered["identity"]["owner"] = audit.observed_stamp(run_root / "control/owner.json")[0]
        c = Chain(auth).append(registered)
        key = c.prepare()
        prepared = audit.strict_json(c.rows[-1][2])["event"]
        for name, field in [("request-body.json", "request_body"), ("request.json", "request_metadata")]: prepared["plan"][field] = {"sha256": audit.sha((attempt / name).read_bytes()), "byte_length": (attempt / name).stat().st_size}
        prepared["identity"] = {"root": audit.paths.actual_identity(attempt), "parent": audit.paths.actual_identity(attempt.parent), "directories": {name: audit.paths.actual_identity(run_root / name) for name in ["attempts", "attempts/unit-a", "attempts/unit-a/1"]}}
        prepared["files"] = {n: audit.observed_stamp(attempt / n)[0] for n in (".owner.json", "request.json", "request-body.json")}
        c.rows.pop(); c.append(prepared)
        c.may_send(key)
        c.append({"kind": "transport_sealed", "key": key, "files": {n: audit.observed_stamp(attempt / n)[0] for n in ("response.bin", "transport-receipt.json")}})
        receipt = {"report": {"sha256": audit.sha((attempt / "validation.json").read_bytes()), "byte_length": (attempt / "validation.json").stat().st_size}, "response": transport["response"], "network_attempts": 1, "http_status": 200, "response_complete": True, "response_id": "resp_noetome_test", "http_request_id": "req_noetome_test", "usage": {"input_tokens": 100, "output_tokens": 600, "total_tokens": 700}, "candidate_status": "failed", "error_code": "EXTRACT_RESPONSE_INVALID", "elapsed_ms": 1, "price_usd": None}
        c.append({"kind": "receipt_settled", "key": key, "receipt": receipt, "files": {p.name: audit.observed_stamp(p)[0] for p in attempt.iterdir()}})
        with closing(sqlite3.connect(database)) as db, db:
            db.execute("INSERT INTO metadata VALUES(1,?)", (owner_raw.decode(),))
            db.executemany("INSERT INTO events VALUES(?,?,?,?)", [(seq, prev, raw.decode(), hashed) for seq, prev, raw, hashed in c.rows])
        return budget, attempt, c

    def test_actual_source_binding_and_complete_readonly_budget_files(self):
        budget, _, _ = self.fixture()
        before = audit.paths.inventory(budget)
        ledger, evidence = audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])
        self.assertEqual(ledger.totals()["accounted_tokens"], 700)
        self.assertTrue(evidence["attempt_originals"][0]["actual_report_checked"])
        self.assertEqual(audit.paths.inventory(budget), before)

    def test_actual_owner_same_bytes_new_identity_refused(self):
        budget, _, _ = self.fixture()
        owner = budget / "control/owner.json"
        raw = owner.read_bytes(); replacement = owner.with_name("replacement")
        replacement.write_bytes(raw); replacement.replace(owner)
        with self.assertRaisesRegex(audit.AuditError, "owner identity"): audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_unknown_sqlite_table_and_sidecar_preserved_refused(self):
        budget, _, _ = self.fixture()
        with closing(sqlite3.connect(budget / "control/budget.sqlite")) as db, db: db.execute("CREATE TABLE forged(value INTEGER)")
        before = audit.paths.inventory(budget)
        with self.assertRaisesRegex(audit.AuditError, "two-table"): audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])
        self.assertEqual(audit.paths.inventory(budget), before)
        (budget / "control/budget.sqlite-journal").write_bytes(b"unknown")
        before = audit.paths.inventory(budget)
        with self.assertRaisesRegex(audit.AuditError, "sidecars"): audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])
        self.assertEqual(audit.paths.inventory(budget), before)

    def test_synchronized_report_and_settlement_fake700_to_one_rejected_by_raw_wire(self):
        budget, attempt, chain = self.fixture()
        report = audit.strict_json((attempt / "validation.json").read_bytes())
        report["usage"]["model_usage"] = {"input_tokens": 1, "output_tokens": 0, "total_tokens": 1}
        (attempt / "validation.json").write_bytes(audit.canonical(report))
        (attempt / "validation.md").write_bytes(("```json\n" + json.dumps(report, ensure_ascii=False) + "\n```\n").encode("utf-8"))
        record = audit.strict_json(chain.rows[-1][2]); event = record["event"]
        event["receipt"]["usage"] = report["usage"]["model_usage"]
        event["receipt"]["report"] = {"sha256": audit.sha((attempt / "validation.json").read_bytes()), "byte_length": (attempt / "validation.json").stat().st_size}
        for n in ("validation.json", "validation.md"): event["files"][n] = audit.observed_stamp(attempt / n)[0]
        raw = audit.canonical(record)
        with closing(sqlite3.connect(budget / "control/budget.sqlite")) as db, db:
            db.execute("UPDATE events SET payload_json=?,payload_sha256=? WHERE sequence=?", (raw.decode(), audit.sha(raw), record["sequence"]))
        with self.assertRaisesRegex(audit.AuditError, "raw wire usage"): audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def rewrite_last_receipt(self, budget, attempt, chain, mutate):
        record = audit.strict_json(chain.rows[-1][2])
        report = audit.strict_json((attempt / "validation.json").read_bytes())
        mutate(record["event"]["receipt"], report)
        (attempt / "validation.json").write_bytes(audit.canonical(report))
        (attempt / "validation.md").write_bytes(("```json\n" + json.dumps(report, ensure_ascii=False) + "\n```\n").encode("utf-8"))
        record["event"]["receipt"]["report"] = {"sha256": audit.sha((attempt / "validation.json").read_bytes()), "byte_length": (attempt / "validation.json").stat().st_size}
        for n in ("validation.json", "validation.md"):
            record["event"]["files"][n] = audit.observed_stamp(attempt / n)[0]
        raw = audit.canonical(record)
        with closing(sqlite3.connect(budget / "control/budget.sqlite")) as db, db:
            db.execute("UPDATE events SET payload_json=?,payload_sha256=? WHERE sequence=?", (raw.decode(), audit.sha(raw), record["sequence"]))

    def test_synchronized_final_network_zero_cannot_refund_original_network_one(self):
        budget, attempt, chain = self.fixture()
        def mutate(receipt, report):
            receipt.update(network_attempts=0, usage=None, http_status=None, response_complete=False, response_id=None, http_request_id=None, response=None, elapsed_ms=0)
            report.update(http_status=None, response_complete=False, response_id=None, http_request_id=None)
            report["usage"].update(network_attempts=0, model_usage=None)
        self.rewrite_last_receipt(budget, attempt, chain, mutate)
        with self.assertRaisesRegex(audit.AuditError, "framing equals original transport including NotSent"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_synchronized_final_header_id_cannot_override_original_transport(self):
        budget, attempt, chain = self.fixture()
        self.rewrite_last_receipt(budget, attempt, chain, lambda receipt, report: (receipt.update(http_request_id="forged_header"), report.update(http_request_id="forged_header")))
        with self.assertRaisesRegex(audit.AuditError, "framing equals original transport"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_final_elapsed_cannot_override_original_transport(self):
        budget, attempt, chain = self.fixture()
        self.rewrite_last_receipt(budget, attempt, chain, lambda receipt, report: receipt.update(elapsed_ms=8))
        with self.assertRaisesRegex(audit.AuditError, "framing equals original transport"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_complete_report_token_details_cannot_be_synchronized_forged(self):
        budget, attempt, chain = self.fixture()
        def mutate(receipt, report):
            report["usage"]["model_usage"]["input_tokens_details"]["cached_tokens"] = 1
        self.rewrite_last_receipt(budget, attempt, chain, mutate)
        with self.assertRaisesRegex(audit.AuditError, "including details equals original report"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_actual_server_response_bytes_must_equal_received_API_original(self):
        budget, attempt, _ = self.fixture()
        ledger, _ = audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])
        server = budget.parent / "server"; server.mkdir()
        body = (attempt / "request-body.json").read_bytes(); (server / "request.bin").write_bytes(body)
        response = (attempt / "response.bin").read_bytes(); (server / "response.bin").write_bytes(response)
        key = next(iter(ledger.attempts))
        receipt = {"request_id": key[3], "run_id": key[0], "unit_id": key[1], "attempt_no": key[2], "method": "POST", "path": "/v1/responses", "authorization_matches_artificial_test_key": True, "request_body_file": "request.bin", "request_body_sha256": audit.sha(body), "request_body_byte_length": len(body), "request_received_utc_ns": ledger.last_at, "response_file": "response.bin", "response_sha256": audit.sha(response), "response_byte_length": len(response), "advertised_status": 200, "response_flush_utc_ns": ledger.last_at}
        checked = audit.verify_server_receipts(ledger, [receipt], server)
        self.assertEqual(checked["response_originals"][0]["sha256"], audit.sha(response))
        forged = response + b" "
        (server / "response.bin").write_bytes(forged)
        receipt.update(response_sha256=audit.sha(forged), response_byte_length=len(forged))
        with self.assertRaisesRegex(audit.AuditError, "API response equals actual server response bytes"):
            audit.verify_server_receipts(ledger, [receipt], server)

    def test_mirrored_owner_metadata_cap_edit_cannot_override_owned_policy(self):
        budget, _, _ = self.fixture()
        path = budget / "control/owner.json"
        identity = audit.paths.actual_identity(path)
        owner = audit.strict_json(path.read_bytes())
        owner["authority"]["global_limits"]["max_total_tokens"] += 8310
        raw = audit.canonical(owner); path.write_bytes(raw)
        with closing(sqlite3.connect(budget / "control/budget.sqlite")) as db, db:
            db.execute("UPDATE metadata SET metadata_json=? WHERE singleton=1", (raw.decode(),))
        self.assertEqual(audit.paths.actual_identity(path), identity)
        with self.assertRaisesRegex(audit.AuditError, "initial cap/opening"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_owned_policy_and_owner_forged_together_still_need_external_author_config(self):
        budget, _, _ = self.fixture()
        owner_path = budget / "control/owner.json"
        owner = audit.strict_json(owner_path.read_bytes())
        policy_path = budget / "control/policy.json"
        policy = audit.strict_json(policy_path.read_bytes())
        policy["global_budget"]["max_total_tokens"] += 8310
        policy_path.write_bytes(audit.canonical(policy))
        owner["authority"]["global_limits"] = policy["global_budget"]
        owner["authority"]["policy_sha256"] = audit.sha(policy_path.read_bytes())
        owner["policy_file"] = audit.observed_stamp(policy_path)[0]
        raw = audit.canonical(owner); owner_path.write_bytes(raw)
        with closing(sqlite3.connect(budget / "control/budget.sqlite")) as db, db:
            db.execute("UPDATE metadata SET metadata_json=?", (raw.decode(),))
        with self.assertRaisesRegex(audit.AuditError, "initial cap/opening"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_external_Run_owner_same_bytes_new_identity_cannot_self_authenticate(self):
        budget, _, chain = self.fixture()
        run = audit.strict_json(chain.rows[0][2])["event"]["spec"]["run_root"]
        owner = Path(run) / "control/owner.json"
        replacement = owner.with_name("replacement")
        replacement.write_bytes(owner.read_bytes()); replacement.replace(owner)
        with self.assertRaisesRegex(audit.AuditError, "sealed Run owner"):
            audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])

    def test_source_uuid_synchronized_in_metadata_and_body_not_selected_source_rejected(self):
        budget, attempt, chain = self.fixture()
        body = audit.strict_json((attempt / "request-body.json").read_bytes())
        bound = audit.strict_json(body["input"]); bound["binding"]["project_id"] = "99999999-9999-4999-8999-999999999999"
        body["input"] = audit.canonical(bound).decode(); (attempt / "request-body.json").write_bytes(audit.canonical(body))
        metadata = audit.strict_json((attempt / "request.json").read_bytes()); metadata["binding"] = bound["binding"]
        metadata["request_body"] = {"sha256": audit.sha((attempt / "request-body.json").read_bytes()), "byte_length": (attempt / "request-body.json").stat().st_size}
        (attempt / "request.json").write_bytes(audit.canonical(metadata))
        rows = []
        previous = "0" * 64
        for seq, _, raw, _ in chain.rows:
            record = audit.strict_json(raw); record["previous_sha256"] = previous
            event = record["event"]
            if event["kind"] == "prepared":
                for n, key in [("request-body.json", "request_body"), ("request.json", "request_metadata")]:
                    event["files"][n] = audit.observed_stamp(attempt / n)[0]
                    event["plan"][key] = {k: event["files"][n][k] for k in ("sha256", "byte_length")}
            if event["kind"] == "receipt_settled":
                for n in ("request-body.json", "request.json"): event["files"][n] = audit.observed_stamp(attempt / n)[0]
            encoded = audit.canonical(record); previous = audit.sha(encoded); rows.append((seq, record["previous_sha256"], encoded.decode(), previous))
        with closing(sqlite3.connect(budget / "control/budget.sqlite")) as db, db:
            db.execute("DELETE FROM events"); db.executemany("INSERT INTO events VALUES(?,?,?,?)", rows)
        with self.assertRaisesRegex(audit.AuditError, "selected real SQL"): audit.read_authority(budget, authority_config=budget.parent / "authority-config.json", selected_run_roots=[budget.parent / "run"])


if __name__ == "__main__":
    unittest.main()
