#!/usr/bin/env python3
"""Independent, local-only evidence audit for frozen B07-T02 request budgets.

Accounting is reconstructed from raw event bytes and separately bound to original
HTTP/body/report files. This module never imports a product projection or Rust
implementation, reads credentials, or contacts a nonnumeric-loopback endpoint.
"""
from __future__ import annotations

import argparse
from contextlib import closing
from concurrent.futures import ThreadPoolExecutor
import copy
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import signal
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import verify_run_checkpoint as paths

AuditError = paths.AuditError
require = paths.require
strict_json = paths.strict_json
sha = paths.sha
save_json = paths.save_json
ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "fixtures/run-api-budget-v1"
FROZEN_SHA = "ef04e12745d16f7e10e98f53b348475bdbfaa71d60c9302c297fbf2af0e32272"
TEST_KEY = "noetome-test-public-credential-never-real"
U64 = (1 << 64) - 1
RETRYABLE = {"EXTRACT_API_HTTP_RATE_LIMIT", "EXTRACT_API_HTTP_SERVER", "EXTRACT_TIMEOUT", "EXTRACT_API_TRANSPORT", "EXTRACT_API_TRUNCATED"}
FILE_NAMES = {".owner.json", "request.json", "request-body.json", "response.bin", "transport-receipt.json", "validation.json", "validation.md", "candidates.json", "recovery-validation.json"}
SCHEMA = {
    "metadata": "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),metadata_json TEXT NOT NULL)",
    "events": "CREATE TABLE events(sequence INTEGER PRIMARY KEY CHECK(sequence>0),previous_sha256 TEXT NOT NULL,payload_json TEXT NOT NULL,payload_sha256 TEXT NOT NULL)",
}
EVENT_FIELDS = {
    "run_registered": "spec identity", "prepared": "plan identity files", "reserved": "key", "may_send": "key", "transport_sealed": "key files",
    "reserved_released": "key reason", "unknown_retained": "key reason", "receipt_settled": "key receipt files", "retry_scheduled": "key schedule",
    "unit_finished": "run_id unit_id outcome diagnostic", "run_finished": "run_id outcome diagnostic", "run_invalidated": "run_id diagnostic",
}


def fields(value, names, label):
    names = set(names.split()) if isinstance(names, str) else set(names)
    require(isinstance(value, dict) and set(value) == names, label + " exact fields")
    return value


def natural(value, label, maximum=U64):
    require(type(value) is int and 0 <= value <= maximum, label + " bounded unsigned integer")
    return value


def add(*values):
    result = sum(natural(v, "sum term") for v in values)
    return natural(result, "sum")


def digest(value, label):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), label + " lowercase SHA256")
    return value


def uuid(value, label):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", value), label + " UUID")
    return value


def utc_ns(value):
    require(isinstance(value, str), "UTC text")
    match = re.fullmatch(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(?:\.(\d{1,9}))?(?:Z|\+00:00)", value)
    require(match is not None, "explicit UTC timestamp")
    try:
        parsed = dt.datetime.strptime(match[1], "%Y-%m-%dT%H:%M:%S").replace(tzinfo=dt.timezone.utc)
    except ValueError as error:
        raise AuditError("invalid UTC date") from error
    seconds = int(parsed.timestamp())
    require(seconds >= 0, "UTC before epoch")
    return seconds * 1_000_000_000 + int((match[2] or "").ljust(9, "0"))


def canonical(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def limits(value):
    fields(value, "max_requests max_total_tokens max_cost_usd", "budget limits")
    natural(value["max_requests"], "request cap", 1_000_000)
    natural(value["max_total_tokens"], "Token cap")
    require(value["max_cost_usd"] is None, "unknown price must remain null")


def file_pin(value):
    fields(value, "sha256 byte_length", "file pin")
    digest(value["sha256"], "file pin")
    natural(value["byte_length"], "file length", 64 * 1024 * 1024)


def identity_schema(value):
    require(isinstance(value, dict) and value.get("platform") in ("unix", "windows"), "supported actual identity")
    fields(value, "platform device inode" if value["platform"] == "unix" else "platform volume index", "file identity")
    for key, number in value.items():
        if key != "platform":
            natural(number, "identity " + key)


def stamp_schema(value):
    fields(value, "identity sha256 byte_length modified", "file stamp")
    identity_schema(value["identity"])
    file_pin({k: value[k] for k in ("sha256", "byte_length")})
    require(isinstance(value["modified"], str) and re.fullmatch(r"[0-9]{1,39}", value["modified"]), "epoch nanoseconds")


def observed_stamp(path):
    path = paths.ordinary_path(path)
    before = path.stat()
    require(before.st_size <= 64 * 1024 * 1024 and path.is_file() and before.st_nlink == 1, "bounded ordinary single-link file")
    raw = paths.read_observed_file(path, before)
    actual = {"identity": paths.actual_identity(path), "sha256": sha(raw), "byte_length": len(raw), "modified": str(before.st_mtime_ns)}
    after = path.stat()
    require((before.st_dev, before.st_ino, before.st_mtime_ns, before.st_size) == (after.st_dev, after.st_ino, after.st_mtime_ns, after.st_size), "file changed during audit")
    return actual, raw


def author_check(fixture=FIXTURE):
    fixture = paths.canonical_selected_path(fixture)
    manifest = paths.read_observed_file(fixture / "frozen-sha256.json")
    require(sha(manifest) == FROZEN_SHA, "independent author freeze anchor")
    frozen = strict_json(manifest)
    for name, expected in frozen["files"].items():
        relative = paths.safe_relative(name)
        observed, _ = observed_stamp(fixture / relative)
        require({k: observed[k] for k in ("sha256", "byte_length")} == expected, "frozen author member " + name)
    require(set(p.relative_to(fixture).as_posix() for p in fixture.rglob("*") if p.is_file()) == set(frozen["files"]) | {"frozen-sha256.json"}, "author complete inventory")
    return strict_json(paths.read_observed_file(fixture / "cases.json"))


def request_body_check(raw, fixture=FIXTURE):
    require(len(raw) == 5238, "frozen actual request body byte length")
    actual = strict_json(raw)
    frozen = strict_json(paths.read_observed_file(fixture / "originals/core05-request-body-a.json"))
    actual_input = strict_json(actual["input"])
    frozen_input = strict_json(frozen["input"])
    windows = strict_json(paths.read_observed_file(fixture / "request-binding-windows.json"))["allowed_decoded_input_uuid_paths"]
    for path in windows:
        for tree in (actual_input, frozen_input):
            parts = path.lstrip("/").split("/")
            node = tree
            for part in parts[:-1]:
                node = node[int(part)] if isinstance(node, list) else node[part]
            key = int(parts[-1]) if isinstance(node, list) else parts[-1]
            uuid(node[key], path)
            node[key] = "<actual-uuid36>"
    require(actual_input == frozen_input, "entire original source/request outside explicit UUID windows")
    actual["input"] = frozen["input"] = "<checked-bound-input>"
    require(actual == frozen, "complete original outer request body")
    return strict_json(strict_json(raw)["input"])


def validate_key(key):
    fields(key, "run_id unit_id attempt_no request_id", "attempt identity")
    uuid(key["run_id"], "Run")
    uuid(key["request_id"], "request")
    require(isinstance(key["unit_id"], str) and re.fullmatch(r"[A-Za-z0-9_-]{1,32}", key["unit_id"]), "WorkUnit ASCII identity")
    natural(key["attempt_no"], "attempt number", 4)
    require(key["attempt_no"] >= 1, "attempt starts at1")
    return tuple(key[k] for k in ("run_id", "unit_id", "attempt_no", "request_id"))


def usage_from_wire(raw, *, status, complete):
    """Missing/invalid framing or usage is unknown, never inferred to be zero."""
    if status != 200 or not complete:
        return None
    try:
        wire = strict_json(raw)
        require(wire.get("object") == "response" and wire.get("status") == "completed" and wire.get("error") is None and wire.get("incomplete_details") is None, "complete Responses metadata")
        value = wire["usage"]
        fields(value, "input_tokens input_tokens_details output_tokens output_tokens_details total_tokens", "Responses usage")
        fields(value["input_tokens_details"], "cached_tokens cache_write_tokens", "input token details")
        fields(value["output_tokens_details"], "reasoning_tokens", "output token details")
        input_tokens = natural(value["input_tokens"], "input usage")
        output_tokens = natural(value["output_tokens"], "output usage")
        total_tokens = natural(value["total_tokens"], "total usage")
        require(add(input_tokens, output_tokens) == total_tokens, "actual usage sum")
        require(add(*value["input_tokens_details"].values()) <= input_tokens and natural(value["output_tokens_details"]["reasoning_tokens"], "reasoning usage") <= output_tokens, "usage details bounds")
        return {"input_tokens": input_tokens, "output_tokens": output_tokens, "total_tokens": total_tokens}
    except (AuditError, KeyError, TypeError):
        return None


class Ledger:
    """Independent event invariants; artifact checks are mandatory in read_authority."""
    def __init__(self, authority):
        fields(authority, "schema_version authority_id global_limits opening policy_sha256", "authority")
        require(type(authority["schema_version"]) is int and authority["schema_version"] == 1 and isinstance(authority["authority_id"], str) and re.fullmatch(r"[A-Za-z0-9_-]{1,128}", authority["authority_id"]), "authority version/id")
        digest(authority["policy_sha256"], "authority policy")
        limits(authority["global_limits"])
        fields(authority["opening"], "accounted_tokens held_tokens dispatched_requests held_requests", "opening")
        for v in authority["opening"].values():
            natural(v, "opening balance")
        self.authority = copy.deepcopy(authority)
        self.runs = {}
        self.attempts = {}
        self.rows = []
        self.sequence = 0
        self.last_sha = "0" * 64
        self.last_at = 0
        self.global_stopped = False
        self.audit_admission()

    def totals(self, run_id=None, retry_unit=None):
        result = {k: 0 for k in ("accounted_tokens", "held_tokens", "dispatched_requests", "held_requests")}
        if run_id is None:
            result.update(self.authority["opening"])
        for key, a in self.attempts.items():
            if run_id is not None and key[0] != run_id or retry_unit is not None and (key[1] != retry_unit or key[2] < 2):
                continue
            state = a["state"]
            if state == "known":
                result["accounted_tokens"] = add(result["accounted_tokens"], a["receipt"]["usage"]["total_tokens"])
                result["dispatched_requests"] = add(result["dispatched_requests"], 1)
            elif state in ("may_send", "unknown"):
                result["held_tokens"] = add(result["held_tokens"], a["plan"]["reserved_tokens"])
                result["dispatched_requests"] = add(result["dispatched_requests"], 1)
            elif state == "reserved":
                result["held_tokens"] = add(result["held_tokens"], a["plan"]["reserved_tokens"])
                result["held_requests"] = add(result["held_requests"], 1)
        return result

    def audit_admission(self):
        cap = self.authority["global_limits"]
        total = self.totals()
        require(add(total["accounted_tokens"], total["held_tokens"]) <= cap["max_total_tokens"] and add(total["dispatched_requests"], total["held_requests"]) <= cap["max_requests"], "opening exceeds global caps")

    def run_unit(self, key):
        require(key[0] in self.runs, "unregistered Run")
        run = self.runs[key[0]]
        require(key[1] in run["units"], "unregistered WorkUnit")
        return run, run["units"][key[1]]

    def check_dispatch(self, key, at, extra=False):
        run, unit = self.run_unit(key)
        require(not self.global_stopped and not run["stopped"] and run["outcome"] is None and unit["outcome"] is None, "new request after stop/terminal")
        require(at < utc_ns(run["spec"]["deadline_utc"]) and at < utc_ns(unit["spec"]["deadline_utc"]), "new request after durable deadline")
        require(key[2] <= unit["spec"]["max_attempts"], "attempt limit")
        if key[2] >= 2:
            prior_key = unit["attempts"][key[2] - 2]
            previous = self.attempts[prior_key]
            require(previous["retry"] is not None and at >= utc_ns(previous["retry"]["retry_at_utc"]), "new attempt before durable backoff")
        if extra:
            amount = self.attempts[key]["plan"]["reserved_tokens"]
            for total, cap, label in ((self.totals(), self.authority["global_limits"], "global"), (self.totals(key[0]), run["spec"]["run_limits"], "Run")):
                require(add(total["accounted_tokens"], total["held_tokens"], amount) <= cap["max_total_tokens"] and add(total["dispatched_requests"], total["held_requests"], 1) <= cap["max_requests"], label + " atomic reservation cap")
            if key[2] >= 2:
                total = self.totals(key[0], key[1])
                require(add(total["accounted_tokens"], total["held_tokens"], amount) <= unit["spec"]["max_retry_tokens"], "independent retry Token cap")

    def apply_row(self, row):
        sequence, previous, raw, declared_sha = row
        require(type(sequence) is int and sequence == self.sequence + 1 and sequence <= 4096 and previous == self.last_sha, "complete contiguous event chain")
        raw = raw.encode("utf-8") if isinstance(raw, str) else raw
        require(isinstance(raw, bytes) and len(raw) <= 4 * 1024 * 1024 and sha(raw) == declared_sha, "original UTF8 payload SHA")
        record = strict_json(raw)
        require(canonical(record) == raw, "canonical event encoding")
        fields(record, "schema_version authority_id sequence previous_sha256 at_utc event", "event record")
        require(type(record["schema_version"]) is int and record["schema_version"] == 1 and record["authority_id"] == self.authority["authority_id"] and record["sequence"] == sequence and record["previous_sha256"] == previous, "event authority/sequence binding")
        at = utc_ns(record["at_utc"])
        require(at >= self.last_at, "event UTC moved backward")
        event = record["event"]
        kind = event.get("kind") if isinstance(event, dict) else None
        require(kind in EVENT_FIELDS, "unknown event kind")
        fields(event, {"kind"} | set(EVENT_FIELDS[kind].split()), "event " + kind)
        self.apply_event(event, at)
        self.sequence, self.last_sha, self.last_at = sequence, declared_sha, at
        self.rows.append(record)

    def apply_event(self, e, at):
        kind = e["kind"]
        if kind == "run_registered":
            s = e["spec"]
            fields(s, "run_id run_root case_sha256 input_pins run_limits deadline_utc retry_base_delay_ms retry_max_delay_ms units", "Run spec")
            uuid(s["run_id"], "registered Run")
            digest(s["case_sha256"], "original config")
            require(s["run_id"] not in self.runs and len(self.runs) < 64 and all(r["spec"]["run_root"] != s["run_root"] for r in self.runs.values()), "Run identity/root uniqueness")
            limits(s["run_limits"])
            require(isinstance(s["run_root"], str) and Path(s["run_root"]).is_absolute(), "absolute registered Run root")
            require(isinstance(s["input_pins"], dict) and 1 <= len(s["input_pins"]) <= 128, "selected input pins")
            for path, stamp in s["input_pins"].items():
                require(Path(path).is_absolute(), "absolute selected input")
                stamp_schema(stamp)
            require(1 <= natural(s["retry_base_delay_ms"], "retry base", 5000) <= 5000 and s["retry_base_delay_ms"] <= natural(s["retry_max_delay_ms"], "retry maximum", 10000) <= 10000, "frozen bounded retry delay")
            require(isinstance(s["units"], list) and 1 <= len(s["units"]) <= 4, "bounded unit count")
            units = {}
            for u in s["units"]:
                fields(u, "unit_id max_attempts max_retry_tokens deadline_utc", "unit spec")
                require(isinstance(u["unit_id"], str) and re.fullmatch(r"[A-Za-z0-9_-]{1,32}", u["unit_id"]) and u["unit_id"] not in units, "unique unit identity")
                require(1 <= natural(u["max_attempts"], "max attempts", 4) <= 4, "unit attempt bound")
                natural(u["max_retry_tokens"], "retry tokens")
                require(at < utc_ns(u["deadline_utc"]) <= utc_ns(s["deadline_utc"]) and utc_ns(s["deadline_utc"]) - at <= 60_000_000_000, "first registration durable deadlines")
                units[u["unit_id"]] = {"spec": copy.deepcopy(u), "attempts": [], "outcome": None}
            fields(e["identity"], "root parent control attempts owner worker units", "Run identities")
            for name in ("root", "parent", "control", "attempts"): identity_schema(e["identity"][name])
            for name in ("owner", "worker"): stamp_schema(e["identity"][name])
            require(e["identity"]["worker"]["sha256"] == sha(b"") and e["identity"]["worker"]["byte_length"] == 0, "original empty worker lock witness")
            fields(e["identity"]["units"], units, "exact original unit directory identities")
            for value in e["identity"]["units"].values(): identity_schema(value)
            self.runs[s["run_id"]] = {"spec": copy.deepcopy(s), "identity": copy.deepcopy(e["identity"]), "units": units, "outcome": None, "stopped": False, "registered_at_ns": at}
            return
        if kind == "prepared":
            p = e["plan"]
            fields(p, "key attempt_root request_body request_metadata input_reserved_tokens max_output_tokens reserved_tokens reservation_policy", "attempt plan")
            key = validate_key(p["key"])
            run, unit = self.run_unit(key)
            require(key not in self.attempts and key[2] == len(unit["attempts"]) + 1 and key[2] <= unit["spec"]["max_attempts"] and all(k[3] != key[3] for k in self.attempts), "monotone unique logical attempt/request")
            require(run["outcome"] is None and unit["outcome"] is None, "prepare after terminal")
            paths.safe_relative(p["attempt_root"])
            require(all(a["plan"]["attempt_root"] != p["attempt_root"] for k, a in self.attempts.items() if k[0] == key[0]), "unique owned attempt root")
            for name in ("request_body", "request_metadata"): file_pin(p[name])
            require(p["reservation_policy"] == "request_utf8_bytes_plus_1024_v1" and 16 <= natural(p["max_output_tokens"], "output limit", 8192) <= 8192 and p["input_reserved_tokens"] == add(p["request_body"]["byte_length"], 1024) and p["reserved_tokens"] == add(p["input_reserved_tokens"], p["max_output_tokens"]), "independent actual byte reservation")
            fields(e["files"], ".owner.json request.json request-body.json", "Prepared sealed inventory")
            for v in e["files"].values(): stamp_schema(v)
            for filename, pin_name in (("request-body.json", "request_body"), ("request.json", "request_metadata")):
                require({k: e["files"][filename][k] for k in ("sha256", "byte_length")} == p[pin_name], "Prepared pin matches original file witness")
            fields(e["identity"], "root parent directories", "attempt ancestor identities")
            prefixes = {"/".join(p["attempt_root"].split("/")[:n]) for n in range(1, len(p["attempt_root"].split("/")) + 1)}
            fields(e["identity"]["directories"], prefixes, "entire attempt ancestor inventory")
            for v in [e["identity"]["root"], e["identity"]["parent"], *e["identity"]["directories"].values()]: identity_schema(v)
            require(e["identity"]["directories"][p["attempt_root"]] == e["identity"]["root"], "attempt root ancestor identity")
            if key[2] >= 2: require(self.attempts[unit["attempts"][-1]]["retry"] is not None, "new logical attempt requires durable retry")
            self.attempts[key] = {"plan": copy.deepcopy(p), "identity": copy.deepcopy(e["identity"]), "files": copy.deepcopy(e["files"]), "state": "prepared", "receipt": None, "retry": None, "transport_sealed": False, "unknown_reason": None}
            unit["attempts"].append(key)
            return
        if kind in ("unit_finished", "run_finished", "run_invalidated"):
            require(e["run_id"] in self.runs and isinstance(e["diagnostic"], str) and 1 <= len(e["diagnostic"].encode()) <= 2048, "terminal Run/diagnostic")
            run = self.runs[e["run_id"]]
            if kind == "unit_finished":
                require(e["unit_id"] in run["units"], "terminal unit")
                unit = run["units"][e["unit_id"]]
                require(unit["outcome"] is None and e["outcome"] in ("succeeded", "failed", "budget_stopped", "invalidated"), "unit terminal once")
                if e["outcome"] == "succeeded":
                    require(unit["attempts"] and self.attempts[unit["attempts"][-1]]["state"] == "known" and self.attempts[unit["attempts"][-1]]["receipt"]["candidate_status"] == "validated", "success requires independently bound candidate")
                unit["outcome"] = e["outcome"]
            else:
                if kind == "run_finished":
                    require(run["outcome"] is None and e["outcome"] in ("succeeded", "failed", "budget_stopped", "invalidated"), "Run terminal once")
                    if e["outcome"] == "succeeded": require(all(u["outcome"] == "succeeded" for u in run["units"].values()), "Run success needs all units")
                    run["outcome"] = e["outcome"]
                else:
                    require(run["outcome"] != "invalidated", "duplicate invalidation")
                    run["outcome"] = "invalidated"
                    run["stopped"] = True
            return
        key = validate_key(e["key"])
        require(key in self.attempts, "event for unknown attempt")
        a = self.attempts[key]
        if kind == "reserved":
            require(a["state"] == "prepared", "reserve only Prepared, once")
            self.check_dispatch(key, at, extra=True)
            a["state"] = "reserved"
        elif kind == "may_send":
            require(a["state"] == "reserved", "MaySend permission exactly once")
            self.check_dispatch(key, at)
            a["state"] = "may_send"
        elif kind in ("transport_sealed", "receipt_settled"):
            require(isinstance(e["files"], dict), "sealed files")
            for name, value in e["files"].items():
                require(name in FILE_NAMES, "unknown sealed artifact")
                stamp_schema(value)
                require(name not in a["files"] or value == a["files"][name], "immutable prior artifact witness")
            if kind == "transport_sealed":
                require(a["state"] == "may_send" and not a["transport_sealed"] and "transport-receipt.json" in e["files"] and set(e["files"]) <= {"transport-receipt.json", "response.bin"}, "transport seal only after MaySend once")
                a["transport_sealed"] = True
            else:
                r = e["receipt"]
                fields(r, "report response network_attempts http_status response_complete response_id http_request_id usage candidate_status error_code elapsed_ms price_usd", "attempt receipt")
                file_pin(r["report"])
                natural(r["elapsed_ms"], "receipt elapsed")
                require(r["http_status"] is None or type(r["http_status"]) is int and 100 <= r["http_status"] <= 599, "receipt HTTP status")
                for nullable in ("response_id", "http_request_id", "error_code"):
                    require(r[nullable] is None or isinstance(r[nullable], str) and 1 <= len(r[nullable].encode()) <= 2048 and not any(ord(c) < 32 for c in r[nullable]), "bounded receipt nullable text")
                require(r["price_usd"] is None and type(r["network_attempts"]) is int and r["network_attempts"] in (0, 1) and type(r["response_complete"]) is bool, "receipt network/price")
                if r["response"] is not None: file_pin(r["response"])
                require(a["state"] not in ("known", "not_sent") and not (a["receipt"] is not None and r["usage"] is None), "settlement not duplicated or weakened")
                require(r["candidate_status"] in ("validated", "failed") and ((r["candidate_status"] == "validated" and r["error_code"] is None and r["usage"] is not None) or (r["candidate_status"] == "failed" and isinstance(r["error_code"], str))), "candidate result separate from usage")
                if r["network_attempts"] == 0:
                    require(r["usage"] is None and r["response"] is None and r["http_status"] is None and r["response_id"] is None and r["http_request_id"] is None and not r["response_complete"], "NotSent has no remote result")
                    require(a["state"] in ("prepared", "reserved") or a["transport_sealed"], "MaySend NotSent needs protocol proof")
                else:
                    require(a["state"] in ("may_send", "unknown") and a["transport_sealed"], "network settlement needs MaySend and framing witness")
                if r["usage"] is not None:
                    fields(r["usage"], "input_tokens output_tokens total_tokens", "settled usage")
                    require(add(r["usage"]["input_tokens"], r["usage"]["output_tokens"]) == r["usage"]["total_tokens"] and r["network_attempts"] == 1 and r["http_status"] == 200 and r["response_complete"] and r["response"] is not None, "Known requires actual complete200 usage")
                report_names = [n for n in ("validation.json", "recovery-validation.json") if n in e["files"] and {k: e["files"][n][k] for k in ("sha256", "byte_length")} == r["report"]]
                require(len(report_names) == 1, "unique original report witness")
                a["receipt"] = copy.deepcopy(r)
                a["state"] = "not_sent" if r["network_attempts"] == 0 else "known" if r["usage"] is not None else "unknown"
                run, _ = self.run_unit(key)
                if r["usage"] and (r["usage"]["input_tokens"] > a["plan"]["input_reserved_tokens"] or r["usage"]["output_tokens"] > a["plan"]["max_output_tokens"] or r["usage"]["total_tokens"] > a["plan"]["reserved_tokens"]):
                    run["stopped"] = True
            a["files"].update(copy.deepcopy(e["files"]))
            total = self.totals()
            if add(total["accounted_tokens"], total["held_tokens"]) > self.authority["global_limits"]["max_total_tokens"] or add(total["dispatched_requests"], total["held_requests"]) > self.authority["global_limits"]["max_requests"]:
                self.global_stopped = True
        elif kind == "reserved_released":
            require(a["state"] in ("prepared", "reserved") and isinstance(e["reason"], str) and e["reason"], "release only before MaySend")
            a["state"] = "not_sent"
            a["unknown_reason"] = e["reason"]
        elif kind == "unknown_retained":
            require(a["state"] == "may_send" and isinstance(e["reason"], str) and e["reason"], "Unknown retains original MaySend")
            a["state"] = "unknown"
            a["unknown_reason"] = e["reason"]
        elif kind == "retry_scheduled":
            s = e["schedule"]
            fields(s, "retry_at_utc error_code", "retry schedule")
            run, unit = self.run_unit(key)
            require(a["retry"] is None and unit["attempts"][-1] == key and a["state"] in ("known", "unknown", "not_sent") and key[2] < unit["spec"]["max_attempts"] and run["outcome"] is None and unit["outcome"] is None and not run["stopped"] and not self.global_stopped, "bounded new retry schedule")
            if a["receipt"] is None:
                require(s["error_code"] == "REQUEST_INTERRUPTED" and a["state"] == "unknown" and a["unknown_reason"] == "REQUEST_INTERRUPTED", "explicit interruption recovery only without final receipt")
            else:
                require(s["error_code"] in RETRYABLE and a["receipt"]["candidate_status"] == "failed" and a["receipt"]["error_code"] == s["error_code"], "HTTP retry fixed whitelist equals original failure")
                require(a["receipt"]["usage"] is None and not (a["receipt"]["response_complete"] and a["receipt"]["http_status"] == 200), "retry classes cannot bypass known usage or complete200 wire/candidate failure isolation")
            delay = min(run["spec"]["retry_base_delay_ms"] * (1 << (key[2] - 1)), run["spec"]["retry_max_delay_ms"])
            due = utc_ns(s["retry_at_utc"])
            require(at + delay * 1_000_000 <= due < utc_ns(unit["spec"]["deadline_utc"]), "durable retry delay/deadline")
            a["retry"] = copy.deepcopy(s)
        else:
            raise AuditError("unsupported event")


def reconstruct_events(authority, rows):
    ledger = Ledger(authority)
    for row in rows:
        ledger.apply_row(row)
    return ledger


def transport_check(value, raw_response, request_id):
    fields(value, "schema_version producer request_id http_status http_request_id response_complete network_attempts response response_file error_code omission_reason elapsed_ms", "transport receipt")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1 and value["producer"] == "synthetic_api_transport_receipt" and value["request_id"] == request_id, "transport identity")
    require(type(value["network_attempts"]) is int and value["network_attempts"] in (0, 1) and type(value["response_complete"]) is bool, "actual transport counters/framing")
    natural(value["elapsed_ms"], "network elapsed")
    file_pin(value["response"])
    require(value["http_status"] is None or type(value["http_status"]) is int and 100 <= value["http_status"] <= 599, "actual HTTP status")
    require(value["response_file"] in (None, "response.bin") and value["omission_reason"] in (None, "omitted_credential_echo"), "safe original response location")
    if value["response_file"] is not None:
        require(raw_response is not None and value["omission_reason"] is None and value["response"] == {"sha256": sha(raw_response), "byte_length": len(raw_response)}, "transport binds actual response bytes")
    else:
        require(raw_response is None, "unlisted actual response")
    if value["network_attempts"] == 0:
        require(value["http_status"] is None and value["http_request_id"] is None and not value["response_complete"] and value["response_file"] is None and value["response"] == {"sha256": sha(b""), "byte_length": 0}, "actual NotSent proof")
    else:
        require(value["response_file"] is not None or value["omission_reason"] is not None, "network result requires safe original or explicit omission")
    return value


def candidate_gold(candidate, bound_input, fixture):
    gold = strict_json(paths.read_observed_file(fixture / "source/gold.json"))
    expected = strict_json(paths.read_observed_file(fixture / "source/candidate-template.json"))
    expected.pop("producer")
    expected["request_id"] = bound_input["request_id"]
    expected["binding"] = bound_input["binding"]
    require(candidate == expected, "complete candidate equals independently frozen author plus actual source binding")
    evidence = candidate["evidence"][0]
    source = paths.read_observed_file(fixture / "source/source.txt")
    quote = source[evidence["excerpt_start_byte"]:evidence["excerpt_end_byte"]]
    require(quote.decode() == gold["quote"] == evidence["quote"] and sha(quote) == evidence["excerpt_sha256"], "exact original gold quote")
    require(candidate["atoms"][0]["atom"]["verification"] == "unverified" and candidate["atoms"][0]["atom"]["required"] is False, "candidate stays unverified")


def selected_source_check(run, unit_id, bound_input, metadata, fixture):
    if run.get("inputs_changed") and run["outcome"] == "invalidated":
        return
    config_paths = [Path(p) for p, stamp in run["spec"]["input_pins"].items() if stamp["sha256"] == run["spec"]["case_sha256"]]
    require(len(config_paths) == 1, "one original pinned Run config")
    config_observed, config_raw = observed_stamp(config_paths[0])
    if config_observed != run["spec"]["input_pins"][str(config_paths[0])]:
        require(run["outcome"] == "invalidated", "Run config changed without invalidation")
        return
    config = strict_json(config_raw)
    fields(config, "schema_version producer run_budget concurrency max_elapsed_ms retry units", "original Run configuration")
    require(config["schema_version"] == 1 and config["producer"] == "synthetic_api_run" and type(config["concurrency"]) is int and config["concurrency"] in (1, 2), "original synthetic Run")
    require(config["run_budget"] == run["spec"]["run_limits"] and config["retry"] == {"base_delay_ms": run["spec"]["retry_base_delay_ms"], "max_delay_ms": run["spec"]["retry_max_delay_ms"]}, "original limits/retry cannot be synchronized-forged")
    require(utc_ns(run["spec"]["deadline_utc"]) - run["registered_at_ns"] <= natural(config["max_elapsed_ms"], "original Run elapsed cap", 60000) * 1_000_000, "Run absolute deadline cannot be extended beyond original cap")
    selected = [u for u in config["units"] if u["unit_id"] == unit_id]
    require(len(selected) == 1, "one actual selected WorkUnit config")
    unit = selected[0]
    fields(unit, "unit_id project import_id api_config max_attempts max_retry_tokens max_elapsed_ms", "original unit configuration")
    require(unit["max_attempts"] == run["units"][unit_id]["spec"]["max_attempts"] and unit["max_retry_tokens"] == run["units"][unit_id]["spec"]["max_retry_tokens"], "unit policy original binding")
    require(utc_ns(run["units"][unit_id]["spec"]["deadline_utc"]) - run["registered_at_ns"] <= natural(unit["max_elapsed_ms"], "original unit elapsed cap", 60000) * 1_000_000, "unit absolute deadline cannot be extended beyond original cap")
    project = paths.canonical_selected_path(unit["project"])
    before = paths.inventory(project)
    database = project / "project.sqlite"
    require(str(database) in run["spec"]["input_pins"], "actual selected project DB is pinned")
    with closing(sqlite3.connect(paths.sqlite_file_uri(database), uri=True)) as db:
        db.row_factory = sqlite3.Row
        project_rows = list(db.execute("SELECT project_id FROM project_metadata WHERE singleton=1"))
        imports = list(db.execute("SELECT * FROM source_imports WHERE import_id=?", (unit["import_id"],)))
        require(len(project_rows) == len(imports) == 1, "actual source project/import")
        imported = dict(imports[0])
        versions = list(db.execute("SELECT * FROM source_versions WHERE version_id=?", (imported["version_id"],)))
        spans = list(db.execute("SELECT * FROM source_spans WHERE source_version_id=? AND ordinal=0", (imported["version_id"],)))
        require(len(versions) == len(spans) == 1, "one original version/span0")
        version, span = dict(versions[0]), dict(spans[0])
    binding = bound_input["binding"]
    require(binding == {"project_id": project_rows[0]["project_id"], "import_id": imported["import_id"], "source_id": version["source_id"], "version_id": version["version_id"], "content_sha256": version["content_sha256"], "normalized_sha256": version["normalized_sha256"], "manifest_sha256": None, "parse_id": None}, "entire body source binding equals selected real SQL")
    require(imported["source_id"] == version["source_id"] and imported["status"] == "ready" and span["parse_status"] == "parsed", "actual ready source")
    raw = paths.read_observed_file(project / paths.safe_relative(version["raw_snapshot"]))
    normalized = paths.read_observed_file(project / paths.safe_relative(version["normalized_snapshot"]))
    require(raw == normalized == paths.read_observed_file(fixture / "source/source.txt") and sha(raw) == version["content_sha256"] and sha(normalized) == version["normalized_sha256"], "entire actual source byte originals")
    require(bound_input["units"][0]["locator"]["span_id"] == span["span_id"] and bound_input["units"][0]["start_byte"] == span["start_byte"] == 0 and bound_input["units"][0]["end_byte"] == span["end_byte"] == len(normalized), "actual source Span coordinates/identity")
    require(metadata["binding"] == binding and metadata["units"] == bound_input["units"], "request metadata agrees with actual body/source")
    api_path = paths.canonical_selected_path(unit["api_config"])
    require(metadata["config"]["path"] == str(api_path) and str(api_path) in run["spec"]["input_pins"], "original API config selected absolute path")
    actual_config = paths.read_observed_file(api_path)
    require(metadata["config"]["fingerprint"] == {"sha256": sha(actual_config), "byte_length": len(actual_config)}, "actual API config fingerprint")
    require(paths.inventory(project) == before, "readonly full selected SQL/source inventory preserved")


def verify_attempt_originals(a, run, fixture):
    plan = a["plan"]
    root = paths.canonical_selected_path(Path(run["spec"]["run_root"]) / paths.safe_relative(plan["attempt_root"]))
    require(paths.actual_identity(root) == a["identity"]["root"] and paths.actual_identity(root.parent) == a["identity"]["parent"], "original attempt/parent identity")
    for name, expected in a["identity"]["directories"].items():
        require(paths.actual_identity(Path(run["spec"]["run_root"]) / paths.safe_relative(name)) == expected, "entire owned attempt ancestor identity")
    actual_files = {p.name for p in root.iterdir()}
    require(actual_files <= FILE_NAMES and set(a["files"]) <= actual_files, "complete bounded API artifact inventory")
    originals = {}
    for name in actual_files:
        observed, raw = observed_stamp(root / name)
        originals[name] = raw
        if name in a["files"]:
            require(observed == a["files"][name], "immutable sealed artifact identity/mtime/SHA: " + name)
    body = originals["request-body.json"]
    bound_input = request_body_check(body, fixture)
    metadata = strict_json(originals["request.json"])
    require(bound_input["request_id"] == metadata["request_id"] == plan["key"]["request_id"], "actual API request identity")
    require(metadata["provider"] == "openai_responses" and metadata["transport_mode"] == "local_http_test" and re.fullmatch(r"http://127\.0\.0\.1:[1-9][0-9]{0,4}/v1/responses", metadata["endpoint"]), "numeric loopback single adapter only")
    require(metadata["reservation_policy"] == "request_utf8_bytes_plus_1024_v1" and metadata["input_reserved_tokens"] == len(body) + 1024 and metadata["reserved_tokens"] == len(body) + 1024 + metadata["limits"]["max_output_tokens"] == plan["reserved_tokens"] == 8310, "original body independent R8310")
    selected_source_check(run, plan["key"]["unit_id"], bound_input, metadata, fixture)
    require(strict_json(originals[".owner.json"]) == {"producer": "api", "purpose": "unpublished_extraction_test", "request_id": plan["key"]["request_id"], "schema_version": 1, "transport_mode": "local_http_test"}, "complete original API owner")
    require(plan["request_body"] == {"sha256": sha(body), "byte_length": len(body)} and plan["request_metadata"] == {"sha256": sha(originals["request.json"]), "byte_length": len(originals["request.json"])}, "plan binds actual request originals")
    report = None
    if a["receipt"] is not None:
        receipt = a["receipt"]
        report_names = [n for n in ("validation.json", "recovery-validation.json") if n in originals and {"sha256": sha(originals[n]), "byte_length": len(originals[n])} == receipt["report"]]
        require(len(report_names) == 1, "actual unique report original")
        report = strict_json(originals[report_names[0]])
        require(report["request_id"] == plan["key"]["request_id"] and report["published"] is False and report["knowledge_status"] == "pending" and report["scientific_quality"] == "not_run", "report remains pending and unpublished")
        natural(report["usage"]["network_attempts"], "report network attempts", 1)
        natural(report["usage"]["real_model_calls"], "report real calls", 0)
        natural(report["usage"]["paid_calls"], "report paid calls", 0)
        require(report["usage"]["real_model_calls"] == report["usage"]["paid_calls"] == 0 and report["usage"]["price_usd"] is None and report["usage"]["model_tokenizer"] is None, "synthetic-only usage report")
        require(report["usage"]["network_attempts"] == receipt["network_attempts"] and report["http_status"] == receipt["http_status"] and report["response_complete"] == receipt["response_complete"] and report["response_id"] == receipt["response_id"] and report["http_request_id"] == receipt["http_request_id"], "settlement binds actual report transport")
        for name, pin in report["artifacts"].items():
            require(name in originals and pin == {"sha256": sha(originals[name]), "byte_length": len(originals[name])}, "report original artifact pin " + name)
        actual_response = originals.get("response.bin")
        transport = transport_check(strict_json(originals["transport-receipt.json"]), actual_response, plan["key"]["request_id"]) if a["transport_sealed"] else None
        if transport is not None:
            require(all(transport[field] == receipt[field] for field in ("network_attempts", "http_status", "response_complete", "http_request_id", "elapsed_ms")), "settlement framing equals original transport including NotSent")
        else:
            require(receipt["network_attempts"] == 0 and receipt["http_status"] is None and not receipt["response_complete"] and receipt["http_request_id"] is None and receipt["elapsed_ms"] == 0, "no transport original cannot certify a network receipt")
        actual_usage = usage_from_wire(actual_response, status=receipt["http_status"], complete=receipt["response_complete"]) if actual_response is not None else None
        require(receipt["usage"] == actual_usage, "independent raw wire usage equals settlement; no synchronized fake totals")
        report_usage = report["usage"]["model_usage"]
        require(report_usage == (strict_json(actual_response)["usage"] if actual_usage is not None else None), "independent raw wire usage including details equals original report")
        require(receipt["response"] == ({"sha256": sha(actual_response), "byte_length": len(actual_response)} if actual_response is not None else None), "settlement raw response pin")
        if actual_usage is not None:
            decoded_wire = strict_json(actual_response)
            require(report["actual_model"] == decoded_wire["model"] and report["response_id"] == decoded_wire["id"], "known response actual model/id metadata")
        if receipt["candidate_status"] == "validated":
            require(report["status"] == "candidates_validated" and report["validated_atoms"] == 1, "one author candidate validated")
            wire = strict_json(actual_response)
            candidate = strict_json(wire["output"][0]["content"][0]["text"])
            candidate_gold(candidate, bound_input, fixture)
            saved = strict_json(originals["candidates.json"])
            fields(saved, "schema_version producer transport_mode request_id binding payload", "saved candidate wrapper")
            require(type(saved["schema_version"]) is int and saved["schema_version"] == 1 and saved["producer"] == "api" and saved["transport_mode"] == "local_http_test" and saved["request_id"] == plan["key"]["request_id"] and saved["binding"] == bound_input["binding"] and saved["payload"] == candidate, "saved complete candidate wrapper equals original response and frozen author")
        else:
            require(report["status"] == "failed" and report["validated_atoms"] == 0 and report["errors"] and report["errors"][0]["code"] == receipt["error_code"], "failed candidates isolated with original diagnostic")
        if report_names[0] == "validation.json":
            markdown = originals["validation.md"].decode()
            require("```json\n" in markdown, "real Markdown report JSON")
            fragment = markdown.split("```json\n", 1)[1].rsplit("\n```", 1)[0]
            require(strict_json(fragment) == report, "Markdown/JSON report facts agree")
    elif a["transport_sealed"]:
        transport_check(strict_json(originals["transport-receipt.json"]), originals.get("response.bin"), plan["key"]["request_id"])
    return {"attempt_root": str(root), "request_id": plan["key"]["request_id"], "body_sha256": sha(body), "body_byte_length": len(body), "state": a["state"], "actual_report_checked": report is not None}


def bind_original_authority(authority, config_path):
    require(config_path is not None, "explicit original AuthorityConfig required; never trust self-reported cap/opening")
    _, raw = observed_stamp(paths.canonical_selected_path(config_path))
    config = strict_json(raw)
    fields(config, "schema_version producer global_budget opening", "original authority config")
    require(type(config["schema_version"]) is int and config["schema_version"] == 1 and config["producer"] == "synthetic_api_budget", "original synthetic authority schema")
    require(authority["policy_sha256"] == sha(raw) and authority["global_limits"] == config["global_budget"] and authority["opening"] == config["opening"], "initial cap/opening bind original config, not mutable owner/metadata mirror")
    return config


def read_authority(root, *, fixture=FIXTURE, authority_config=None, expected_fingerprint=None, selected_run_roots=None):
    author_check(fixture)
    root = paths.canonical_selected_path(root)
    before = paths.inventory(root)
    require(set(p.name for p in root.iterdir()) == {"control"} and set(p.name for p in (root / "control").iterdir()) == {"budget.sqlite", "owner.json", "authority.lock", "policy.json"}, "exact authority owned inventory; unknown SQLite sidecars refused")
    owner_raw = paths.read_observed_file(root / "control/owner.json")
    owner = strict_json(owner_raw)
    fields(owner, "schema_version producer root root_identity parent_identity control_identity database_identity owner_identity lock_identity policy_file compiler_fingerprint authority", "authority owner")
    require(type(owner["schema_version"]) is int and owner["schema_version"] == 1 and owner["producer"] == "synthetic_request_budget" and owner["root"] == str(root), "canonical actual authority owner")
    digest(owner["compiler_fingerprint"], "compiler fingerprint")
    if expected_fingerprint is not None: require(owner["compiler_fingerprint"] == expected_fingerprint, "selected executed compiler fingerprint")
    for name, path in {"root_identity": root, "parent_identity": root.parent, "control_identity": root / "control", "database_identity": root / "control/budget.sqlite", "owner_identity": root / "control/owner.json", "lock_identity": root / "control/authority.lock"}.items():
        require(paths.actual_identity(path) == owner[name], "actual owner identity " + name)
    require((root / "control/authority.lock").stat().st_size == 0, "authority lock must remain empty")
    stamp_schema(owner["policy_file"])
    policy_stamp, policy_raw = observed_stamp(root / "control/policy.json")
    require(policy_stamp == owner["policy_file"], "owned original policy file identity/mtime/SHA")
    bind_original_authority(owner["authority"], root / "control/policy.json")
    with closing(sqlite3.connect(paths.sqlite_file_uri(root / "control/budget.sqlite"), uri=True)) as connection:
        require(connection.execute("PRAGMA user_version").fetchone()[0] == 1 and connection.execute("PRAGMA application_id").fetchone()[0] == 0x4E425544 and connection.execute("PRAGMA journal_mode").fetchone()[0].lower() == "delete", "budget SQLite schema/DELETE mode")
        actual_schema = dict(connection.execute("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'"))
        compact = lambda x: re.sub(r"\s", "", x)
        require({k: compact(v) for k, v in actual_schema.items()} == {k: compact(v) for k, v in SCHEMA.items()}, "complete exact two-table schema")
        metadata = list(connection.execute("SELECT singleton,metadata_json FROM metadata"))
        require(metadata == [(1, owner_raw.decode())], "metadata must equal original owner bytes")
        rows = list(connection.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM events ORDER BY sequence"))
    bind_original_authority(owner["authority"], authority_config)
    ledger = reconstruct_events(owner["authority"], rows)
    approved_roots = None if selected_run_roots is None else {str(paths.canonical_selected_path(p, leaf_missing=True)) for p in selected_run_roots}
    require(not ledger.runs or approved_roots is not None and {r["spec"]["run_root"] for r in ledger.runs.values()} <= approved_roots, "explicit selected Run roots required before reading any owner-derived external path")
    originals = []
    input_checks = {}
    for run_id, run in ledger.runs.items():
        rroot = paths.canonical_selected_path(run["spec"]["run_root"])
        require(paths.actual_identity(rroot) == run["identity"]["root"] and paths.actual_identity(rroot.parent) == run["identity"]["parent"], "registered Run original root/parent identity")
        require(paths.actual_identity(rroot / "control") == run["identity"]["control"] and paths.actual_identity(rroot / "attempts") == run["identity"]["attempts"], "sealed original Run control/attempts directory identities")
        for name, path in (("owner", rroot / "control/owner.json"), ("worker", rroot / "control/worker.lock")):
            require(observed_stamp(path)[0] == run["identity"][name], "external sealed Run " + name + " original stamp")
        run_owner = strict_json(paths.read_observed_file(rroot / "control/owner.json"))
        fields(run_owner, "schema_version producer run_id root budget_root budget_identity root_identity parent_identity control_identity attempts_identity owner_identity lock_identity config_path config_stamp compiler_fingerprint config spec input_directories unit_directories", "original selected Run owner")
        require(type(run_owner["schema_version"]) is int and run_owner["schema_version"] == 1 and run_owner["producer"] == "synthetic_api_run_owner" and run_owner["run_id"] == run_id and run_owner["root"] == str(rroot) and run_owner["budget_root"] == str(root) and run_owner["budget_identity"] == owner["root_identity"] and run_owner["compiler_fingerprint"] == owner["compiler_fingerprint"] and run_owner["spec"] == run["spec"], "sealed original Run owner binds complete selected config/source paths before external reads")
        require(run_owner["config_path"] in run["spec"]["input_pins"] and run_owner["config_stamp"] == run["spec"]["input_pins"][run_owner["config_path"]], "original selected config path/stamp cannot be substituted by forged event pins")
        for field, original in (("root_identity", run["identity"]["root"]), ("parent_identity", run["identity"]["parent"]), ("control_identity", run["identity"]["control"]), ("attempts_identity", run["identity"]["attempts"]), ("owner_identity", run["identity"]["owner"]["identity"]), ("lock_identity", run["identity"]["worker"]["identity"])):
            require(run_owner[field] == original, "complete Run owner identity matches external sealed authority witness")
        for unit, identity in run["identity"]["units"].items():
            require(paths.actual_identity(rroot / "attempts" / unit) == identity, "sealed original unit directory identity")
        changed = []
        for path, expected in run["spec"]["input_pins"].items():
            try:
                observed, _ = observed_stamp(Path(path))
                if observed != expected: changed.append(path)
            except (AuditError, OSError):
                changed.append(path)
        require(not changed or run["outcome"] == "invalidated", "changed selected inputs require retained invalidation and no new work")
        run["inputs_changed"] = bool(changed)
        input_checks[run_id] = {"input_pins_match": not changed, "changed_selected_paths": changed}
    for key, a in ledger.attempts.items(): originals.append(verify_attempt_originals(a, ledger.runs[key[0]], fixture))
    after = paths.inventory(root)
    require(before == after, "read-only audit preserved complete authority identity/mtime/bytes")
    return ledger, {"authority_owner": owner, "sequence": ledger.sequence, "last_sha256": ledger.last_sha, "totals": ledger.totals(), "run_totals": {r: ledger.totals(r) for r in ledger.runs}, "attempt_originals": originals, "input_checks": input_checks, "before": before, "after": after}


def verify_server_receipts(ledger, receipts, artifact_root):
    """Actual server records prove observed requests separately from MaySend."""
    seen = set()
    response_originals = []
    for receipt in receipts:
        request_id = uuid(receipt["request_id"], "server received request")
        require(request_id not in seen, "same request_id actually resent")
        seen.add(request_id)
        matching = [(k, a) for k, a in ledger.attempts.items() if k[3] == request_id]
        require(len(matching) == 1, "server request must bind one actual persistent attempt")
        key, a = matching[0]
        raw = paths.read_observed_file(Path(artifact_root) / paths.safe_relative(receipt["request_body_file"]))
        require(sha(raw) == receipt["request_body_sha256"] == a["plan"]["request_body"]["sha256"] and len(raw) == receipt["request_body_byte_length"] == a["plan"]["request_body"]["byte_length"], "actual HTTP body exact persistent original")
        require(key[:3] == (receipt["run_id"], receipt["unit_id"], receipt["attempt_no"]), "server real Run/WorkUnit/attempt mapping")
        require(a["state"] in ("may_send", "unknown", "known"), "server observed request has no dispatch permission")
        starts = [utc_ns(r["at_utc"]) for r in ledger.rows if r["event"]["kind"] == "may_send" and validate_key(r["event"]["key"]) == key]
        require(len(starts) == 1 and receipt["request_received_utc_ns"] >= starts[0], "persistent permission precedes actual server receive")
        require(receipt["authorization_matches_artificial_test_key"] is True and receipt["method"] == "POST" and receipt["path"] == "/v1/responses", "actual artificial request transport")
        if "response_file" in receipt:
            response = paths.read_observed_file(Path(artifact_root) / paths.safe_relative(receipt["response_file"]))
            require(sha(response) == receipt["response_sha256"] and len(response) == receipt["response_byte_length"], "actual server response original SHA/length")
            attempt_root = Path(ledger.runs[key[0]]["spec"]["run_root"]) / paths.safe_relative(a["plan"]["attempt_root"])
            saved_response = attempt_root / "response.bin"
            if saved_response.exists():
                require(paths.read_observed_file(saved_response) == response, "received API response equals actual server response bytes")
            response_originals.append({"request_id": request_id, "sha256": sha(response), "byte_length": len(response), "advertised_status": receipt["advertised_status"], "actual_response_flush": "response_flush_utc_ns" in receipt})
        else:
            require(a["state"] != "known", "Known receipt requires actual server response original")
    require(all(k[3] in seen for k, a in ledger.attempts.items() if a["state"] == "known"), "all Known network attempts require actual server observations")
    return {"observed_HTTP_requests": len(receipts), "unique_received_request_ids": len(seen), "response_originals": response_originals, "possibly_dispatched_not_observed": [k[3] for k, a in ledger.attempts.items() if a["state"] in ("may_send", "unknown", "known") and k[3] not in seen]}


class ArtificialHttp:
    """Real numeric-loopback HTTP, routing only by already-persisted UUID binding."""
    def __init__(self, root, fixture, routes, steps, lookup_attempt, before_response=None):
        self.root = paths.canonical_selected_path(root)
        self.fixture = paths.canonical_selected_path(fixture)
        self.routes = routes  # actual import UUID -> author unit name
        self.steps = steps
        self.lookup_attempt = lookup_attempt
        self.before_response = before_response
        self.receipts = []
        self.incoming = []
        self.errors = []
        self.handler_threads = []
        self.lock = threading.Lock()
        self.release_response = threading.Event()
        self.accepted = threading.Event()
        self.stopped = threading.Event()
        service = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_POST(self):
                number = None
                receipt = None
                with service.lock:
                    number = len(service.incoming) + 1
                    incoming = {"number": number, "method": "POST", "path": self.path, "request_started_utc_ns": time.time_ns(), "request_started_monotonic_ns": time.monotonic_ns(), "authorization_matches_artificial_test_key": self.headers.get("Authorization") == "Bearer " + TEST_KEY, "body_complete": False}
                    service.incoming.append(incoming)
                    service.persist()
                try:
                    require(self.path == "/v1/responses", "local synthetic Responses path")
                    require(self.headers.get("Authorization") == "Bearer " + TEST_KEY, "artificial credential only")
                    length = int(self.headers.get("Content-Length", "-1"))
                    require(0 < length <= 65536, "bounded actual body")
                    raw = self.rfile.read(length)
                    require(len(raw) == length, "complete actual request received")
                    received_utc_ns, received_monotonic_ns = time.time_ns(), time.monotonic_ns()
                    with service.lock:
                        (service.root / f"incoming-{number:03}.bin").write_bytes(raw)
                        incoming.update(body_complete=True, request_body_file=f"incoming-{number:03}.bin", request_body_sha256=sha(raw), request_body_byte_length=len(raw), request_received_utc_ns=received_utc_ns, request_received_monotonic_ns=received_monotonic_ns)
                        service.persist()
                    bound = request_body_check(raw, service.fixture)
                    unit = service.routes.get(bound["binding"]["import_id"])
                    require(unit is not None, "actual selected source import routing")
                    key = service.lookup_attempt(bound["request_id"])
                    require(key[1] == unit and key[3] == bound["request_id"], "UUID router matches Prepared attempt")
                    with service.lock:
                        used = sum(r["unit_id"] == unit for r in service.receipts)
                        require(used < len(service.steps[unit]), "unexpected extra unit HTTP attempt")
                        step = service.steps[unit][used]
                        require(not step.startswith("must-not-") and step != "subsequent-budget-denied", "forbidden extra request actually received")
                        request_file = f"request-{number:03}.bin"
                        (service.root / request_file).write_bytes(raw)
                        receipt = {"method": "POST", "path": self.path, "request_id": bound["request_id"], "run_id": key[0], "unit_id": key[1], "attempt_no": key[2], "step": step, "request_body_file": request_file, "request_body_sha256": sha(raw), "request_body_byte_length": len(raw), "request_received_utc_ns": received_utc_ns, "request_received_monotonic_ns": received_monotonic_ns, "authorization_matches_artificial_test_key": True}
                        service.receipts.append(receipt)
                        service.persist()
                    service.accepted.set()
                    if step in ("success", "success-held", "preceding-success"):
                        candidate = strict_json(paths.read_observed_file(service.fixture / "source/candidate-template.json"))
                        candidate.pop("producer")
                        candidate["request_id"], candidate["binding"] = bound["request_id"], bound["binding"]
                        wire = strict_json(paths.read_observed_file(service.fixture / "wire/success-envelope-template.json"))
                        wire["output"][0]["content"][0]["text"] = canonical(candidate).decode()
                        response, status = canonical(wire), 200
                    elif step == "fixed-invalid-candidate":
                        response, status = paths.read_observed_file(service.fixture / "wire/fixed-200-invalid-candidate.bin"), 200
                    elif step in ("http-429", "http-503", "http-401"):
                        response, status = paths.read_observed_file(service.fixture / ("wire/" + step + ".bin")), int(step[-3:])
                    elif step == "invalid-json":
                        response, status = paths.read_observed_file(service.fixture / "wire/invalid-json-200.bin"), 200
                    elif step in ("unknown-after-accept", "timeout", "disconnect", "truncated"):
                        response, status = b"", 200
                    else:
                        raise AuditError("unknown frozen response step")
                    if service.before_response is not None:
                        service.before_response(unit, key, step)
                    if step == "success-held":
                        require(service.release_response.wait(10), "held response not released by actual audit")
                    if step in ("unknown-after-accept", "timeout"):
                        service.stopped.wait(0.5)
                    if step == "disconnect":
                        self.connection.shutdown(socket.SHUT_RDWR)
                        self.connection.close()
                        return
                    with service.lock:
                        response_file = f"response-{number:03}.bin"
                        (service.root / response_file).write_bytes(response)
                        receipt.update(advertised_status=status, response_file=response_file, response_sha256=sha(response), response_byte_length=len(response), response_before_write_utc_ns=time.time_ns(), response_before_write_monotonic_ns=time.monotonic_ns(), advertised_content_length=len(response) + (17 if step == "truncated" else 0))
                        service.persist()
                    self.send_response(status)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(receipt["advertised_content_length"]))
                    self.send_header("Connection", "close")
                    self.send_header("x-request-id", "req_noetome_test")
                    self.end_headers()
                    self.wfile.write(response)
                    self.wfile.flush()
                    self.close_connection = True
                    with service.lock:
                        receipt["response_flush_utc_ns"] = time.time_ns()
                        receipt["response_flush_monotonic_ns"] = time.monotonic_ns()
                        service.persist()
                except (BrokenPipeError, ConnectionResetError, OSError) as error:
                    if receipt is not None:
                        with service.lock:
                            receipt["actual_socket_exception"] = type(error).__name__
                            service.persist()
                except BaseException as error:
                    with service.lock:
                        service.errors.append({"type": type(error).__name__, "text": str(error)})
                        service.persist()
        class TrackingServer(ThreadingHTTPServer):
            def process_request(self, request, client_address):
                thread = threading.Thread(target=self.process_request_thread, args=(request, client_address), daemon=True)
                with service.lock:
                    service.handler_threads.append(thread)
                thread.start()
        self.server = TrackingServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.endpoint = f"http://127.0.0.1:{self.server.server_port}/v1/responses"
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)

    def persist(self):
        save_json(self.root / "server-receipts.json", {"receipts": self.receipts, "incoming": self.incoming, "observed_HTTP_request_count": len(self.incoming), "errors": self.errors, "server_PID": os.getpid(), "scope": "actual artificial numerical-loopback HTTP; authentication header omitted; zero real models/fees"})

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stopped.set()
        self.release_response.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(2)
        require(not self.thread.is_alive(), "artificial HTTP acceptor joined")
        with self.lock:
            handlers = list(self.handler_threads)
        for thread in handlers:
            thread.join(2)
        require(all(not thread.is_alive() for thread in handlers), "all actual local HTTP handlers joined before receipt/source guards")
        self.persist()


def clean_environment(extra=None):
    names = ("PATH", "SYSTEMROOT", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP", "LANG", "LC_ALL")
    result = {n: os.environ[n] for n in names if n in os.environ}
    result.update(PYTHONIOENCODING="utf-8", PYTHONUTF8="1", NO_PROXY="127.0.0.1,::1", no_proxy="127.0.0.1,::1", NOETOME_TEST_CREDENTIAL=TEST_KEY)
    if extra:
        require(all(k.startswith("NOETOME_REQUEST_TEST_") for k in extra), "only explicit request test driver environment")
        result.update(extra)
    return result


def streaming_pin(path):
    path = paths.ordinary_path(path)
    before = path.stat()
    require(path.is_file() and before.st_nlink == 1, "selected executable single-link ordinary file")
    digest_value = hashlib.sha256()
    with path.open("rb") as f:
        for part in iter(lambda: f.read(1024 * 1024), b""): digest_value.update(part)
    after = path.stat()
    require((before.st_dev, before.st_ino, before.st_mtime_ns, before.st_size) == (after.st_dev, after.st_ino, after.st_mtime_ns, after.st_size), "selected executable changed during hash")
    return {"sha256": digest_value.hexdigest(), "byte_length": before.st_size, "identity": paths.actual_identity(path), "modified": str(before.st_mtime_ns)}


class Recorder:
    def __init__(self, root, cli, fixture=FIXTURE):
        author_check(fixture)
        self.root = paths.canonical_selected_path(root, leaf_missing=True)
        require(not self.root.exists(), "artifact root must be new; no adoption or cleanup")
        self.root.mkdir()
        self.root = paths.canonical_selected_path(self.root)
        self.cli = paths.canonical_selected_path(cli)
        self.fixture = paths.canonical_selected_path(fixture)
        self.cli_before = streaming_pin(self.cli)
        self.build_fingerprint = None
        self.author_before = paths.inventory(self.fixture)
        self.calls = []
        self.launch_lock = threading.Lock()
        self.command_number = 0
        self.driver_pins = {}
        self.children = {}
        self.cases = []
        self.commands = self.root / "commands"
        self.commands.mkdir()
        self.sentinel = self.root / "external-sentinel.txt"
        self.sentinel.write_bytes("用户哨兵；与所有 product 根分离。\n".encode())
        self.sentinel_before = observed_stamp(self.sentinel)[0]
        self.checker_files = [p for p in (Path(__file__), ROOT / "tests/test_verify_request_budget.py", Path(paths.__file__)) if p.exists()]
        self.checker_before = {str(p): observed_stamp(p)[0] for p in self.checker_files}
        copy_root = self.root / "checker-source"; copy_root.mkdir()
        for path in self.checker_files:
            shutil.copy2(path, copy_root / path.name)
            require(sha(paths.read_observed_file(copy_root / path.name)) == self.checker_before[str(path)]["sha256"], "checker original byte copy matches actual source before launch")
        save_json(self.root / "checker-source-before.json", self.checker_before)
        save_json(self.root / "author-source-before.json", self.author_before)
        save_json(self.root / "runtime.json", {"python": sys.executable, "python_version": sys.version, "platform": sys.platform, "OS": os.name, "outer_PID": os.getpid(), "selected_cli": str(self.cli), "cli_before": self.cli_before, "fixture": str(self.fixture), "freeze_sha256": FROZEN_SHA, "scope": "actual explicit artificial loopback only; no production credentials/budget/Vault"})

    def bind_build_receipt(self, receipt_path, driver=None):
        selected = paths.canonical_selected_path(receipt_path)
        receipt_stamp, raw = observed_stamp(selected)
        receipt = strict_json(raw)
        require(receipt["source_before_after_equal"] is True and {k: receipt["cli"][k] for k in ("sha256", "byte_length")} == {k: self.cli_before[k] for k in ("sha256", "byte_length")}, "selected actual CLI binds explicit source-build receipt")
        fingerprint = digest(receipt["actual_cli_fingerprint"], "source-build actual compiler fingerprint")
        require(receipt["cargo_build_script_fingerprints"] == [fingerprint], "actual Cargo source fingerprint equals actual CLI probe")
        if driver is not None:
            actual_driver = streaming_pin(paths.canonical_selected_path(driver))
            require({k: receipt["test_driver"][k] for k in ("sha256", "byte_length")} == {k: actual_driver[k] for k in ("sha256", "byte_length")}, "selected driver binds same actual source-build receipt")
        before = strict_json(paths.read_observed_file(selected.parent / "source-before.json"))
        after = strict_json(paths.read_observed_file(selected.parent / "source-after.json"))
        require(before == after and len(before) == receipt["source_files"], "actual whole selected build source inventories before/after equal")
        archive = selected.parent / "source-originals.zip"
        archive_pin = streaming_pin(archive)
        require({k: archive_pin[k] for k in ("sha256", "byte_length")} == {k: receipt["source_archive"][k] for k in ("sha256", "byte_length")}, "source archive SHA/length anchored by actual build receipt")
        member_pins = {}
        with zipfile.ZipFile(archive) as source:
            require(set(source.namelist()) == set(before) and len(source.namelist()) == len(before), "exact selected compilation source archive member inventory")
            for name in source.namelist():
                paths.safe_relative(name)
                data = source.read(name)  # ZipFile checks actual CRC; bytes are never extracted.
                pin = {"sha256": sha(data), "byte_length": len(data)}
                require(pin == {k: before[name][k] for k in pin}, "each archived actual build source original matches source-before/after")
                member_pins[name] = pin
        require(observed_stamp(selected)[0] == receipt_stamp, "explicit build receipt original preserved")
        self.build_fingerprint = fingerprint
        save_json(self.root / "source-build-binding.json", {"explicit_build_receipt": str(selected), "original_receipt_stamp": receipt_stamp, "receipt_original": receipt, "selected_CLI": self.cli_before, "selected_driver": None if driver is None else actual_driver, "source_archive": str(archive), "source_members": member_pins, "scope": "actual selected executable body, Cargo/runtime fingerprint and original archived compilation inputs; independent semantic expectations remain frozen author"})
        (self.root / "build-receipt-original.json").write_bytes(raw)

    def launch(self, args, *, label=None, extra=None, executable=None):
        with self.launch_lock:
            self.command_number += 1
            number = self.command_number
        folder = self.commands / f"{number:04}"
        folder.mkdir()
        cli = self.cli if executable is None else paths.canonical_selected_path(executable)
        stdout = (folder / "stdout.bin").open("wb")
        stderr = (folder / "stderr.bin").open("wb")
        argv = [str(cli), *map(str, args)]
        actual_executable = streaming_pin(cli)
        selected_executable_sha = actual_executable["sha256"]
        require(selected_executable_sha == self.cli_before["sha256"] if executable is None else True, "selected CLI changed before launch")
        if executable is not None:
            with self.launch_lock:
                if str(cli) in self.driver_pins:
                    require(self.driver_pins[str(cli)] == actual_executable, "selected driver changed between launches")
                else:
                    self.driver_pins[str(cli)] = actual_executable
        started = time.time_ns()
        child = subprocess.Popen(argv, stdout=stdout, stderr=stderr, stdin=subprocess.PIPE, env=clean_environment(extra))
        record = {"number": number, "label": label, "argv": argv, "PID": child.pid, "started_utc_ns": started, "started_monotonic_ns": time.monotonic_ns(), "selected_executable_sha256": selected_executable_sha, "command_kind": "ordinary_cli" if executable is None else "explicit_test_driver", "fake_environment_keys": sorted(clean_environment(extra)), "credential_value_omitted": True}
        if extra is not None:
            record["explicit_request_test_selection"] = dict(extra)
        save_json(folder / "launch.json", record)
        with self.launch_lock:
            self.calls.append(record)
            self.children[child.pid] = (child, stdout, stderr, folder, record)
        return child

    def wait(self, child, *, expected=None, timeout=30, action="ordinary_exit"):
        require(child.pid in self.children, "only own launched PID may be waited or terminated")
        _, out, err, folder, record = self.children[child.pid]
        try:
            code = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            child.kill()
            code = child.wait(timeout=10)
            action = "timeout_actual_force_kill"
        out.close(); err.close()
        if child.stdin: child.stdin.close()
        stdout, stderr = (folder / "stdout.bin").read_bytes(), (folder / "stderr.bin").read_bytes()
        record.update(exit_code=code, finished_utc_ns=time.time_ns(), finished_monotonic_ns=time.monotonic_ns(), wait_action=action, stdout={"sha256": sha(stdout), "byte_length": len(stdout)}, stderr={"sha256": sha(stderr), "byte_length": len(stderr)})
        save_json(folder / "receipt.json", record)
        del self.children[child.pid]
        require(TEST_KEY.encode() not in stdout + stderr, "artificial credential not exposed to terminal")
        if expected is not None:
            allowed = {expected} if isinstance(expected, int) else set(expected)
            require(code in allowed, "actual CLI exit mismatch " + str(record["number"]) + ": " + stderr.decode(errors="replace")[:1024])
        require(action != "timeout_actual_force_kill", "ordinary command timed out; original PID/kill/wait receipt retained")
        return stdout, stderr, record

    def command(self, args, *, expected=None, label=None):
        return self.wait(self.launch(args, label=label), expected=expected)

    def force_kill(self, child):
        require(child.pid in self.children and child.poll() is None, "only alive own PID may be killed")
        requested = time.time_ns()
        child.kill()
        result = self.wait(child, action="actual_OS_force_kill_and_wait")
        result[2]["kill_requested_utc_ns"] = requested
        save_json(self.commands / f'{result[2]["number"]:04}' / "receipt.json", result[2])
        require(child.poll() is not None, "old worker still has sending capability")
        require(result[2]["exit_code"] == -signal.SIGKILL if os.name != "nt" else result[2]["exit_code"] != 0, "force kill must have actual OS termination exit code; natural exit is not interruption evidence")
        return result

    def finish(self, status, failure=None):
        for child, *_ in list(self.children.values()):
            if child.poll() is None: self.force_kill(child)
            else: self.wait(child)
        cli_after = streaming_pin(self.cli)
        author_after = paths.inventory(self.fixture)
        checker_after = {str(p): observed_stamp(p)[0] for p in self.checker_files}
        sentinel_after = observed_stamp(self.sentinel)[0]
        driver_after = {p: streaming_pin(Path(p)) for p in self.driver_pins}
        guards = {"CLI_before_after_equal": self.cli_before == cli_after, "drivers_before_after_equal": self.driver_pins == driver_after, "author_before_after_equal": self.author_before == author_after, "checker_before_after_equal": self.checker_before == checker_after, "external_sentinel_before_after_equal": self.sentinel_before == sentinel_after}
        result = {"status": status, "failure": failure, "commands": self.calls, "ordinary_CLI_count": sum(c["command_kind"] == "ordinary_cli" for c in self.calls), "driver_count": sum(c["command_kind"] == "explicit_test_driver" for c in self.calls), "cases": self.cases, "guards": guards, "cli_after": cli_after, "driver_before": self.driver_pins, "driver_after": driver_after, "real_model_calls": 0, "paid_calls": 0, "production_credentials_or_ledger_accessed": False, "scope": "actual raw outcomes only; unexecuted groups remain not_run"}
        save_json(self.root / "actual.json", result)
        save_json(self.root / "checker-source-after.json", checker_after)
        save_json(self.root / "author-source-after.json", author_after)
        require(all(guards.values()), "actual final original/source guards failed")
        return result


def lookup_persistent_attempt(budget_root, request_id, authority_config):
    """Read the committed event chain before the router supplies any response."""
    budget_root = paths.canonical_selected_path(budget_root)
    owner = strict_json(paths.read_observed_file(budget_root / "control/owner.json"))
    with closing(sqlite3.connect(paths.sqlite_file_uri(budget_root / "control/budget.sqlite"), uri=True, timeout=5)) as db:
        rows = list(db.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM events ORDER BY sequence"))
    bind_original_authority(owner["authority"], authority_config)
    ledger = reconstruct_events(owner["authority"], rows)
    keys = [k for k, a in ledger.attempts.items() if k[3] == request_id and a["state"] in ("may_send", "unknown", "known")]
    require(len(keys) == 1, "server request requires one committed MaySend permission")
    return keys[0]


def prepare_case(recorder, case, name=None):
    case_root = recorder.root / (name or case["case_id"])
    case_root.mkdir()
    sources = case_root / "sources"; sources.mkdir()
    server_root = case_root / "server"; server_root.mkdir()
    projects, imports, routes = {}, {}, {}
    for unit in case["units"]:
        project = sources / unit
        recorder.command(["project", "init", project], expected=0, label="source-init " + unit)
        stdout, _, _ = recorder.command(["source", "import", project, recorder.fixture / "source/source.txt"], expected=0, label="source-import " + unit)
        imported = strict_json(stdout)
        projects[unit], imports[unit] = paths.canonical_selected_path(project), imported
        routes[imported["import_id"]] = unit
    budget = case_root / "budget"
    run_configs, run_roots = {}, {}
    authority = strict_json(paths.read_observed_file(recorder.fixture / "policies/authority-template.json"))
    authority["global_budget"].update(case["policy_overrides"].get("global", {}))
    if "authority_opening_template" in case:
        authority = strict_json(paths.read_observed_file(recorder.fixture / case["authority_opening_template"]))
    authority_file = case_root / "authority-config.json"; save_json(authority_file, authority)
    for run_name in dict.fromkeys(case["execution_layout"]["unit_to_run"].values()):
        config = strict_json(paths.read_observed_file(recorder.fixture / "policies/run-template.json"))
        config["units"] = []
        config["run_budget"].update(case["policy_overrides"].get("run", {}))
        config["retry"].update(case["policy_overrides"].get("retry", {}))
        for unit, owner_run in case["execution_layout"]["unit_to_run"].items():
            if owner_run != run_name: continue
            definition = {"unit_id": unit, "project": str(projects[unit]), "import_id": imports[unit]["import_id"], "api_config": str(case_root / (unit + "-api.json")), "max_attempts": 3, "max_retry_tokens": 16620, "max_elapsed_ms": 5000}
            definition.update(case["policy_overrides"].get(unit, {}))
            config["units"].append(definition)
        if len(config["units"]) == 1: config["concurrency"] = 1
        config_file = case_root / (run_name + "-config.json"); save_json(config_file, config)
        run_configs[run_name], run_roots[run_name] = config_file, case_root / run_name
    inputs_before = {str(p): paths.inventory(p) for p in projects.values()}
    save_json(case_root / "source-before.json", inputs_before)
    return {"root": case_root, "budget": budget, "authority_file": authority_file, "configs": run_configs, "run_roots": run_roots, "projects": projects, "imports": imports, "routes": routes, "source_before": inputs_before, "server_root": server_root}


def case_expected_check(case, ledger, traffic):
    e = case["expected"]
    total = ledger.totals()
    require(traffic["observed_HTTP_requests"] == e["observed_HTTP_requests"], "author actual observed HTTP count")
    require(total["accounted_tokens"] == e["known_tokens"] and total["held_tokens"] == e["unknown_commitment_tokens"] and add(total["accounted_tokens"], total["held_tokens"]) == e["total_committed_tokens"], "independently rebuilt author budget expected")
    if e.get("identical_complete_response_bytes"):
        responses = traffic["response_originals"]
        require(len(responses) == e["observed_HTTP_requests"] and all(r["sha256"] == e["same_response_sha256"] and r["advertised_status"] == 200 and r["actual_response_flush"] for r in responses), "all distinct requests received exact frozen identical complete response bytes")
        known = [a for a in ledger.attempts.values() if a["state"] == "known"]
        require(len(known) == len(responses) and all(a["receipt"]["usage"]["total_tokens"] == e["known_tokens_per_distinct_attempt"] for a in known), "same response SHA settles each distinct attempt independently")
    known_units = [u for r in ledger.runs.values() for u, state in r["units"].items() if state["outcome"] == "succeeded"]
    failed_units = [u for r in ledger.runs.values() for u, state in r["units"].items() if state["outcome"] == "failed"]
    if "candidate_validated_units" in e: require(sorted(known_units) == sorted(e["candidate_validated_units"]), "exact author independent successful units")
    if "candidate_validated_unit_count" in e: require(len(known_units) == e["candidate_validated_unit_count"] and set(known_units) <= set(e["candidate_validated_unit_subset"]), "one nondeterministic budget winner")
    require(sorted(failed_units) == sorted(e["isolated_failed_units"]), "author isolated failed units")
    unknown = [a for a in ledger.attempts.values() if a["state"] in ("unknown", "may_send")]
    require(len(unknown) == e["unknown_commitment_attempts"] and all(a["plan"]["reserved_tokens"] == 8310 for a in unknown), "all unknown attempts retain entire R8310")
    if "budget_stopped_unfinished_units" in e:
        require(sum(u["outcome"] == "budget_stopped" for r in ledger.runs.values() for u in r["units"].values()) == e["budget_stopped_unfinished_units"], "budget stopped unfinished unit retained")


def verify_api_envelope(value, ledger):
    """Compare declared API output with the independent raw-event prefix."""
    fields(value, "schema_version producer operation budget real_model_calls paid_calls price_usd published knowledge_status scientific_quality" if value.get("operation") == "budget_init" else "schema_version producer operation run_id run_status budget_root run_root compiler_fingerprint run budget real_model_calls paid_calls price_usd published knowledge_status scientific_quality", "ordinary API report")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1 and value["producer"] == "synthetic_api_run" and type(value["real_model_calls"]) is int and value["real_model_calls"] == 0 and type(value["paid_calls"]) is int and value["paid_calls"] == 0 and value["price_usd"] is None and value["published"] is False and value["knowledge_status"] == "pending" and value["scientific_quality"] == "not_run", "ordinary report retains synthetic/pending/unpublished scope")
    budget = fields(value["budget"], "schema_version authority sequence last_sha256 global_stopped totals runs price_usd", "budget projection")
    require(type(budget["schema_version"]) is int and budget["schema_version"] == 1 and budget["authority"] == ledger.authority and budget["sequence"] == ledger.sequence and budget["last_sha256"] == ledger.last_sha and budget["global_stopped"] is ledger.global_stopped and budget["price_usd"] is None, "report budget authority and complete event chain prefix")
    total = ledger.totals(); caps = ledger.authority["global_limits"]
    expected_totals = {**total, "remaining_tokens": max(0, caps["max_total_tokens"] - add(total["accounted_tokens"], total["held_tokens"])), "remaining_requests": max(0, caps["max_requests"] - add(total["dispatched_requests"], total["held_requests"]))}
    require(budget["totals"] == expected_totals and all(type(v) is int for v in budget["totals"].values()), "report budget totals independently reconstructed, including remaining capacity")
    require(isinstance(budget["runs"], list) and len(budget["runs"]) == len(ledger.runs), "report exact registered Run count")
    seen = set()
    def diagnostic(kind, run_id, unit=None):
        matching = [r["event"]["diagnostic"] for r in ledger.rows if r["event"]["kind"] in kind and r["event"]["run_id"] == run_id and (unit is None or r["event"].get("unit_id") == unit)]
        return matching[-1] if matching else None
    for reported in budget["runs"]:
        fields(reported, "spec identity units outcome diagnostic budget_stopped", "reported Run")
        run_id = reported["spec"]["run_id"]
        require(run_id in ledger.runs and run_id not in seen, "report one unique original Run")
        seen.add(run_id); run = ledger.runs[run_id]
        require(reported["spec"] == run["spec"] and reported["identity"] == run["identity"] and reported["outcome"] == run["outcome"] and reported["diagnostic"] == diagnostic({"run_finished", "run_invalidated"}, run_id), "report original Run source/identity/terminal facts")
        unit_seen = set()
        for unit in reported["units"]:
            fields(unit, "spec attempts outcome diagnostic", "reported WorkUnit")
            name = unit["spec"]["unit_id"]
            require(name in run["units"] and name not in unit_seen, "report one original WorkUnit")
            unit_seen.add(name); original_unit = run["units"][name]
            require(unit["spec"] == original_unit["spec"] and unit["outcome"] == original_unit["outcome"] and unit["diagnostic"] == diagnostic({"unit_finished"}, run_id, name) and len(unit["attempts"]) == len(original_unit["attempts"]), "report WorkUnit bounds, outcome and attempt count")
            for reported_attempt, key in zip(unit["attempts"], original_unit["attempts"]):
                fields(reported_attempt, "plan identity sealed_files transport_sealed state receipt unknown_reason retry", "reported attempt")
                a = ledger.attempts[key]
                require(reported_attempt == {"plan": a["plan"], "identity": a["identity"], "sealed_files": a["files"], **{f: a[f] for f in ("transport_sealed", "state", "receipt", "unknown_reason", "retry")}}, "report each distinct attempt equals durable original prefix; no response-SHA aggregation")
        require(unit_seen == set(run["units"]), "report complete unit inventory")
        rt = ledger.totals(run_id); rc = run["spec"]["run_limits"]
        over = any(a["receipt"] is not None and a["receipt"]["usage"] is not None and (a["receipt"]["usage"]["input_tokens"] > a["plan"]["input_reserved_tokens"] or a["receipt"]["usage"]["output_tokens"] > a["plan"]["max_output_tokens"] or a["receipt"]["usage"]["total_tokens"] > a["plan"]["reserved_tokens"]) for key, a in ledger.attempts.items() if key[0] == run_id)
        require(reported["budget_stopped"] is (over or add(rt["accounted_tokens"], rt["held_tokens"]) > rc["max_total_tokens"] or add(rt["dispatched_requests"], rt["held_requests"]) > rc["max_requests"]), "report budget stop grounded in actual usage/caps")
    if value["operation"] != "budget_init":
        require(value["run_id"] in ledger.runs and value["run"] == next(r for r in budget["runs"] if r["spec"]["run_id"] == value["run_id"]), "ordinary top-level selected Run equals independent budget projection")
        run = ledger.runs[value["run_id"]]
        require(value["run_root"] == run["spec"]["run_root"] and value["run_status"] == (run["outcome"] or "paused_or_interrupted"), "ordinary root and status agree with original selected Run")
        digest(value["compiler_fingerprint"], "ordinary executed compiler fingerprint")


def verify_saved_envelopes(recorder, selected, final_ledger):
    rows = [(r["sequence"], r["previous_sha256"], canonical(r), sha(canonical(r))) for r in final_ledger.rows]
    checked = []
    for record in recorder.calls:
        if (str(selected["budget"]) not in record["argv"] and record.get("explicit_request_test_selection", {}).get("NOETOME_REQUEST_TEST_BUDGET_ROOT") != str(selected["budget"])) or "exit_code" not in record: continue
        folder = recorder.commands / f'{record["number"]:04}'
        if record["command_kind"] == "ordinary_cli":
            raw = paths.read_observed_file(folder / "stdout.bin")
            if not raw: continue
            values = [strict_json(raw)]
        else:
            stream = strict_json(paths.read_observed_file(folder / "driver-stream.json"))
            values = [r["result"] for r in stream["results"] if "result" in r]
        for value in values:
            sequence = natural(value["budget"]["sequence"], "actual report prefix", final_ledger.sequence)
            prefix = reconstruct_events(final_ledger.authority, rows[:sequence])
            verify_api_envelope(value, prefix)
            if value["operation"] != "budget_init":
                owner = strict_json(paths.read_observed_file(selected["budget"] / "control/owner.json"))
                require(value["compiler_fingerprint"] == owner["compiler_fingerprint"] and value["budget_root"] == str(selected["budget"]), "report compiler/root equals actually selected CLI-owned authority")
            checked.append({"actual_command_number": record["number"], "actual_PID": record["PID"], "event_prefix_sequence": sequence, "original_stdout_sha256": record["stdout"]["sha256"]})
    require(checked, "actual ordinary/driver API reports independently checked")
    save_json(selected["root"] / "independent-cli-reports.json", {"checked_original_reports": checked})


def ordinary_case(recorder, case, test_driver=None):
    selected = prepare_case(recorder, case)
    server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], case["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
    with server:
        for unit in case["units"]:
            config = strict_json(paths.read_observed_file(recorder.fixture / "policies/single-request-api-template.json"))
            config["endpoint"] = server.endpoint
            save_json(selected["root"] / (unit + "-api.json"), config)
        recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0, label=case["case_id"] + " authority-init")
        mode = case["execution_layout"]["kind"]
        children = []
        drivers = []
        if mode == "two-sequential-runs-one-unit-each":
            for name in selected["configs"]:
                recorder.command(["run", "api", "start", selected["configs"][name], selected["budget"], selected["run_roots"][name]], expected=(0, 3), label=case["case_id"] + " " + name)
        else:
            def launch_run(name):
                if test_driver is not None:
                    observed = RequestDriver(recorder, selected, test_driver, event="observe", run_name=name)
                    return observed.child, observed
                return recorder.launch(["run", "api", "start", selected["configs"][name], selected["budget"], selected["run_roots"][name]], label=case["case_id"] + " " + name), None
            if mode == "two-concurrent-runs-one-unit-each":
                # Hash each actual selected executable before Popen, concurrently;
                # verifier I/O cannot serially delay the second real worker past
                # the first request's unchanged 200ms author HTTP deadline.
                with ThreadPoolExecutor(max_workers=2) as pool:
                    futures = [pool.submit(launch_run, name) for name in selected["configs"]]
                    launched = [future.result() for future in futures]
            else:
                launched = [launch_run(name) for name in selected["configs"]]
            for child, observed in launched:
                children.append(child)
                if observed is not None: drivers.append(observed)
            if "simultaneous_real_workers" in case["expected"]:
                require(server.accepted.wait(10), "actual first near-cap request not received")
                hold_start = time.monotonic_ns()
                # Preserve a real held-response interval; independently inspect the
                # event chain before releasing, without claiming worker count.
                owner = strict_json(paths.read_observed_file(selected["budget"] / "control/owner.json"))
                while True:
                    if server.errors: raise AuditError(str(server.errors))
                    with closing(sqlite3.connect(paths.sqlite_file_uri(selected["budget"] / "control/budget.sqlite"), uri=True)) as db:
                        held = reconstruct_events(owner["authority"], list(db.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM events ORDER BY sequence")))
                    if {key[1] for key in held.attempts} == set(case["units"]) and time.monotonic_ns() - hold_start >= 30_000_000: break
                    require(time.monotonic_ns() - server.receipts[0]["request_received_monotonic_ns"] < 150_000_000, "other actual concurrent unit did not prepare before held-response timing cap")
                    threading.Event().wait(0.005)
                require(len(server.receipts) == 1 and held.totals()["held_tokens"] == 8310 and held.totals()["dispatched_requests"] == 1 and {key[1] for key in held.attempts} == set(case["units"]), "both actual units prepared while only one admission/response is held")
                save_json(selected["root"] / "held-response-observation.json", {"observed_requests": len(server.receipts), "totals": held.totals(), "actual_outer_child_PIDs": [c.pid for c in children], "prepared_attempts": [a["plan"]["key"] for a in held.attempts.values()], "worker_count_from_config_is_not_runtime_proof": True})
                server.release_response.set()
            if drivers:
                for observed in drivers: observed.finish()
                if "simultaneous_real_workers" in case["expected"]:
                    thread_ids = {(row["pid"], row["thread_id"]) for observed in drivers for row in observed.handshakes if row["unit"] in case["units"]}
                    require(len(thread_ids) == 2, "two actual Rust worker identities, separately scoped from OS TIDs")
                    save_json(selected["root"] / "observed-worker-identities.json", {"actual_process_RustThreadId_pairs": sorted(thread_ids), "all_callbacks_nonblocking": all(not row["will_wait"] for observed in drivers for row in observed.handshakes), "not_OS_thread_ids": True})
            else:
                for child in children: recorder.wait(child, expected=(0, 3))
        require(not server.errors and len(server.incoming) == len(server.receipts), "actual local service error or rejected incoming request: " + str(server.errors))
    ledger, evidence = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
    traffic = verify_server_receipts(ledger, server.receipts, selected["server_root"])
    case_expected_check(case, ledger, traffic)
    sources_after = {str(p): paths.inventory(p) for p in selected["projects"].values()}
    require(sources_after == selected["source_before"], "source project inventory/SQL/identity/mtime preserved")
    save_json(selected["root"] / "source-after.json", sources_after)
    save_json(selected["root"] / "independent-audit.json", {"case_id": case["case_id"], "evidence": evidence, "traffic": traffic, "source_before_after_equal": True, "real_model_calls": 0, "paid_calls": 0})
    for name in selected["configs"]:
        before = paths.inventory(selected["budget"])
        recorder.command(["run", "api", "status", selected["budget"], selected["run_roots"][name]], expected=(0, 3), label=case["case_id"] + " readonly-status")
        require(paths.inventory(selected["budget"]) == before, "status does not modify accounting authority")
    verify_saved_envelopes(recorder, selected, ledger)
    recorder.cases.append({"case_id": case["case_id"], "status": "passed", "actual_HTTP_requests": len(server.receipts), "independent_totals": ledger.totals(), "source_before_after_equal": True})


TRACE_BASELINE_BOUNDARIES = {"global_authority_created_commit", "api_run_created_commit", "request_body_original_sync", "attempt_reserved_commit", "dispatch_intent_commit", "response_original_sync", "attempt_settled_commit", "retry_schedule_commit", "unit_terminal_commit", "run_terminal_commit"}
ENGINE_BOUNDARIES = TRACE_BASELINE_BOUNDARIES | {"attempt_unsent_release_commit"}


def validate_handshake(row, *, pid, fingerprint=None):
    fields(row, "producer event unit attempt run_id pid compiler_fingerprint thread_id sequence authority_id root will_wait", "actual request driver handshake")
    require(row["producer"] == "request_run_checkpoint_test_handshake" and row["event"] in ENGINE_BOUNDARIES and type(row["pid"]) is int and row["pid"] == pid, "actual driver producer/event/own PID")
    require(type(row["will_wait"]) is bool and isinstance(row["thread_id"], str) and re.fullmatch(r"ThreadId\([0-9]+\)", row["thread_id"]), "actual Rust thread identity; not OS TID")
    natural(row["sequence"], "actual checkpoint sequence", 4096)
    natural(row["attempt"], "actual callback attempt", 4)
    digest(row["compiler_fingerprint"], "executed driver compiler fingerprint")
    if fingerprint is not None: require(row["compiler_fingerprint"] == fingerprint, "CLI/driver same actual compiler fingerprint")
    require(isinstance(row["root"], str) and Path(row["root"]).is_absolute(), "actual callback root")
    if row["event"] != "global_authority_created_commit": uuid(row["run_id"], "actual callback Run")
    else: require(row["run_id"] == "" and row["unit"] == "" and row["attempt"] == 0, "authority init has no invented Run/attempt")
    if row["unit"]:
        require(re.fullmatch(r"[A-Za-z0-9_-]{1,32}", row["unit"]), "callback selected unit identity")
    return row


class RequestDriver:
    def __init__(self, recorder, selected, driver, *, mode="start", event="trace", unit="", attempt=0, run_name=None):
        self.recorder, self.selected = recorder, selected
        self.run_name = run_name or next(iter(selected["configs"]))
        self.event, self.unit, self.attempt = event, unit, attempt
        self.run_root = selected["run_roots"][self.run_name]
        config = selected["authority_file"] if mode == "init" else selected["configs"][self.run_name]
        self.child = recorder.launch(["--exact", "request_run::tests::checkpoint_driver", "--nocapture", "--test-threads=1"], executable=driver, label="request driver " + mode + " " + event, extra={"NOETOME_REQUEST_TEST_MODE": mode, "NOETOME_REQUEST_TEST_CONFIG": str(config), "NOETOME_REQUEST_TEST_BUDGET_ROOT": str(selected["budget"]), "NOETOME_REQUEST_TEST_RUN_ROOT": str(self.run_root), "NOETOME_REQUEST_TEST_EVENT": event, "NOETOME_REQUEST_TEST_UNIT": unit, "NOETOME_REQUEST_TEST_ATTEMPT": str(attempt)})
        self.folder = recorder.children[self.child.pid][3]
        self.position = 0
        self.pending = b""
        self.handshakes = []
        self.unconsumed = []
        self.results = []
        self.held = None
        self.fingerprint = None
        if selected["budget"].exists():
            self.fingerprint = strict_json(paths.read_observed_file(selected["budget"] / "control/owner.json"))["compiler_fingerprint"]

    def poll(self):
        file = self.folder / "stdout.bin"
        with file.open("rb") as stream:
            stream.seek(self.position); new = stream.read(); self.position += len(new)
        self.pending += new
        while b"\n" in self.pending:
            line, self.pending = self.pending.split(b"\n", 1)
            if line.endswith(b"\r"): line = line[:-1]
            beginning = line.find(b"{")
            if beginning < 0: continue  # Actual libtest headings remain in raw stdout.
            row = strict_json(line[beginning:])
            if row.get("producer") == "request_run_checkpoint_test_handshake":
                validate_handshake(row, pid=self.child.pid, fingerprint=self.fingerprint)
                if self.recorder.build_fingerprint is not None:
                    require(row["compiler_fingerprint"] == self.recorder.build_fingerprint, "actual driver fingerprint equals explicitly bound build/source receipt")
                if self.fingerprint is None: self.fingerprint = row["compiler_fingerprint"]
                self.handshakes.append(row); self.unconsumed.append(row)
            elif row.get("producer") == "request_run_test_driver_result":
                require(row["pid"] == self.child.pid and row["compiler_fingerprint"] == self.fingerprint, "actual final driver PID/fingerprint")
                self.results.append(row)
            else:
                raise AuditError("unknown JSON producer in actual driver stream")
        save_json(self.folder / "driver-stream.json", {"handshakes": self.handshakes, "results": self.results, "thread_ids_are_Rust_Debug_not_OS_TIDs": True})

    def next(self, timeout=15):
        require(self.held is None, "must release prior exact handshake first")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.poll()
            while self.unconsumed:
                row = self.unconsumed.pop(0)
                if row["will_wait"]:
                    require(self.child.poll() is None, "handshake producer PID no longer alive")
                    require(self.event == "trace" or row["event"] == self.event and (not self.unit or row["unit"] == self.unit) and (not self.attempt or row["attempt"] == self.attempt), "actual selected handshake selector")
                    self.held = row
                    return row
            if self.child.poll() is not None:
                self.poll()
                raise AuditError("driver exited before requested physical boundary; raw output preserved")
            threading.Event().wait(0.005)
        raise AuditError("actual driver handshake timeout")

    def release(self):
        require(self.held is not None and self.child.poll() is None and self.child.stdin is not None, "only actual held alive own driver may release")
        released = {"handshake": self.held, "release_utc_ns": time.time_ns(), "release_bytes": "release LF"}
        self.child.stdin.write(b"release\n"); self.child.stdin.flush()
        save_json(self.folder / ("release-" + str(len(self.handshakes)) + ".json"), released)
        self.held = None

    def kill(self):
        require(self.child.poll() is None, "only active own driver PID kill")
        self.poll()
        result = self.recorder.force_kill(self.child)
        self.poll()
        save_json(self.folder / "killed-at.json", {"actual_handshake": self.held, "last_actual_handshakes": self.handshakes, "actual_wait_receipt": result[2], "old_worker_exited": self.child.poll() is not None})
        return result

    def finish(self):
        require(self.held is None, "cannot finish blocked driver without release")
        result = self.recorder.wait(self.child, expected=0)
        self.poll()
        require(len(self.results) == 1 and "result" in self.results[0], "one actual completed driver result; semantic errors are not a test exit pass")
        return result


def phase_evidence(driver, row):
    ledger, evidence = read_authority(driver.selected["budget"], fixture=driver.recorder.fixture, authority_config=driver.selected["authority_file"], selected_run_roots=driver.selected["run_roots"].values(), expected_fingerprint=row["compiler_fingerprint"])
    require(ledger.sequence >= row["sequence"] and ledger.authority["authority_id"] == row["authority_id"], "actual handshake names committed authority prefix; concurrent appends may follow")
    if row["event"] == "global_authority_created_commit":
        require(not ledger.runs, "authority boundary has no invented Run")
    else:
        require(row["run_id"] in ledger.runs and ledger.runs[row["run_id"]]["spec"]["run_root"] == row["root"], "physical boundary original Run")
        run = ledger.runs[row["run_id"]]
        if row["unit"]:
            require(row["unit"] in run["units"], "physical boundary original unit")
            if row["attempt"]:
                matches = [a for key, a in ledger.attempts.items() if key[:3] == (row["run_id"], row["unit"], row["attempt"])]
                require(len(matches) == 1, "callback attempt has actual committed original")
                a = matches[0]
                expected = {"request_body_original_sync": "prepared", "attempt_reserved_commit": "reserved", "dispatch_intent_commit": "may_send", "attempt_unsent_release_commit": "not_sent"}
                if row["event"] in expected: require(a["state"] == expected[row["event"]], "held callback actual durable state")
                if row["event"] == "response_original_sync": require(a["transport_sealed"] and a["receipt"] is None, "held actual response/framing seal before final receipt")
                if row["event"] == "attempt_settled_commit": require(a["receipt"] is not None, "held confirmed actual receipt")
                if row["event"] == "retry_schedule_commit": require(a["retry"] is not None, "held durable retry schedule")
            if row["event"] == "unit_terminal_commit": require(run["units"][row["unit"]]["outcome"] is not None, "held confirmed unit terminal")
        if row["event"] == "run_terminal_commit": require(run["outcome"] is not None, "held confirmed Run terminal")
    return ledger, evidence


def single_unit_component(author_case, name):
    case = copy.deepcopy(author_case)
    case["case_id"] = name
    case["units"] = {"unit-a": case["units"]["unit-a"]}
    case["execution_layout"] = {"kind": "one-run-one-representative-unit", "unit_to_run": {"unit-a": "run-main"}, "shared_global": "budget-main"}
    # This is the frozen generic boundary_expected scope, separately reported;
    # it never reuses the dual-unit literal case totals or claims concurrent kill.
    return case


def write_api_configs(recorder, selected, server, case):
    for unit in case["units"]:
        config = strict_json(paths.read_observed_file(recorder.fixture / "policies/single-request-api-template.json"))
        config["endpoint"] = server.endpoint
        save_json(selected["root"] / (unit + "-api.json"), config)


def trace_request_boundaries(recorder, driver, author_case, label="generic-boundary-trace"):
    # Actual full trace is captured first; target lists are never invented.
    component = single_unit_component(author_case, label)
    selected = prepare_case(recorder, component)
    server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
    boundaries = []
    with server:
        write_api_configs(recorder, selected, server, component)
        initial = RequestDriver(recorder, selected, driver, mode="init", event="trace")
        row = initial.next(); _, evidence = phase_evidence(initial, row)
        boundaries.append(dict(row, source_case_id=author_case["case_id"])); save_json(selected["root"] / "trace-init-observation.json", evidence)
        initial.release(); initial.finish()
        running = RequestDriver(recorder, selected, driver, event="trace")
        while True:
            running.poll()
            if running.child.poll() is not None: break
            if running.results and not running.unconsumed: break
            try:
                row = running.next(timeout=10)
            except AuditError:
                if running.child.poll() is not None:
                    running.poll(); break
                raise
            _, evidence = phase_evidence(running, row)
            boundaries.append(dict(row, source_case_id=author_case["case_id"]))
            save_json(selected["root"] / ("trace-observation-" + str(len(boundaries)) + ".json"), evidence)
            running.release()
        running.finish()
        require(not server.errors and len(server.incoming) == len(server.receipts), "actual trace HTTP complete original receipts")
    ledger, evidence = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
    traffic = verify_server_receipts(ledger, server.receipts, selected["server_root"])
    expected_unknown = sum(step in ("http-429", "http-503") for step in component["units"]["unit-a"])
    require(ledger.totals()["accounted_tokens"] == 700 and ledger.totals()["held_tokens"] == expected_unknown * 8310 and all(r["outcome"] == "succeeded" for r in ledger.runs.values()), "actual representative trace completes independently frozen candidate/usage")
    verify_saved_envelopes(recorder, selected, ledger)
    save_json(recorder.root / (label + "-boundaries.json"), {"boundaries": boundaries, "actual_boundary_count": len(boundaries), "scope": "one representative author unit; actual callback trace only; not concurrent interruption coverage", "traffic": traffic, "final": evidence})
    recorder.cases.append({"case_id": label, "status": "passed", "actual_HTTP_requests": len(server.receipts), "actual_callback_count": len(boundaries), "scope": "actual representative callback trace; no OS kill in this trace"})
    return boundaries


def require_event_prefix(before, after):
    require(len(after.rows) >= len(before.rows) and after.rows[:len(before.rows)] == before.rows, "durable original event prefix never rewritten after resume")
    for key, original in before.attempts.items():
        require(key in after.attempts and after.attempts[key]["plan"] == original["plan"] and after.attempts[key]["identity"] == original["identity"], "same original Run/attempt/UUID/plan identity after recovery")
        for name, stamp in original["files"].items(): require(after.attempts[key]["files"].get(name) == stamp, "same confirmed original artifact stamp after recovery")


def generic_interruption_matrix(recorder, driver, boundaries, author_cases):
    for index, target in enumerate(boundaries):
        source_case = next(c for c in author_cases if c["case_id"] == target["source_case_id"])
        component = single_unit_component(source_case, f"generic-kill-{index + 1:03}-{target['event']}")
        selected = prepare_case(recorder, component)
        server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
        with server:
            write_api_configs(recorder, selected, server, component)
            mode = "init" if target["event"] == "global_authority_created_commit" else "start"
            if mode != "init": recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
            held = RequestDriver(recorder, selected, driver, mode=mode, event=target["event"], unit=target["unit"], attempt=target["attempt"])
            row = held.next(); phase_evidence(held, row)
            held.kill()  # actual OS kill + own PID wait, not a throwing hook
            before, before_evidence = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
            received_before = len(server.receipts)
            operation = "start" if mode == "init" else "resume"
            recorder.command(["run", "api", operation, selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=(0, 3), label="ordinary same-original recovery " + target["event"])
            after, after_evidence = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
            require_event_prefix(before, after)
            traffic = verify_server_receipts(after, server.receipts, selected["server_root"])
            expected_known = 700
            unknown_before = [a for a in before.attempts.values() if a["state"] in ("may_send", "unknown") and (not a["transport_sealed"] or a["receipt"] is not None and a["receipt"]["usage"] is None)]
            expected_held = len(unknown_before) * 8310
            require(after.totals()["accounted_tokens"] == expected_known and after.totals()["held_tokens"] == expected_held, "independent frozen R/700 phase recovery conservation")
            require(after.runs and all(r["outcome"] == "succeeded" for r in after.runs.values()), "representative original author candidate completes after recovery")
            if target["event"] in ("request_body_original_sync", "attempt_reserved_commit"):
                require(len(after.attempts) == 1 and len(server.receipts) == 1, "SamePrepared keeps original UUID/attempt, exactly one actual HTTP")
            if target["event"] in ("response_original_sync", "attempt_settled_commit", "unit_terminal_commit", "run_terminal_commit"):
                require(len(server.receipts) == received_before, "safe original response or confirmed result recovery sends no extra HTTP")
            if target["event"] == "retry_schedule_commit":
                schedules = [a["retry"] for a in before.attempts.values() if a["retry"]]
                require(all(a["retry"] in schedules for key, a in after.attempts.items() if key in before.attempts and a["retry"]), "original retry_at not advanced/reset")
            final_before = {"budget": paths.inventory(selected["budget"]), "run": paths.inventory(selected["run_roots"]["run-main"])}
            recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=0, label="duplicate confirmed ordinary resume")
            require(final_before == {"budget": paths.inventory(selected["budget"]), "run": paths.inventory(selected["run_roots"]["run-main"])}, "confirmed repeated resume preserves complete bytes/mtime/IDs with no event or settlement duplicate")
            require(not server.errors and len(server.incoming) == len(server.receipts), "actual interruption raw HTTP evidence")
        after_sources = {str(p): paths.inventory(p) for p in selected["projects"].values()}
        require(after_sources == selected["source_before"], "interrupted source originals preserved")
        verify_saved_envelopes(recorder, selected, after)
        save_json(selected["root"] / "independent-audit.json", {"actual_target": row, "before_kill_final": before_evidence, "after_ordinary_resume": after_evidence, "traffic": traffic, "expected_from_frozen_R_known700_and_prior_phase": {"known": expected_known, "held": expected_held}, "scope": "generic single-unit boundary_expected; no concurrent kill claim"})
        recorder.cases.append({"case_id": component["case_id"], "status": "passed", "scope": "actual one-unit OS interruption/ordinary recovery", "actual_HTTP_requests": len(server.receipts)})


def server_accepted_interruption(recorder, driver, author_cases):
    original = next(c for c in author_cases if c["case_id"] == "unknown-remote-completion-retained")
    component = single_unit_component(original, "generic-server-accepted-kill")
    selected = prepare_case(recorder, component)
    server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
    with server:
        write_api_configs(recorder, selected, server, component)
        recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
        active = RequestDriver(recorder, selected, driver, event="observe")
        require(server.accepted.wait(10), "actual complete server request must be received before OS kill")
        require(len(server.receipts) == 1 and server.receipts[0]["unit_id"] == "unit-a", "one actual server accepted request")
        before, evidence_before = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
        require(len(before.attempts) == 1 and next(iter(before.attempts.values()))["state"] == "may_send" and next(iter(before.attempts.values()))["receipt"] is None and not next(iter(before.attempts.values()))["transport_sealed"], "actual accepted request must lack trustworthy final/framing receipt before kill")
        active.kill()
        recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=0, label="explicit interrupted request ordinary resume")
        after, evidence_after = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
        require_event_prefix(before, after)
        require(after.totals()["accounted_tokens"] == 700 and after.totals()["held_tokens"] == 8310 and len(server.receipts) == 2 and len(after.attempts) == 2, "old accepted MaySend remains Unknown R; exactly one fresh bounded request")
        old_key = next(iter(before.attempts))
        require(after.attempts[old_key]["state"] == "unknown" and after.attempts[old_key]["unknown_reason"] == "REQUEST_INTERRUPTED", "explicit process interruption classified separately from HTTP wire failure")
        require(not server.errors and len(server.incoming) == len(server.receipts), "all actual accepted synthetic HTTP receipts preserved")
    require({str(p): paths.inventory(p) for p in selected["projects"].values()} == selected["source_before"], "source preserved through accepted request interruption")
    traffic = verify_server_receipts(after, server.receipts, selected["server_root"])
    verify_saved_envelopes(recorder, selected, after)
    save_json(selected["root"] / "independent-audit.json", {"scope": "single representative original author unit; actual server receive/own PID kill/wait/explicit resume", "before": evidence_before, "after": evidence_after, "traffic": traffic})
    recorder.cases.append({"case_id": component["case_id"], "status": "passed", "actual_HTTP_requests": 2, "old_unknown_R_retained": 8310})


def reserved_expired_recovery(recorder, driver, author_cases, *, kill_after_unsent=False):
    original = next(c for c in author_cases if c["case_id"] == "baseline-two-independent")
    component = single_unit_component(original, "reserved-expired-release-COMMIT-kill" if kill_after_unsent else "reserved-expired-before-MaySend")
    selected = prepare_case(recorder, component)
    server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
    with server:
        write_api_configs(recorder, selected, server, component)
        recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
        active = RequestDriver(recorder, selected, driver, event="attempt_reserved_commit", unit="unit-a", attempt=1)
        row = active.next()
        before, evidence_before = phase_evidence(active, row)
        require(before.totals()["held_tokens"] == 8310 and before.totals()["held_requests"] == 1 and not server.incoming, "actual Reserved before any MaySend/HTTP")
        active.kill()
        expiry = min(utc_ns(r["spec"]["deadline_utc"]) for r in before.runs.values())
        expiry = min(expiry, min(utc_ns(u["spec"]["deadline_utc"]) for r in before.runs.values() for u in r["units"].values()))
        wait_started = time.time_ns()
        while time.time_ns() <= expiry + 50_000_000:
            threading.Event().wait(min(0.05, max(0.001, (expiry + 50_000_000 - time.time_ns()) / 1_000_000_000)))
        save_json(selected["root"] / "actual-expiry-wait.json", {"expiry_utc_ns": expiry, "wait_started_utc_ns": wait_started, "wait_finished_utc_ns": time.time_ns(), "original_unit_limit_ms": 5000})
        if kill_after_unsent:
            recovering = RequestDriver(recorder, selected, driver, mode="resume", event="attempt_unsent_release_commit", unit="unit-a", attempt=1)
            released_row = recovering.next(); released, release_evidence = phase_evidence(recovering, released_row)
            require_event_prefix(before, released)
            require(next(iter(released.attempts.values()))["state"] == "not_sent" and all(v == 0 for v in released.totals().values()), "actual never-MaySend release COMMIT has zero known/held/request balances")
            save_json(selected["root"] / "actual-unsent-release-boundary.json", {"actual_handshake": released_row, "actual_released_state": release_evidence})
            recovering.kill()
        recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=(0, 3), label="same Run expired Reserved ordinary recovery")
        after, evidence_after = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values(), expected_fingerprint=row["compiler_fingerprint"])
        require_event_prefix(before, after)
        traffic = verify_server_receipts(after, server.receipts, selected["server_root"])
        oracle = {"HTTP_requests": 0, "known_tokens": 0, "held_tokens": 0, "dispatched_requests": 0, "held_requests": 0, "logical_attempts": 1, "state": "not_sent"}
        save_json(selected["root"] / "independent-audit.json", {"before": evidence_before, "after": evidence_after, "traffic": traffic, "expected_before_feature_from_frozen_NotSent_protocol": oracle, "scope": "single representative original unit; actual Reserved commit ownPID kill/wait, absolute deadline expires before explicit ordinary resume"})
        require(not server.errors and not server.incoming and not server.receipts, "expired Reserved must never make actual HTTP")
        require(after.totals() == {**after.totals(), "accounted_tokens": 0, "held_tokens": 0, "dispatched_requests": 0, "held_requests": 0} and len(after.attempts) == 1 and next(iter(after.attempts.values()))["state"] == "not_sent", "expired never-MaySend Reserved must release original R8310 and held request slot as NotSent")
    if kill_after_unsent:
        stable = protected_inventory(selected)
        recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=(0, 3), label="confirmed NotSent duplicate ordinary resume")
        require(protected_inventory(selected) == stable, "NotSent repeated resume never reuses attempt, appends release or changes any original")
    require({str(p): paths.inventory(p) for p in selected["projects"].values()} == selected["source_before"], "source originals preserved through expired Reserved recovery")
    verify_saved_envelopes(recorder, selected, after)
    recorder.cases.append({"case_id": component["case_id"], "status": "passed", "actual_HTTP_requests": 0, "actual_OS_kill_and_wait": True})


def settled_success_attempts(ledger, unit):
    """Known author usage is a safe receipt, independently of unit terminal state."""
    return {key: attempt for key, attempt in ledger.attempts.items()
            if key[1] == unit and attempt["state"] == "known" and attempt["transport_sealed"]
            and attempt["receipt"]["usage"] == {"input_tokens": 100, "output_tokens": 600, "total_tokens": 700}
            and attempt["receipt"]["candidate_status"] == "validated" and attempt["receipt"]["error_code"] is None
            and attempt["receipt"]["network_attempts"] == 1 and attempt["receipt"]["http_status"] == 200
            and attempt["receipt"]["response_complete"] is True}


def coordination_chain(budget, authority_file):
    owner = strict_json(paths.read_observed_file(budget / "control/owner.json"))
    bind_original_authority(owner["authority"], authority_file)
    with closing(sqlite3.connect(paths.sqlite_file_uri(budget / "control/budget.sqlite"), uri=True)) as db:
        ledger = reconstruct_events(owner["authority"], list(db.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM events ORDER BY sequence")))
    return ledger


def unit_has_safe_completion(budget, unit, authority_file):
    return bool(settled_success_attempts(coordination_chain(budget, authority_file), unit))


def dual_unit_kill_case(recorder, case, driver, *, accepted=False):
    selected = prepare_case(recorder, case)
    def gate(unit, key, step):
        if unit != "unit-a" or key[2] != 1: return
        end = time.monotonic() + 0.125
        while not unit_has_safe_completion(selected["budget"], "unit-b", selected["authority_file"]):
            require(time.monotonic() < end, "independent B lacks a trusted settled success within first A response timing window")
            threading.Event().wait(0.005)
    server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], case["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]), before_response=None if accepted else gate)
    with server:
        write_api_configs(recorder, selected, server, case)
        recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
        active = RequestDriver(recorder, selected, driver, event="observe" if accepted else "retry_schedule_commit", unit="unit-a", attempt=1)
        if accepted:
            end = time.monotonic() + 10
            while True:
                with server.lock:
                    accepted_a = [r for r in server.receipts if r["unit_id"] == "unit-a" and r["attempt_no"] == 1]
                    received_b = copy.deepcopy([r for r in server.receipts if r["unit_id"] == "unit-b"])
                if accepted_a:
                    fast = coordination_chain(selected["budget"], selected["authority_file"])
                    a_fast = next(a for key, a in fast.attempts.items() if key[1:3] == ("unit-a", 1))
                    require(a_fast["state"] == "may_send" and a_fast["receipt"] is None and not a_fast["transport_sealed"], "accepted OS interruption window ended before actual kill")
                    b_fast = settled_success_attempts(fast, "unit-b")
                    if len(b_fast) == 1 and len(received_b) == 1:
                        b_key, b_attempt = next(iter(b_fast.items()))
                        require(received_b[0]["request_id"] == b_key[3] and received_b[0].get("advertised_status") == 200
                                and "response_flush_utc_ns" in received_b[0]
                                and received_b[0]["response_sha256"] == b_attempt["receipt"]["response"]["sha256"]
                                and received_b[0]["request_body_sha256"] == b_attempt["plan"]["request_body"]["sha256"],
                                "live settled B key/body/full response equals actual server receipt")
                        observed_before_kill = time.time_ns()
                        break
                require(time.monotonic() < end and not server.errors and active.child.poll() is None, "real A accept/B trusted receipt prerequisite not observed before interruption")
                threading.Event().wait(0.005)
            active.kill()
            before, evidence_before = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
            require_event_prefix(fast, before)
            require(all(before.attempts[key] == attempt for key, attempt in b_fast.items()), "stable postkill B full original audit equals live known key/receipt/file witnesses")
            a = next(a for key, a in before.attempts.items() if key[1:3] == ("unit-a", 1))
            require(a["state"] == "may_send" and a["receipt"] is None and not a["transport_sealed"], "actual killed A still lacks sealed/final receipt; timer completion cannot count as interruption")
            b_terminal_before = any(run["units"]["unit-b"]["outcome"] == "succeeded" for run in fast.runs.values())
            save_json(selected["root"] / "actual-prekill-chain.json", {"observed_before_kill_utc_ns": observed_before_kill, "live_validated_chain": fast.rows, "B_actual_server_receipt": received_b, "stable_postkill_full_original_audit": evidence_before, "scope": "live chain/server phase only before immediate kill; every request/response/report/FileStamp audited after actual old PID death and before ordinary resume"})
        else:
            row = active.next()
            require(row["event"] == "retry_schedule_commit" and row["unit"] == "unit-a" and row["attempt"] == 1, "actual selected first retry schedule")
            phase_evidence(active, row)
            before, evidence_before = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
            b_terminal_before = any(run["units"]["unit-b"]["outcome"] == "succeeded" for run in before.runs.values())
            active.kill()
        b_before = settled_success_attempts(before, "unit-b")
        require(len(b_before) == 1, "B Known700 validated complete200 transport-sealed originals fully audited before resume")
        if case["case_id"].startswith("elapsed-limit-persists-through-kill"):
            started = time.monotonic_ns()
            threading.Event().wait(1.7)
            save_json(selected["root"] / "actual-deadline-wait.json", {"before_monotonic_ns": started, "after_monotonic_ns": time.monotonic_ns(), "minimum_author_wait_ms": 1700})
        recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=(0, 3), label="ordinary dual-unit same Run interrupted recovery")
    ledger, evidence_after = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
    require_event_prefix(before, ledger)
    require({key for key in ledger.attempts if key[1] == "unit-b"} == set(b_before)
            and all(ledger.attempts[key] == attempt for key, attempt in b_before.items()),
            "independent B same known key/body/raw/file witnesses unchanged; resume only completes its unit")
    require(len([receipt for receipt in server.receipts if receipt["unit_id"] == "unit-b"]) == 1,
            "independent B trustworthy settled success recovery sends no additional HTTP")
    traffic = verify_server_receipts(ledger, server.receipts, selected["server_root"])
    case_expected_check(case, ledger, traffic)
    if case["case_id"].startswith("retry-backoff-persists-through-kill"):
        a_receipts = sorted((r for r in server.receipts if r["unit_id"] == "unit-a"), key=lambda r: r["attempt_no"])
        require(len(a_receipts) == 2 and a_receipts[1]["request_received_monotonic_ns"] - a_receipts[0]["response_before_write_monotonic_ns"] >= 1_000_000_000, "actual server retry delay lower bound1000ms")
    require(not server.errors and len(server.incoming) == len(server.receipts), "all dual-unit HTTP originals valid; no silently rejected extra request")
    sources_after = {str(p): paths.inventory(p) for p in selected["projects"].values()}
    require(sources_after == selected["source_before"], "dual-unit OS interruption preserves complete source originals")
    save_json(selected["root"] / "source-after.json", sources_after)
    verify_saved_envelopes(recorder, selected, ledger)
    save_json(selected["root"] / "independent-audit.json", {"before": evidence_before, "after": evidence_after, "traffic": traffic, "B_unit_terminal_before_kill": b_terminal_before, "B_known700_validated_receipt_in_live_chain_before_kill": True, "B_all_originals_audited_before_resume": True, "B_original_attempts_unchanged_no_additional_HTTP": True, "scope": "original two-unit author layout; actual ownPID kill/wait follows trusted settled B success; accepted variant audits stable originals after death and preserves unknown500ms/API200ms without extra response gate"})
    recorder.cases.append({"case_id": case["case_id"], "status": "passed", "actual_HTTP_requests": len(server.receipts), "independent_totals": ledger.totals(), "actual_OS_kill_and_wait": True})


def parameter_case(original, variant):
    case = copy.deepcopy(original)
    case["case_id"] = original["case_id"] + "--" + variant["name"]
    if variant["name"] == "explicit-synthetic-opening":
        case["units"] = {"unit-b": ["subsequent-budget-denied"]}
        case["execution_layout"] = {"kind": "one-run-one-unit", "unit_to_run": {"unit-b": "run-main"}, "shared_global": "budget-main"}
        case["authority_opening_template"] = variant["authority_template"]
        case["expected"].update(variant["expected"])
        case["expected"].update(candidate_validated_units=[], candidate_validated_unit_subset=[], isolated_failed_units=[], unknown_commitment_attempts=0)
    else:
        step = {"timeout": "timeout", "disconnect": "disconnect", "truncated": "truncated", "OS-kill-after-accepted": "unknown-after-accept"}[variant["name"]]
        case["units"]["unit-a"][0] = step
    return case


def protected_inventory(selected):
    return {"budget": paths.inventory(selected["budget"]), "runs": {n: paths.inventory(p) for n, p in selected["run_roots"].items()}, "sources": {str(p): paths.inventory(p) for p in selected["projects"].values()}, "configs": {str(p): observed_stamp(p)[0] for p in [selected["authority_file"], *selected["configs"].values(), *(selected["root"] / (u + "-api.json") for u in selected["projects"])]}}


def replace_same_bytes(path):
    before = observed_stamp(path)[0]
    replacement = path.with_name(path.name + ".checker-replacement")
    require(not replacement.exists(), "only create new owned replacement")
    replacement.write_bytes(paths.read_observed_file(path))
    replacement.replace(path)
    require(observed_stamp(path)[0]["identity"] != before["identity"], "real replacement must change actual filesystem identity")


def paused_protection_matrix(recorder, author_cases):
    original = next(c for c in author_cases if c["case_id"] == "baseline-two-independent")
    attacks = ["global-owner-new-identity", "global-database-new-identity", "owned-policy-new-identity", "owned-policy-mtime", "global-caps-mirror", "opening-mirror", "Run-owner-new-identity", "unknown-owner-field", "unknown-SQL-table", "unknown-SQL-sidecar", "compiler-mirror", "wrong-config-path", "API-mtime", "API-new-identity", "source-original-mutation", "Run-config-mtime"]
    for attack in attacks:
        component = single_unit_component(original, "protection-" + attack)
        if attack == "opening-mirror": component["authority_opening_template"] = "policies/authority-opening-template.json"
        selected = prepare_case(recorder, component)
        server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
        with server:
            write_api_configs(recorder, selected, server, component)
            recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
            recorder.command(["run", "api", "start", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"], "--paused"], expected=0)
            ledger_before, evidence_before = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
            require(not ledger_before.attempts and not server.incoming, "paused ordinary start has no synthetic HTTP or invented attempts")
            owner_path = selected["budget"] / "control/owner.json"
            database = selected["budget"] / "control/budget.sqlite"
            run_owner = selected["run_roots"]["run-main"] / "control/owner.json"
            resume_config = selected["configs"]["run-main"]
            if attack in ("global-owner-new-identity", "global-database-new-identity", "owned-policy-new-identity", "Run-owner-new-identity", "API-new-identity"):
                target = {"global-owner-new-identity": owner_path, "global-database-new-identity": database, "owned-policy-new-identity": selected["budget"] / "control/policy.json", "Run-owner-new-identity": run_owner, "API-new-identity": selected["root"] / "unit-a-api.json"}[attack]
                replace_same_bytes(target)
            elif attack in ("owned-policy-mtime", "API-mtime", "Run-config-mtime"):
                target = {"owned-policy-mtime": selected["budget"] / "control/policy.json", "API-mtime": selected["root"] / "unit-a-api.json", "Run-config-mtime": resume_config}[attack]
                state = target.stat(); os.utime(target, ns=(state.st_atime_ns, state.st_mtime_ns + 1_000_000_000))
                require(target.stat().st_mtime_ns != state.st_mtime_ns, "actual selected original mtime changed")
            elif attack in ("global-caps-mirror", "opening-mirror", "unknown-owner-field", "compiler-mirror"):
                owner = strict_json(paths.read_observed_file(owner_path))
                if attack == "global-caps-mirror": owner["authority"]["global_limits"]["max_total_tokens"] += 8310
                elif attack == "opening-mirror": owner["authority"]["opening"] = {k: 0 for k in owner["authority"]["opening"]}
                elif attack == "compiler-mirror": owner["compiler_fingerprint"] = "f" * 64 if owner["compiler_fingerprint"] != "f" * 64 else "e" * 64
                else: owner["unknown_field"] = True
                raw = canonical(owner); owner_path.write_bytes(raw)
                with closing(sqlite3.connect(database)) as db, db:
                    db.execute("UPDATE metadata SET metadata_json=? WHERE singleton=1", (raw.decode(),))
            elif attack == "unknown-SQL-table":
                with closing(sqlite3.connect(database)) as db, db: db.execute("CREATE TABLE unknown_table(value INTEGER)")
            elif attack == "unknown-SQL-sidecar":
                (database.parent / "budget.sqlite-journal").write_bytes(b"explicit synthetic unknown journal; preserve")
            elif attack == "wrong-config-path":
                resume_config = selected["root"] / "same-bytes-other-config.json"
                resume_config.write_bytes(paths.read_observed_file(selected["configs"]["run-main"]))
            elif attack == "source-original-mutation":
                cache_files = [p for p in selected["projects"]["unit-a"].rglob("*") if p.is_file() and paths.read_observed_file(p) == paths.read_observed_file(recorder.fixture / "source/source.txt")]
                require(cache_files, "actual registered synthetic original source cache found")
                cache_files[0].write_bytes(b"synthetic deliberately changed source; never author gold\n")
            else: raise AuditError("unknown independent protected mutation")
            before_command = protected_inventory(selected)
            if attack == "wrong-config-path":
                _, _, refusal = recorder.command(["run", "api", "resume", resume_config, selected["budget"], selected["run_roots"]["run-main"]], label="wrong selected config path must preserve original Run")
                require(refusal["exit_code"] != 0, "different original config path must refuse")
                require(protected_inventory(selected) == before_command, "mis-selected config refuses without invalidating original Run")
            else:
                _, _, status = recorder.command(["run", "api", "status", selected["budget"], selected["run_roots"]["run-main"]], label="readonly actual protection " + attack)
                require(status["exit_code"] != 0 and protected_inventory(selected) == before_command, "readonly corrupted/changed source status refuses and preserves original inventory")
                _, _, refusal = recorder.command(["run", "api", "resume", resume_config, selected["budget"], selected["run_roots"]["run-main"]], label="explicit ordinary protected recovery " + attack)
                require(refusal["exit_code"] != 0, "corrupted original explicit resume must refuse")
                after_command = protected_inventory(selected)
                if attack in ("API-mtime", "API-new-identity", "source-original-mutation", "Run-config-mtime") and after_command != before_command:
                    after, evidence_after = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
                    require_event_prefix(ledger_before, after)
                    require(all(r["outcome"] == "invalidated" for r in after.runs.values()) and after.totals() == ledger_before.totals() and not after.attempts, "changed original can only retain typed invalidation; no new request or budget reset")
                    require(after_command["sources"] == before_command["sources"] and after_command["configs"] == before_command["configs"] and after_command["runs"] == before_command["runs"], "invalidating selected original never overwrites source/config/Run originals")
                else:
                    require(after_command == before_command, "bad owner/policy/unknown original recovery refuses without writing or replacing originals")
            require(not server.incoming and not server.receipts and not server.errors, "protection attempts make exactly zero actual HTTP")
            save_json(selected["root"] / "independent-audit.json", {"attack": attack, "original_ledger": evidence_before, "after_deliberate_mutation_before_commands": before_command, "after_commands": protected_inventory(selected), "actual_HTTP_requests": len(server.incoming), "scope": "frozen guards independently exercised through ordinary CLI; mutations only in newly owned synthetic roots"})
        recorder.cases.append({"case_id": component["case_id"], "status": "passed", "actual_HTTP_requests": 0})


def driver_matrix(recorder, driver, author):
    baseline = next(c for c in author["cases"] if c["case_id"] == "baseline-two-independent")
    retry = next(c for c in author["cases"] if c["case_id"] == "retry-backoff-persists-through-kill")
    boundaries = trace_request_boundaries(recorder, driver, baseline)
    retry_trace = trace_request_boundaries(recorder, driver, retry, label="generic-retry-trace")
    boundaries += [row for row in retry_trace if row["event"] == "retry_schedule_commit"]
    require({row["event"] for row in boundaries} == TRACE_BASELINE_BOUNDARIES, "actual original callback traces cover declared baseline and retry engine boundaries")
    save_json(recorder.root / "selected-interruption-boundaries.json", {"actual_targets": boundaries, "scope": "single-unit generic boundary_expected; no natural/concurrent kill claim"})
    generic_interruption_matrix(recorder, driver, boundaries, author["cases"])
    server_accepted_interruption(recorder, driver, author["cases"])
    reserved_expired_recovery(recorder, driver, author["cases"])
    reserved_expired_recovery(recorder, driver, author["cases"], kill_after_unsent=True)


def active_owner_protection(recorder, driver, author_cases):
    baseline = next(c for c in author_cases if c["case_id"] == "baseline-two-independent")
    component = single_unit_component(baseline, "protection-actual-live-worker")
    selected = prepare_case(recorder, component)
    server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
    with server:
        write_api_configs(recorder, selected, server, component)
        recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
        active = RequestDriver(recorder, selected, driver, event="request_body_original_sync", unit="unit-a", attempt=1)
        row = active.next(); before, _ = phase_evidence(active, row)
        inventory = protected_inventory(selected)
        recorder.command(["run", "api", "status", selected["budget"], selected["run_roots"]["run-main"]], expected=0, label="actual readonly status while own worker is held")
        _, _, refusal = recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], label="second actual process cannot take live worker ownership")
        require(refusal["exit_code"] != 0 and active.child.poll() is None and protected_inventory(selected) == inventory and not server.incoming, "actual live worker excludes second resume and preserves all originals")
        active.kill()
        recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=0)
    after, evidence = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
    require_event_prefix(before, after)
    traffic = verify_server_receipts(after, server.receipts, selected["server_root"])
    require(traffic["observed_HTTP_requests"] == 1 and len(after.attempts) == 1 and after.totals()["accounted_tokens"] == 700 and all(r["outcome"] == "succeeded" for r in after.runs.values()), "after actual worker death same Prepared attempt can send once and settle once")
    verify_saved_envelopes(recorder, selected, after)
    save_json(selected["root"] / "independent-audit.json", {"actual_held_worker": row, "inventory_at_second_process": inventory, "after": evidence, "traffic": traffic})
    recorder.cases.append({"case_id": component["case_id"], "status": "passed", "actual_HTTP_requests": 1})


def artifact_protection_matrix(recorder, driver, author_cases):
    baseline = next(c for c in author_cases if c["case_id"] == "baseline-two-independent")
    for attack in ("missing-response", "response-new-identity", "missing-transport", "unknown-artifact", "request-sameSHA-new-identity", "synchronized-known-usage", "synchronized-candidate-wrapper"):
        component = single_unit_component(baseline, "protection-" + attack)
        selected = prepare_case(recorder, component)
        server = ArtificialHttp(selected["server_root"], recorder.fixture, selected["routes"], component["units"], lambda request_id: lookup_persistent_attempt(selected["budget"], request_id, selected["authority_file"]))
        with server:
            write_api_configs(recorder, selected, server, component)
            recorder.command(["run", "api", "init", selected["authority_file"], selected["budget"]], expected=0)
            if attack.startswith("synchronized-"):
                recorder.command(["run", "api", "start", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], expected=0)
                before, evidence_before = read_authority(selected["budget"], fixture=recorder.fixture, authority_config=selected["authority_file"], selected_run_roots=selected["run_roots"].values())
                require(len(before.attempts) == 1 and before.totals()["accounted_tokens"] == 700, "actual one author response known700 before coordinated forgery")
            else:
                active = RequestDriver(recorder, selected, driver, event="response_original_sync", unit="unit-a", attempt=1)
                row = active.next(); before, evidence_before = phase_evidence(active, row); active.kill()
            a = next(iter(before.attempts.values()))
            attempt_root = selected["run_roots"]["run-main"] / paths.safe_relative(a["plan"]["attempt_root"])
            received_before = len(server.incoming)
            if attack == "missing-response": (attempt_root / "response.bin").unlink()
            elif attack == "response-new-identity": replace_same_bytes(attempt_root / "response.bin")
            elif attack == "missing-transport": (attempt_root / "transport-receipt.json").unlink()
            elif attack == "unknown-artifact": (attempt_root / "unknown-artifact.bin").write_bytes(b"new synthetic unknown; preserve")
            elif attack == "request-sameSHA-new-identity": replace_same_bytes(attempt_root / "request-body.json")
            else:
                report_path = attempt_root / "validation.json"
                report = strict_json(paths.read_observed_file(report_path))
                changed_names = {"validation.json", "validation.md"}
                if attack == "synchronized-known-usage":
                    fake_usage = {"input_tokens": 1, "output_tokens": 0, "total_tokens": 1}
                    report["usage"]["model_usage"] = {**fake_usage, "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0}, "output_tokens_details": {"reasoning_tokens": 0}}
                else:
                    candidate_path = attempt_root / "candidates.json"
                    candidate = strict_json(paths.read_observed_file(candidate_path))
                    candidate["request_id"] = "ffffffff-ffff-4fff-8fff-ffffffffffff"
                    candidate_path.write_bytes(canonical(candidate)); changed_names.add("candidates.json")
                    report["artifacts"]["candidates.json"] = {"sha256": sha(paths.read_observed_file(candidate_path)), "byte_length": candidate_path.stat().st_size}
                report_path.write_bytes(canonical(report))
                (attempt_root / "validation.md").write_bytes(b"```json\n" + report_path.read_bytes() + b"\n```\n")
                database = selected["budget"] / "control/budget.sqlite"
                with closing(sqlite3.connect(database)) as db, db:
                    rows = list(db.execute("SELECT sequence,payload_json FROM events ORDER BY sequence"))
                    rewritten = []; previous = "0" * 64
                    for sequence, raw in rows:
                        record = strict_json(raw); record["previous_sha256"] = previous
                        if record["event"]["kind"] == "receipt_settled":
                            event = record["event"]
                            if attack == "synchronized-known-usage": event["receipt"]["usage"] = fake_usage
                            event["receipt"]["report"] = {"sha256": sha(paths.read_observed_file(report_path)), "byte_length": report_path.stat().st_size}
                            for name in changed_names: event["files"][name] = observed_stamp(attempt_root / name)[0]
                        encoded = canonical(record); previous = sha(encoded)
                        rewritten.append((record["previous_sha256"], encoded.decode(), previous, sequence))
                    db.executemany("UPDATE events SET previous_sha256=?,payload_json=?,payload_sha256=? WHERE sequence=?", rewritten)
            inventory = protected_inventory(selected)
            _, _, status = recorder.command(["run", "api", "status", selected["budget"], selected["run_roots"]["run-main"]], label="actual readonly artifact protection " + attack)
            require(status["exit_code"] != 0 and protected_inventory(selected) == inventory, "readonly artifact corruption/forgery refused and preserved")
            _, _, resume = recorder.command(["run", "api", "resume", selected["configs"]["run-main"], selected["budget"], selected["run_roots"]["run-main"]], label="actual explicit artifact protection " + attack)
            require(resume["exit_code"] != 0 and protected_inventory(selected) == inventory and len(server.incoming) == received_before, "bad sealed artifact never overwritten/refunded/resubmitted")
            save_json(selected["root"] / "independent-audit.json", {"attack": attack, "before_attack": evidence_before, "after_deliberate_attack_before_commands": inventory, "after_commands": protected_inventory(selected), "actual_HTTP_requests_before_attack": received_before, "actual_HTTP_requests_after_attack": len(server.incoming), "scope": "actual original source/HTTP/API bytes; coordinated forgery only in newly owned synthetic output and budget DB"})
        recorder.cases.append({"case_id": component["case_id"], "status": "passed", "actual_HTTP_requests": received_before})


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=os.environ.get("CLI_BIN"), help="explicit fixed ordinary CLI; no production endpoint")
    parser.add_argument("--new-root", type=Path, help="new artifact root under an existing ordinary parent")
    parser.add_argument("--fixture", type=Path, default=FIXTURE)
    parser.add_argument("--case-id", action="append", help="selected frozen case; repeat to select cases")
    parser.add_argument("--test-driver", type=Path, help="explicit libtest driver compiled from the same exact source/fingerprint as selected CLI")
    parser.add_argument("--build-receipt", type=Path, help="explicit actual.json pair receipt beside source-before/after and source-originals.zip; independently bind selected executable and archived compilation inputs")
    parser.add_argument("--full", action="store_true", help="all14 core+5 frozen variants, generic actual interruption matrix, and protection gates; explicit same-source driver required")
    parser.add_argument("--driver-only", action="store_true", help="generic single-unit real callback/OS interruption matrix; separate from core dual-unit cases")
    parser.add_argument("--reserved-expired-only", action="store_true", help="actual Reserved commit kill/expiry/ordinary resume; zero HTTP and NotSent release required")
    parser.add_argument("--protection-only", action="store_true", help="ordinary corruption/input/ownership guards plus real held-worker and sealed-artifact guards; same-source driver required")
    parser.add_argument("--public-only", action="store_true", help="ordinary CLI cases; kill/driver groups require a separately bound test-driver")
    parser.add_argument("--audit-authority", type=Path, help="read-only independently reconstruct an existing explicitly selected synthetic authority")
    parser.add_argument("--authority-config", type=Path, help="explicit original synthetic authority config for read-only audit")
    parser.add_argument("--run-root", type=Path, action="append", help="explicit selected owned Run root; repeat for shared-authority read-only audit, never derive private paths from untrusted ledger")
    parser.add_argument("--old-cli-red", action="store_true", help="save actual unsupported run-api command outcome with a fixed old CLI")
    args = parser.parse_args(argv)
    require(sum(bool(v) for v in (args.full, args.driver_only, args.reserved_expired_only, args.protection_only, args.public_only, args.old_cli_red, args.audit_authority)) <= 1, "choose one explicit execution scope")
    require(not args.full or args.case_id is None, "--full always executes every frozen core case; use --case-id in a selected scope")
    author = author_check(args.fixture)
    if args.audit_authority:
        _, evidence = read_authority(args.audit_authority, fixture=args.fixture, authority_config=args.authority_config, selected_run_roots=args.run_root)
        print(json.dumps(evidence, ensure_ascii=False))
        return 0
    require(args.cli is not None and args.new_root is not None, "--cli/CLI_BIN and --new-root required")
    recorder = Recorder(args.new_root, args.cli, args.fixture)
    try:
        if args.build_receipt is not None:
            recorder.bind_build_receipt(args.build_receipt, args.test_driver)
        if args.old_cli_red:
            config = recorder.root / "authority-config.json"
            config.write_bytes(paths.read_observed_file(args.fixture / "policies/authority-template.json"))
            _, _, receipt = recorder.command(["run", "api", "init", config, recorder.root / "unsupported-budget"], label="old ordinary CLI actual RED")
            require(receipt["exit_code"] != 0 and not (recorder.root / "unsupported-budget").exists(), "old CLI unexpectedly supports or wrote run-api authority")
            recorder.cases.append({"case_id": "old-cli-red", "status": "expected_refusal", "actual_HTTP_requests": 0})
        elif args.reserved_expired_only:
            require(args.test_driver is not None, "--reserved-expired-only requires exact source-bound driver")
            reserved_expired_recovery(recorder, args.test_driver, author["cases"])
        elif args.protection_only:
            require(args.test_driver is not None, "--protection-only requires same-source driver")
            paused_protection_matrix(recorder, author["cases"])
            active_owner_protection(recorder, args.test_driver, author["cases"])
            artifact_protection_matrix(recorder, args.test_driver, author["cases"])
        elif args.driver_only:
            require(args.test_driver is not None, "--driver-only requires explicit --test-driver")
            driver_matrix(recorder, args.test_driver, author)
        else:
            if args.full:
                require(args.test_driver is not None, "--full requires exact same-source test-driver")
            selected_ids = args.case_id or [c["case_id"] for c in author["cases"] if args.full or c["case_id"] not in ("unknown-remote-completion-retained", "retry-backoff-persists-through-kill", "elapsed-limit-persists-through-kill")]
            require(len(selected_ids) == len(set(selected_ids)) and set(selected_ids) <= {c["case_id"] for c in author["cases"]}, "only unique frozen author case IDs")
            for case_id in selected_ids:
                case = next(c for c in author["cases"] if c["case_id"] == case_id)
                if case_id in ("retry-backoff-persists-through-kill", "elapsed-limit-persists-through-kill"):
                    require(args.test_driver is not None, "kill case requires real driver hooks; no fabricated public-only pass")
                    dual_unit_kill_case(recorder, case, args.test_driver)
                else:
                    ordinary_case(recorder, case)
                if args.full:
                    for variant in case["variants"]:
                        derived = parameter_case(case, variant)
                        if variant["name"] == "OS-kill-after-accepted":
                            dual_unit_kill_case(recorder, derived, args.test_driver, accepted=True)
                        else:
                            ordinary_case(recorder, derived)
                    if "simultaneous_real_workers" in case["expected"]:
                        observed_case = copy.deepcopy(case)
                        observed_case["case_id"] += "--actual-worker-observe"
                        ordinary_case(recorder, observed_case, test_driver=args.test_driver)
            if args.full:
                driver_matrix(recorder, args.test_driver, author)
                paused_protection_matrix(recorder, author["cases"])
                active_owner_protection(recorder, args.test_driver, author["cases"])
                artifact_protection_matrix(recorder, args.test_driver, author["cases"])
        save_json(recorder.root / "execution-scope.json", {"requested_scope": "full" if args.full else "driver" if args.driver_only else "protection" if args.protection_only else "reserved-expired" if args.reserved_expired_only else "old-cli-red" if args.old_cli_red else "selected-ordinary", "executed_case_ids": [c["case_id"] for c in recorder.cases], "natural_concurrency_evidence": "only observe+actual server-held cases; trace/selected hook blocking is controlled schedule", "actual_previous_capable_compiler_version_guard": "not_run: requires a separately selected capable previous executable; compiler mirror refusal is separately exercised", "scope": "reported counts come from actual process receipts and actual validated server incoming; arbitrary uninstrumented file/SQL power-loss windows are not claimed"})
        result = recorder.finish("passed")
        print(json.dumps({"status": result["status"], "ordinary_CLI_count": result["ordinary_CLI_count"], "case_count": len(result["cases"])}))
        return 0
    except BaseException as error:
        recorder.finish("failed", {"type": type(error).__name__, "text": str(error)})
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
