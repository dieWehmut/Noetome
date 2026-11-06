"""Independently exercise the bounded fixture Run CLI in an absent test root.

Only the frozen run-checkpoint-v1 author package is selected. This checker never
calls a model, changes author expected, or trusts a self-reported file hash in
place of rereading the file. Durable interruption uses an explicitly selected
Rust test driver; production CLI operations remain ordinary subprocess calls.
"""

import argparse
import copy
from contextlib import closing
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
from urllib.parse import parse_qsl
import re
import queue
import shutil
import sqlite3
import stat
import subprocess
import sys
import threading
import time
import uuid

import verify_fixture_compile as compile_checker


UNITS = ("source_contract", "render", "stage_verification", "vault_readback", "reports", "publication")
AUTHOR_PINS = {
    "case.json": "a4790b5bb67ec8b4fea71ced53cd44a853a226ffecf74d3599daebb08bbc4437",
    "expected.json": "0477e6dc10ee3e5be654a3bbce4c22cb214972bf658a58e9f8a0cf16db39eadd",
    "ir.json": "3dd1e99ef340d1f603c9514be9e5a3b2b2b2225135f0397d20bcffbe7577fc50",
    "note-plan.json": "2944b5d0ec3868947c0880650eaaa3c444c6b030f3b5dc77f876e91f4394e1fb",
    "sources/材料乙.md": "ed8491e5b6935d46ba4708da6d969593f1c79c06739294bde7376435bbfa6535",
    "sources/材料甲.md": "4f5c914504aaab35408dfcc88813b838b9b5ff99efd7023af60888e258425562",
}


class AuditError(ValueError):
    """A real receipt or independent expectation did not agree."""


def require(condition, explanation):
    if not condition:
        raise AuditError(explanation)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def strict_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate JSON key: " + key)
            result[key] = value
        return result
    def constant(value):
        raise AuditError("nonfinite JSON constant: " + value)
    try:
        return json.loads(data, object_pairs_hook=pairs, parse_constant=constant)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise AuditError("invalid JSON receipt") from error


def read_json(path):
    return strict_json(path.read_bytes())


def safe_relative(value):
    require(isinstance(value, str) and value != "" and "\\" not in value and ":" not in value,
            "invalid relative path")
    path = PurePosixPath(value)
    require(not path.is_absolute() and all(p not in ("", ".", "..") for p in value.split("/")),
            "relative path leaves its owner")
    return path


def ordinary_path(path, leaf_missing=False):
    """Reject linked/reparse ancestors before reading or creating selected files."""
    path = Path(path).absolute()
    parts = [path, *path.parents]
    for index, part in enumerate(parts):
        try:
            info = part.lstat()
        except FileNotFoundError:
            require(leaf_missing and index == 0, "missing selected ancestor: " + str(part))
            continue
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400,
                "linked or reparse path: " + str(part))
        require(index == 0 or stat.S_ISDIR(info.st_mode), "non-directory ancestor: " + str(part))
    return path


def inventory(root):
    """Reread bytes, identity and metadata without following linked directories."""
    root = ordinary_path(root)
    result = {}
    pending = [root]
    while pending:
        path = pending.pop()
        info = path.lstat()
        name = "." if path == root else path.relative_to(root).as_posix()
        item = {"mode": stat.S_IMODE(info.st_mode), "mtime_ns": info.st_mtime_ns,
                "device": info.st_dev, "inode": info.st_ino,
                "attributes": getattr(info, "st_file_attributes", None)}
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            item.update(kind="link", target=os.readlink(path) if stat.S_ISLNK(info.st_mode) else None)
        elif stat.S_ISDIR(info.st_mode):
            item["kind"] = "directory"
            pending.extend(sorted(path.iterdir(), reverse=True))
        elif stat.S_ISREG(info.st_mode):
            item.update(kind="file", length=info.st_size, sha256=sha(read_observed_file(path, info)), links=info.st_nlink)
        else:
            item["kind"] = "special"
        result[name] = item
    return dict(sorted(result.items()))


def author_check(fixture):
    fixture = ordinary_path(fixture)
    before = inventory(fixture)
    for relative, digest in AUTHOR_PINS.items():
        path = ordinary_path(fixture.joinpath(*safe_relative(relative).parts))
        require(path.is_file() and sha(path.read_bytes()) == digest, "author package changed: " + relative)
    require(inventory(fixture) == before, "author package changed during readback")
    return before


def frozen_pack_check(pack, expected_sha256=None):
    pack = ordinary_path(pack)
    manifest_path = ordinary_path(pack / "frozen-sha256.json")
    if expected_sha256 is not None:
        require(sha(manifest_path.read_bytes()) == expected_sha256, "selected author freeze anchor changed")
    manifest = read_json(manifest_path)
    require(manifest["schema_version"] == 1 and manifest["producer"] == "fixture_author_freeze"
            and manifest["workflow_id"] == "run-checkpoint-v1", "unknown frozen author package")
    before = inventory(pack)
    actual_files = {name for name, item in before.items() if item["kind"] == "file"}
    require(actual_files == set(manifest["files"]) | {"frozen-sha256.json"}, "author freeze members differ")
    for relative, pin in manifest["files"].items():
        safe_relative(relative)
        require(before[relative]["sha256"] == pin["sha256"] and before[relative]["length"] == pin["byte_length"],
                "frozen author member changed: " + relative)
    author_check(pack / "fixture")
    expected = read_json(pack / "expected.json")
    graph = read_json(pack / "work-graph.json")
    require(graph["ordered_unit_ids"] == list(UNITS), "frozen finite graph differs")
    for index, unit in enumerate(graph["units"]):
        require(unit["unit_id"] == UNITS[index] and unit["depends_on"] == ([] if index == 0 else [UNITS[index - 1]]),
                "author dependency graph differs")
    for file in expected["normative_files"]:
        path = pack.joinpath(*safe_relative(file["expected_file"]).parts)
        require(sha(path.read_bytes()) == file["sha256"] and len(path.read_bytes()) == file["byte_length"],
                "independent normative Markdown changed")
    require(inventory(pack) == before, "frozen author readback changed originals")
    return {"sha256": sha(manifest_path.read_bytes()), "expected": expected, "before": before}


def exact_fields(value, names, description):
    require(isinstance(value, dict) and set(value) == set(names), "unknown/missing " + description + " fields")


def natural(value, maximum, description):
    require(type(value) is int and 0 <= value <= maximum, "invalid " + description)


def utc_time(value):
    require(isinstance(value, str), "timestamp is not text")
    require(re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|\+00:00)", value), "timestamp is not RFC3339 UTC")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise AuditError("invalid actual timestamp") from error
    require(parsed.tzinfo is not None and parsed.utcoffset() == timezone.utc.utcoffset(parsed), "timestamp is not UTC")
    return parsed


def actual_identity(path):
    path = ordinary_path(path)
    info = path.stat()
    require(not path.is_file() or info.st_nlink == 1, "owned file has multiple hardlinks")
    if os.name != "nt":
        return {"platform": "unix", "device": info.st_dev, "inode": info.st_ino}
    # Python's stat exposes a wider modern Windows file ID. Use the ordinary
    # Win32 handle API for the explicitly frozen volume/index64 contract.
    import ctypes
    from ctypes import wintypes
    class FileInfo(ctypes.Structure):
        _fields_ = [("attributes", wintypes.DWORD), ("creation", wintypes.FILETIME),
                    ("access", wintypes.FILETIME), ("write", wintypes.FILETIME),
                    ("volume", wintypes.DWORD), ("size_high", wintypes.DWORD), ("size_low", wintypes.DWORD),
                    ("links", wintypes.DWORD), ("index_high", wintypes.DWORD), ("index_low", wintypes.DWORD)]
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                                  wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.GetFileInformationByHandle.argtypes = [wintypes.HANDLE, ctypes.POINTER(FileInfo)]
    kernel.GetFileInformationByHandle.restype = wintypes.BOOL
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.CreateFileW(str(path), 0, 3, None, 3, 0x02200000, None)
    require(handle not in (None, ctypes.c_void_p(-1).value), "cannot open actual Windows identity")
    try:
        value = FileInfo()
        require(kernel.GetFileInformationByHandle(handle, ctypes.byref(value)), "cannot query actual Windows identity")
        require(not value.attributes & 0x400 and (not path.is_file() or value.links == 1), "linked Windows owner")
        return {"platform": "windows", "volume": value.volume, "index": value.index_high << 32 | value.index_low}
    finally:
        kernel.CloseHandle(handle)


def read_observed_file(path, observed=None):
    """Prove a Windows empty file through metadata while its empty range is locked."""
    path = Path(path)
    observed = path.lstat() if observed is None else observed
    if os.name != "nt" or observed.st_size != 0:
        return path.read_bytes()
    identity = actual_identity(path)
    def unchanged():
        current = path.lstat()
        require(stat.S_ISREG(current.st_mode) and not getattr(current, "st_file_attributes", 0) & 0x400
                and (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns, current.st_nlink)
                == (observed.st_dev, observed.st_ino, 0, observed.st_mtime_ns, observed.st_nlink),
                "empty file metadata changed during readback")
    unchanged()
    require(actual_identity(path) == identity, "empty file identity changed during readback")
    unchanged()
    return b""


def copy_observed_file(source, destination):
    """Archive proved empty bytes; nonempty files retain ordinary copy/read errors."""
    path = Path(source)
    info = path.lstat()
    if os.name == "nt" and stat.S_ISREG(info.st_mode) and info.st_size == 0:
        Path(destination).write_bytes(read_observed_file(path, info))
        shutil.copystat(source, destination)
        return destination
    return shutil.copy2(source, destination)


def canonical_selected_path(path, leaf_missing=False):
    """Use the real Windows handle path after rejecting reparse ancestors."""
    path = ordinary_path(path, leaf_missing=leaf_missing)
    if os.name != "nt":
        return path
    if leaf_missing and not path.exists():
        return canonical_selected_path(path.parent) / path.name
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                                  wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.GetFinalPathNameByHandleW.argtypes = [wintypes.HANDLE, wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD]
    kernel.GetFinalPathNameByHandleW.restype = wintypes.DWORD
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.CreateFileW(str(path), 0, 7, None, 3, 0x02200000, None)
    require(handle not in (None, ctypes.c_void_p(-1).value), "cannot open selected Windows canonical path")
    try:
        size = kernel.GetFinalPathNameByHandleW(handle, None, 0, 0)
        require(0 < size < 32768, "invalid Windows canonical path length")
        buffer = ctypes.create_unicode_buffer(size + 1)
        written = kernel.GetFinalPathNameByHandleW(handle, buffer, len(buffer), 0)
        require(0 < written < len(buffer), "cannot read actual Windows canonical path")
        selected = ordinary_path(Path(buffer.value))
    finally:
        kernel.CloseHandle(handle)
    require(actual_identity(selected) == actual_identity(path), "Windows canonical selection changed file identity")
    return selected


def ordinary_sqlite_namespace(path):
    """Return a checked local drive alias; no unverified UNC authority grant."""
    path = ordinary_path(path)
    if os.name != "nt":
        return path
    text = str(path)
    require(not text.upper().startswith("\\\\?\\UNC\\") and not (text.startswith("\\\\") and not text.startswith("\\\\?\\")),
            "UNC SQLite authority has no selected native validation")
    ordinary = ordinary_path(Path(text[4:]) if text.startswith("\\\\?\\") else path)
    require(re.fullmatch(r"[A-Za-z]:", ordinary.drive) is not None, "unknown Windows SQLite namespace")
    require(actual_identity(path) == actual_identity(ordinary), "SQLite namespace conversion changed actual file identity")
    return ordinary


def sqlite_file_uri(path, query="mode=ro"):
    require(isinstance(query, str) and [value for key, value in parse_qsl(query, keep_blank_values=True) if key == "mode"] == ["ro"],
            "SQLite URI must preserve one read-only mode")
    return ordinary_sqlite_namespace(path).as_uri() + "?" + query


def validate_sqlite_writer_handshake(line):
    require(line in {b"actual_sqlite_writer_ready\n", b"actual_sqlite_writer_ready\r\n"},
            "actual SQLite writer did not reach its live transaction handshake")


def validate_state(state):
    exact_fields(state, ("schema_version", "status", "units", "acknowledged_control", "checks", "manifest",
                         "output_snapshot", "pending_intent", "validation_report", "diagnostic"), "RunState")
    require(type(state["schema_version"]) is int and state["schema_version"] == 1, "unknown RunState schema")
    require(state["status"] in {"queued", "running", "paused", "cancelled", "succeeded", "invalidated", "failed"}, "unknown Run status")
    natural(state["acknowledged_control"], 128, "acknowledged control")
    require(isinstance(state["units"], list) and len(state["units"]) == len(UNITS), "fixed six-unit graph missing")
    confirmed = 0
    while confirmed < len(UNITS) and state["units"][confirmed].get("state") == "confirmed":
        confirmed += 1
    for index, unit in enumerate(state["units"]):
        exact_fields(unit, ("unit_id", "dependencies", "state", "attempts", "output_paths"), "WorkUnit")
        require(unit["unit_id"] == UNITS[index] and unit["dependencies"] == ([] if index == 0 else [UNITS[index - 1]]), "work dependency/identity changed")
        natural(unit["attempts"], 512, "actual unit attempts")
        require(unit["state"] in {"pending", "running", "confirmed"}, "unknown unit status")
        require(isinstance(unit["output_paths"], list) and len(unit["output_paths"]) == len(set(unit["output_paths"])), "duplicate or missing unit output paths")
        for path in unit["output_paths"]:
            safe_relative(path)
        if index < confirmed:
            require(unit["attempts"] > 0, "confirmed unit never started")
        elif index == confirmed:
            require(unit["state"] != "confirmed", "confirmation skipped a dependency")
            require(unit["state"] != "running" or unit["attempts"] > 0, "inflight unit never started")
        else:
            require(unit["state"] == "pending" and unit["attempts"] == 0 and unit["output_paths"] == [], "later unit started before dependency confirmation")
    require(state["status"] != "succeeded" or confirmed == len(UNITS), "success without all six confirmations")
    require(confirmed != len(UNITS) or state["status"] in {"succeeded", "invalidated", "failed"}, "completed graph has nonterminal state")
    require(isinstance(state["checks"], dict) and isinstance(state["manifest"], list), "invalid durable result facts")
    require(state["diagnostic"] is None or isinstance(state["diagnostic"], str), "invalid diagnostic")
    if state["pending_intent"] is not None:
        exact_fields(state["pending_intent"], ("phase", "manifest", "files"), "StoreIntent")
        require(state["pending_intent"]["phase"] in {"planned", "staged", "verified", "published"}
                and isinstance(state["pending_intent"]["manifest"], list) and isinstance(state["pending_intent"]["files"], dict), "unknown prepared output intent")
        for path, expectation in state["pending_intent"]["files"].items():
            safe_relative(path)
            exact_fields(expectation, ("sha256", "byte_length"), "prepared FileExpectation")
            require(isinstance(expectation["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", expectation["sha256"]), "invalid prepared file SHA")
            natural(expectation["byte_length"], 64 * 1024 * 1024, "prepared file length")
    if confirmed > 0:
        require(state["checks"] != {} and isinstance(state["output_snapshot"], dict), "confirmed source lacks durable check/output witness")
    if confirmed >= 2:
        require(len(state["manifest"]) == 2, "confirmed render lacks two actual file witnesses")
    if confirmed >= 5:
        require(isinstance(state["validation_report"], dict), "confirmed reports lack retained validation report")
    if confirmed == 6:
        require(state["output_snapshot"].get("published") is True, "publication confirmation lacks actual committed snapshot")
    return confirmed


def validate_history(rows, run_id, controls):
    """Validate exact stored payload bytes and ordered real state transitions."""
    require(0 < len(rows) <= 512, "missing or excessive checkpoint history")
    previous_hash, previous_state, checkpoints = "", None, []
    confirmation_counts = dict.fromkeys(UNITS, 0)
    for index, (sequence, previous, raw, digest) in enumerate(rows, 1):
        require(type(sequence) is int and sequence == index, "checkpoint sequence gap/reorder")
        require(isinstance(raw, str) and len(raw.encode("utf-8")) <= 8 * 1024 * 1024, "checkpoint payload limit")
        require(previous == previous_hash and isinstance(digest, str) and re.fullmatch(r"[0-9a-f]{64}", digest)
                and sha(raw.encode("utf-8")) == digest, "checkpoint raw-byte hash chain differs")
        checkpoint = strict_json(raw)
        exact_fields(checkpoint, ("schema_version", "sequence", "event", "at", "run_id", "state"), "Checkpoint")
        require(checkpoint["schema_version"] == 1 and type(checkpoint["schema_version"]) is int
                and checkpoint["sequence"] == sequence and type(checkpoint["sequence"]) is int
                and checkpoint["run_id"] == run_id, "checkpoint schema/Run identity differs")
        utc_time(checkpoint["at"])
        state, event = checkpoint["state"], checkpoint["event"]
        count = validate_state(state)
        if state["validation_report"] is not None:
            require(state["validation_report"].get("run_id") == run_id, "validation report belongs to another Run")
        require(state["acknowledged_control"] <= len(controls), "acknowledged nonexistent control")
        if previous_state is None:
            require(event == "created" and count == 0 and all(unit["state"] == "pending" and unit["attempts"] == 0 for unit in state["units"]), "no actual initial creation checkpoint")
            require(state["status"] in {"queued", "paused"} and state["acknowledged_control"] == 0
                    and state["checks"] == {} and state["manifest"] == [] and state["output_snapshot"] is None
                    and state["pending_intent"] is None and state["validation_report"] is None and state["diagnostic"] is None
                    and all(unit["output_paths"] == [] for unit in state["units"]), "creation seed contains unearned control/result/terminal state")
        else:
            old_count = validate_state(previous_state)
            require(previous_state["status"] not in {"cancelled", "invalidated"}, "terminal Run restarted")
            require(previous_state["status"] != "succeeded" or event in {"completed_control", "invalidated", "failed"},
                    "completed Run restarted or reconfirmed work")
            require(previous_state["status"] != "failed" or event in {"resumed", "invalidated"}, "failed Run bypassed explicit checked resume/invalidation")
            require(state["acknowledged_control"] >= previous_state["acknowledged_control"], "control acknowledgement rewound")
            acknowledged = state["acknowledged_control"]
            effective = None
            for control in controls[:acknowledged]:
                if effective != "cancel":
                    effective = control[2]
            if event in {"paused", "cancelled"}:
                require(acknowledged > previous_state["acknowledged_control"] and effective == ("pause" if event == "paused" else "cancel"), "control event lacks its actual dominating request")
            elif event == "completed_control":
                require(acknowledged > previous_state["acknowledged_control"] and effective in {"pause", "cancel"},
                        "completed control lacks a new actual request")
            elif event == "resumed":
                require(effective != "cancel", "resume acknowledged and bypassed cancellation")
            else:
                require(acknowledged == previous_state["acknowledged_control"], "work event forged control acknowledgement")
            require(state["units"][:old_count] == previous_state["units"][:old_count], "confirmed unit changed or repeated")
            if event == "unit_started":
                require(old_count < len(UNITS) and count == old_count, "unit start changed confirmations")
                require(state["units"][old_count]["state"] == "running" and state["units"][old_count]["attempts"] == previous_state["units"][old_count]["attempts"] + 1,
                        "start has no new actual attempt")
                expected_unit = {**previous_state["units"][old_count], "state": "running", "attempts": previous_state["units"][old_count]["attempts"] + 1}
                require(state["units"][old_count] == expected_unit and state["status"] == "running"
                        and all(state[key] == previous_state[key] for key in ("checks", "manifest", "output_snapshot", "pending_intent", "validation_report", "diagnostic")),
                        "unit start rewrote retained facts or output paths")
            elif event in {"unit_confirmed", "publication_reconciled"}:
                require(old_count < len(UNITS) and count == old_count + 1 and previous_state["units"][old_count]["state"] == "running"
                        and state["units"][old_count]["attempts"] == previous_state["units"][old_count]["attempts"], "confirmation lacks its started attempt")
                require(event != "publication_reconciled" or old_count == len(UNITS) - 1, "non-publication result reconciled as commit")
                confirmation_counts[UNITS[old_count]] += 1
                require(state["status"] == ("succeeded" if count == len(UNITS) else "running"), "unit confirmation and Run status differ")
            elif event in {"unit_progress", "unit_prepared"}:
                require(count == old_count and state["units"] == previous_state["units"] and old_count < len(UNITS)
                        and state["units"][old_count]["state"] == "running", "inflight progress changed scheduling/attempt")
                require(event != "unit_prepared" or state["pending_intent"] is not None, "prepared checkpoint lacks a bounded intent")
                require(state["status"] == "running", "inflight event concealed actual Run state")
            elif event == "completed_control":
                require(old_count == count == len(UNITS) and previous_state["status"] == state["status"] == "succeeded"
                        and state["diagnostic"] == "completed before control took effect",
                        "completed control pretended unfinished work was already published")
                require(all(state[key] == previous_state[key] for key in state if key not in {"acknowledged_control", "diagnostic"}),
                        "completed control rewrote confirmed work or retained output facts")
            else:
                require(event in {"resumed", "paused", "cancelled", "invalidated", "failed"}, "unknown state event")
                require(state["units"] == previous_state["units"], "control event changed work confirmations")
                require(all(state[key] == previous_state[key] for key in ("checks", "manifest", "output_snapshot", "pending_intent", "validation_report")), "control event rewrote retained results")
                expected_status = {"resumed": {"queued"}, "paused": {"paused"}, "cancelled": {"cancelled"}, "invalidated": {"invalidated"}, "failed": {"failed"}}[event]
                require(state["status"] in expected_status, "event and visible status differ")
        checkpoints.append(checkpoint)
        previous_hash, previous_state = digest, state
    require(all(value <= 1 for value in confirmation_counts.values()), "same unit confirmed more than once")
    return {"checkpoints": checkpoints, "last_sha256": previous_hash, "state": previous_state, "confirmations": confirmation_counts}


def verify_snapshot(run_root, snapshot, context):
    exact_fields(snapshot, ("schema_version", "context", "root", "parent", "directories", "files", "database",
                            "phase", "failure_release_status", "manifest", "published"), "StoreSnapshot")
    require(snapshot["schema_version"] == 1 and snapshot["context"] == context and isinstance(snapshot["root"], str)
            and Path(snapshot["root"]) == run_root / "output", "snapshot version/context/root differs")
    require(snapshot["phase"] in {"planned", "staged", "verified", "failed", "published"} and type(snapshot["published"]) is bool
            and snapshot["published"] == (snapshot["phase"] == "published"), "unknown/inconsistent output phase")
    exact_fields(snapshot["parent"], ("path", "identity"), "snapshot parent")
    require(isinstance(snapshot["parent"]["path"], str) and Path(snapshot["parent"]["path"]) == run_root
            and snapshot["parent"]["identity"] == actual_identity(run_root), "snapshot parent substituted")
    output = ordinary_path(run_root / "output")
    original = inventory(output)
    directories = {"" if name == "." else name for name, item in original.items() if item["kind"] == "directory"}
    files = {name for name, item in original.items() if item["kind"] == "file"}
    require(directories == set(snapshot["directories"]) and files == set(snapshot["files"]) | {"state/compile.sqlite"}, "snapshot has unknown/missing actual members")
    for relative, identity in snapshot["directories"].items():
        path = output if relative == "" else output.joinpath(*safe_relative(relative).parts)
        require(identity == actual_identity(path), "snapshot directory identity differs: " + relative)
    for relative, stamp in [*snapshot["files"].items(), ("state/compile.sqlite", snapshot["database"])]:
        exact_fields(stamp, ("identity", "sha256", "byte_length"), "output file stamp")
        path = ordinary_path(output.joinpath(*safe_relative(relative).parts))
        data = path.read_bytes()
        require(stamp == {"identity": actual_identity(path), "sha256": sha(data), "byte_length": len(data)}, "snapshot file byte/identity differs: " + relative)
    require(inventory(output) == original, "snapshot audit mutated output")


def verify_partial_author(pack, run_root, journal):
    output = run_root / "output"
    if not output.exists():
        return
    original = inventory(output)
    expected = read_json(pack / "expected.json")
    normative = {item["relative_path"]: (pack / item["expected_file"]).read_bytes() for item in expected["normative_files"]}
    allowed = {"state/compile.sqlite", "reports/validation.json", "reports/validation.md"}
    allowed.update(prefix + relative for prefix in ("staging/vault/", "vault/") for relative in normative)
    allowed_directories = {".", "state", "reports", "staging", "staging/vault", "staging/vault/笔记", "vault", "vault/笔记"}
    require(all(item["kind"] == "file" and name in allowed or item["kind"] == "directory" and name in allowed_directories
                for name, item in original.items()), "partial output has unknown/linked members")
    for prefix in ("staging/vault/", "vault/"):
        for relative, data in normative.items():
            path = output / (prefix + relative)
            if path.exists():
                require(path.read_bytes() == data, "partial staged/Vault bytes differ from independent author")
    report_path = output / "reports/validation.json"
    if report_path.exists():
        require(read_json(report_path) == journal["state"]["validation_report"], "partial report lost its persisted validation witness")
    intent = journal["state"]["pending_intent"]
    if intent is not None:
        for relative, pin in intent["files"].items():
            path = output.joinpath(*safe_relative(relative).parts)
            if path.exists():
                require(sha(path.read_bytes()) == pin["sha256"] and path.stat().st_size == pin["byte_length"], "actual prepared output differs from durable intent")
    database = output / "state/compile.sqlite"
    with closing(sqlite3.connect(sqlite_file_uri(database), uri=True)) as db:
        require(db.execute("PRAGMA integrity_check").fetchall() == [("ok",)], "partial output SQLite integrity failed")
        require(db.execute("SELECT run_id FROM runs").fetchall() == [(journal["owner"]["run_id"],)], "partial output duplicated/switched Run")
        releases = db.execute("SELECT release_id,run_id FROM releases").fetchall()
        require(releases == [(journal["owner"]["release_id"], journal["owner"]["run_id"])], "partial output duplicated/switched Release")
        require(db.execute("SELECT count(*) FROM publication_commits").fetchone()[0] <= 1, "partial output duplicated publication")
    require(inventory(output) == original, "partial independent readback changed product bytes")


def read_journal(run_root, expected_run_id=None, fixture=None, validate_output=True):
    """Read a quiescent journal using SQLite read-only mode, never immutable."""
    run_root = ordinary_path(run_root)
    before = inventory(run_root)
    owner = read_json(ordinary_path(run_root / "control/owner.json"))
    exact_fields(owner, ("schema_version", "producer", "run_id", "release_id", "root", "case_root", "root_identity",
                         "parent_identity", "control_identity", "database_identity", "lock_identity", "case_identity",
                         "compiler_fingerprint", "policy_version"), "Owner")
    require(owner["schema_version"] == 1 and owner["producer"] == "fixture_run" and owner["policy_version"] == "fixture-six-unit-v1", "unknown owner protocol")
    try:
        require(all(str(uuid.UUID(owner[key])) == owner[key] and uuid.UUID(owner[key]).version == 4 for key in ("run_id", "release_id")), "Run/Release is not a persisted UUIDv4")
    except (ValueError, TypeError, AttributeError) as error:
        raise AuditError("invalid Run/Release identity") from error
    require(owner["run_id"] != owner["release_id"] and isinstance(owner["compiler_fingerprint"], str)
            and re.fullmatch(r"[0-9a-f]{64}", owner["compiler_fingerprint"]), "invalid logical release/compiler fingerprint")
    require(owner["root"] == str(run_root), "owner path differs")
    require(expected_run_id is None or owner["run_id"] == expected_run_id, "Run differs from independently retained start receipt")
    for name, path in (("root", run_root), ("parent", run_root.parent), ("control", run_root / "control"),
                       ("database", run_root / "control/run.sqlite"), ("lock", run_root / "control/worker.lock")):
        require(owner[name + "_identity"] == actual_identity(path), "actual " + name + " owner identity differs")
    if fixture is not None:
        fixture = ordinary_path(fixture)
        require(owner["case_root"] == str(fixture) and owner["case_identity"] == actual_identity(fixture), "selected input root identity differs")
    database = ordinary_path(run_root / "control/run.sqlite")
    require({item.name for item in database.parent.iterdir()} == {"owner.json", "run.sqlite", "worker.lock"}, "unknown control member or unfinished SQLite sidefile")
    require({item.name for item in run_root.iterdir()} in ({"control"}, {"control", "output"}), "unknown Run member")
    with closing(sqlite3.connect(sqlite_file_uri(database), uri=True)) as db:
        require(db.execute("PRAGMA integrity_check").fetchall() == [("ok",)], "Run SQLite integrity differs")
        require(db.execute("PRAGMA application_id").fetchone() == (0x4E52554E,) and db.execute("PRAGMA user_version").fetchone() == (1,), "unknown Run SQLite schema")
        require(db.execute("PRAGMA journal_mode").fetchone() == ("delete",), "unapproved journal strategy")
        objects = db.execute("SELECT type,name FROM sqlite_master ORDER BY name").fetchall()
        require(objects == [("table", "checkpoints"), ("table", "controls"), ("table", "metadata")], "unknown Run SQLite object")
        columns = {"metadata": ["singleton", "schema_version", "metadata_json"], "checkpoints": ["sequence", "previous_sha256", "payload_json", "payload_sha256"], "controls": ["sequence", "run_id", "action", "requested_at"]}
        for table, expected in columns.items():
            require([row[1] for row in db.execute("PRAGMA table_info(" + table + ")")] == expected, "unknown journal columns")
        metadata_rows = db.execute("SELECT singleton,schema_version,metadata_json FROM metadata").fetchall()
        require(len(metadata_rows) == 1 and metadata_rows[0][:2] == (1, 1), "Run metadata is not a strict singleton")
        metadata = strict_json(metadata_rows[0][2])
        exact_fields(metadata, ("owner", "owner_file_identity", "context", "inputs"), "RunMetadata")
        require(metadata["owner_file_identity"] == actual_identity(run_root / "control/owner.json"), "original owner file was replaced")
        exact_fields(metadata["context"], ("case_id", "run_id", "release_id", "started_at", "fixture_scope_id", "ir_revision", "plan_revision", "input_hashes", "versions", "counts"), "CompileContext")
        require(metadata["owner"] == owner and metadata["context"]["run_id"] == owner["run_id"]
                and metadata["context"]["release_id"] == owner["release_id"], "SQL/owner/compiler identity differs")
        controls = db.execute("SELECT sequence,run_id,action,requested_at FROM controls ORDER BY sequence").fetchall()
        require(len(controls) <= 128, "control history exceeded finite limit")
        for index, (sequence, run_id, action, at) in enumerate(controls, 1):
            require(sequence == index and run_id == owner["run_id"] and action in {"pause", "cancel"}, "invalid control request chain")
            utc_time(at)
        rows = db.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM checkpoints ORDER BY sequence").fetchall()
    history = validate_history(rows, owner["run_id"], controls)
    if validate_output and history["state"]["output_snapshot"] is not None:
        verify_snapshot(run_root, history["state"]["output_snapshot"], metadata["context"])
    elif validate_output:
        require(not (run_root / "output").exists(), "output exists without any trusted snapshot")
    if fixture is not None:
        require(set(metadata["inputs"]) == set(AUTHOR_PINS), "journal selected input set differs")
        for relative, stamp in metadata["inputs"].items():
            exact_fields(stamp, ("identity", "sha256", "byte_length", "modified"), "FileStamp")
            path = ordinary_path(fixture.joinpath(*safe_relative(relative).parts))
            info = path.stat()
            require(stamp == {"identity": actual_identity(path), "sha256": sha(path.read_bytes()), "byte_length": info.st_size, "modified": str(info.st_mtime_ns)}, "input stamp differs from selected original: " + relative)
    require(inventory(run_root) == before, "read-only independent journal check changed product bytes")
    history.update(owner=owner, metadata=metadata, controls=[list(row) for row in controls], database_sha256=sha(database.read_bytes()))
    return history


def clean_environment(extra=None):
    # Never inspect, log or inherit real API keys, budgets, proxies or credentials.
    names = ("PATH", "SYSTEMROOT", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP", "LANG", "LC_ALL")
    value = {name: os.environ[name] for name in names if name in os.environ}
    value.update(PYTHONIOENCODING="utf-8", PYTHONUTF8="1")
    if extra:
        require(all(name.startswith("NOETOME_RUN_TEST_") for name in extra), "unexpected test environment key")
        value.update(extra)
    return value


def save_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")


def selected_source_pins():
    repository = Path(__file__).resolve().parent.parent
    selected = {repository / name for name in ("Cargo.toml", "Cargo.lock", "scripts/verify_run_checkpoint.py",
                "scripts/verify_fixture_compile.py", "tests/test_verify_run_checkpoint.py")}
    selected.update(repository.glob("crates/*/Cargo.toml"))
    selected.update(repository.glob("crates/*/build.rs"))
    selected.update(repository.glob("crates/*/src/**/*.rs"))
    return {path.relative_to(repository).as_posix(): {"sha256": sha(path.read_bytes()), "byte_length": path.stat().st_size}
            for path in sorted(selected) if path.is_file()}


def normalized_report(report):
    """Only lifecycle identity/times differ; scientific and semantic facts stay."""
    value = copy.deepcopy(report)
    for name in ("run_id", "release_id", "started_at", "finished_at"):
        require(name in value, "missing compile lifecycle field: " + name)
        del value[name]
    require(isinstance(value.get("publication"), dict) and "release_id" in value["publication"],
            "missing publication lifecycle identity")
    del value["publication"]["release_id"]
    return value


def verify_published_output(output, fixture, outcome):
    """Use old independent author/Markdown checker, plus exact table cardinality."""
    require(__debug__, "independent legacy assertions require Python without -O")
    output = ordinary_path(output)
    before = inventory(output)
    report_path = output.joinpath(*safe_relative(outcome["paths"]["json_report"]).parts)
    report = read_json(ordinary_path(report_path))
    require(report == outcome["report"], "compile stdout and persisted report differ")
    for file in report["files"]:
        safe_relative(file["relative_path"])
        for link in file["links"]:
            safe_relative(link["target_path"])
    # This checker predates the Run implementation and fixes both actual note
    # literals from the author. Its expected is never obtained from this CLI.
    legacy_output, legacy_fixture = ordinary_sqlite_namespace(output), ordinary_sqlite_namespace(fixture)
    output_identity, fixture_identity = actual_identity(output), actual_identity(fixture)
    legacy_sqlite, legacy_connections = compile_checker.sqlite3, []
    class LegacyReadConnections:
        @staticmethod
        def connect(*arguments, **keywords):
            connection = sqlite3.connect(*arguments, **keywords)
            legacy_connections.append(connection)
            return connection
    compile_checker.sqlite3 = LegacyReadConnections
    try:
        checked = compile_checker.verify(legacy_output, legacy_fixture, outcome)
    finally:
        compile_checker.sqlite3 = legacy_sqlite
        for connection in legacy_connections:
            connection.close()
    require(actual_identity(legacy_output) == output_identity and actual_identity(legacy_fixture) == fixture_identity,
            "legacy checker namespace bridge changed selected directory identity")
    database = ordinary_path(output / "state/compile.sqlite")
    with closing(sqlite3.connect(sqlite_file_uri(database), uri=True)) as db:
        names = {row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        require(names == {"compile_metadata", "runs", "releases", "coverage_records", "rendered_files", "publication_commits"},
                "compile database has unknown or missing tables")
        for table in ("compile_metadata", "runs", "releases", "publication_commits"):
            require(db.execute("SELECT count(*) FROM " + table).fetchone() == (1,), "duplicate compile confirmation/publication: " + table)
        require(db.execute("SELECT run_id FROM runs").fetchone() == (report["run_id"],), "compile SQL Run identity differs")
    plan = read_json(fixture / "note-plan.json")
    require({file["note_id"] for file in report["files"]} == {note["note_id"] for note in plan["notes"]},
            "stable note identity changed")
    require(all(file["plan_revision"] == plan["revision"] for file in report["files"]), "stable plan revision changed")
    require(inventory(output) == before, "independent compile readback mutated product root")
    checked["normalized_report"] = normalized_report(report)
    checked["actual_vault_files"] = {file["relative_path"]: sha((output / "vault" / file["relative_path"]).read_bytes()) for file in report["files"]}
    return checked


def verify_author_output(pack, output, report):
    expected = read_json(pack / "expected.json")
    require({file["relative_path"] for file in report["files"]} == {file["relative_path"] for file in expected["normative_files"]}, "author note set differs")
    files = {file["relative_path"]: file for file in report["files"]}
    for item in expected["normative_files"]:
        actual = files[item["relative_path"]]
        require(all(actual[key] == item[key] for key in ("note_id", "plan_revision", "byte_length")), "stable author file identity/revision differs")
        normative = (pack / item["expected_file"]).read_bytes()
        require(actual["content_sha256"] == item["sha256"] and (output / "vault" / item["relative_path"]).read_bytes() == normative,
                "actual note differs from independently frozen Markdown")
    semantics = expected["expected_report_semantics"]
    require(all(report[key] == semantics[key] for key in ("producer", "case_id", "result")), "author report result differs")
    require(report["checks"]["scientific_quality"]["status"] == semantics["scientific_quality"], "scientific status falsely promoted")
    for name, value in semantics["stable_counts"].items():
        require(report["counts"][name] == value, "frozen author denominator differs: " + name)
    mappings = []
    links = []
    for file in report["files"]:
        for mapping in file["content_mappings"]:
            scope = mapping["scope"]
            mappings.append({"note_id": file["note_id"], "section_id": scope["section_id"], "node_id": scope["node_id"],
                             "kind": scope["kind"], "row_id": scope["row_id"], "column_id": scope["column_id"],
                             "item_id": scope["item_id"], "atom_ids": mapping["atom_ids"]})
        for link in file["links"]:
            scope = link["scope"]
            links.append({"from_path": file["relative_path"], "from_node_id": scope["node_id"], "from_note_id": file["note_id"],
                          "from_section_id": scope["section_id"], "target_concept_id": link["target_concept_id"],
                          "target_path": link["target_path"], "target_heading": link["target_heading"], "display_text": link["display_text"],
                          "start_byte": link["start_byte"], "end_byte": link["end_byte"]})
    wanted = [{key: item.get(key) for key in ("note_id", "section_id", "node_id", "kind", "row_id", "column_id", "item_id", "atom_ids")} for item in expected["content_mapping_scopes"]]
    key = lambda value: json.dumps(value, ensure_ascii=False, sort_keys=True)
    require(sorted(mappings, key=key) == sorted(wanted, key=key), "mapping scopes/atom identities differ from author")
    wanted_links = [{key: item[key] for key in links[0]} for item in expected["semantic_links"]]
    require(sorted(links, key=key) == sorted(wanted_links, key=key), "actual semantic link identity/range differs from author")


def verify_envelope(run_root, fixture, envelope, expected_run_id=None, expected_status=None, committed_gap=False):
    exact_fields(envelope, ("schema_version", "producer", "operation", "run_id", "release_id", "run_status", "checkpoint_sequence",
                            "confirmed_units", "work_units", "release_status", "controls", "input_hashes", "compiler_fingerprint",
                            "policy_version", "paths", "validation_report", "diagnostic", "scientific_quality", "real_model_calls", "paid_calls"), "CLI envelope")
    require(envelope["schema_version"] == 1 and envelope["producer"] == "fixture_run", "unknown Run CLI envelope")
    require(envelope["scientific_quality"] == "not_run" and envelope["real_model_calls"] == 0 and envelope["paid_calls"] == 0,
            "bounded fixture falsely claims scientific/model work")
    journal = read_journal(run_root, expected_run_id, fixture, validate_output=not committed_gap)
    state, owner = journal["state"], journal["owner"]
    require(envelope["run_id"] == owner["run_id"] and envelope["release_id"] == owner["release_id"], "CLI and durable Run/Release differ")
    require(envelope["run_status"] == state["status"] and (expected_status is None or state["status"] == expected_status), "CLI visible state differs")
    require(envelope["checkpoint_sequence"] == len(journal["checkpoints"]) and envelope["work_units"] == state["units"]
            and envelope["confirmed_units"] == validate_state(state), "CLI progress differs from durable checkpoint")
    require(envelope["validation_report"] == state["validation_report"] and envelope["diagnostic"] == state["diagnostic"], "CLI retained facts differ")
    require(envelope["input_hashes"] == journal["metadata"]["context"]["input_hashes"] and envelope["compiler_fingerprint"] == owner["compiler_fingerprint"]
            and envelope["policy_version"] == owner["policy_version"], "CLI input/strategy binding differs")
    exact_fields(envelope["controls"], ("latest_sequence", "action", "acknowledged_sequence", "request_sequence"), "CLI controls")
    require(envelope["controls"]["latest_sequence"] == len(journal["controls"])
            and envelope["controls"]["acknowledged_sequence"] == state["acknowledged_control"], "CLI control receipt differs")
    require(envelope["paths"] == {"root": str(run_root), "journal": "control/run.sqlite", "output": "output", "vault": "output/vault", "publication_database": "output/state/compile.sqlite"}, "CLI owned paths differ")
    if state["status"] == "succeeded":
        require(envelope["release_status"] == "published" and journal["confirmations"] == dict.fromkeys(UNITS, 1), "successful Run lacks one confirmation per unit")
    if committed_gap:
        require(validate_state(state) == 5 and state["status"] == "running" and state["units"][5]["state"] == "running"
                and envelope["release_status"] == "published", "commit gap changed work confirmation or concealed actual publication")
    return journal


def verify_finished(recorder, pack, run_root, envelope, retained_run_id=None):
    journal = verify_envelope(run_root, recorder.fixture, envelope, retained_run_id, "succeeded")
    # Adapt the actual persisted report to the old compile checker interface;
    # the public stdout original remains independently preserved in commands/.
    report = envelope["validation_report"]
    outcome = {"release_status": envelope["release_status"], "output_created": True,
               "paths": {"json_report": "reports/validation.json", "markdown_report": "reports/validation.md"}, "report": report}
    checked = verify_published_output(run_root / "output", recorder.fixture, outcome)
    verify_author_output(pack, run_root / "output", report)
    require(checked["run_id"] == journal["owner"]["run_id"] and checked["release_id"] == journal["owner"]["release_id"], "two SQLite databases refer to different logical Run/Release")
    return {"run_id": checked["run_id"], "release_id": checked["release_id"], "normalized_report": checked["normalized_report"],
            "files": checked["files"], "confirmations": journal["confirmations"], "attempts": {unit["unit_id"]: unit["attempts"] for unit in journal["state"]["units"]},
            "checkpoint_sequence": len(journal["checkpoints"]), "journal_sha256": journal["database_sha256"]}


def public_audit(recorder, pack):
    frozen = frozen_pack_check(pack)
    cases = []
    baseline = recorder.root / "fresh-old-compile"
    outcome, _ = recorder.command(("case", "compile", recorder.fixture, baseline), label="fresh independent ordinary compiler baseline")
    baseline_check = verify_published_output(baseline, recorder.fixture, outcome)
    verify_author_output(pack, baseline, outcome["report"])
    for name, paused in (("run-default", False), ("run-paused-resumed", True)):
        root = recorder.root / name
        arguments = ("run", "start", recorder.fixture, root, "--paused") if paused else ("run", "start", recorder.fixture, root)
        envelope, _ = recorder.command(arguments, label=name)
        run_id = envelope["run_id"]
        if paused:
            journal = verify_envelope(root, recorder.fixture, envelope, run_id, "paused")
            require(all(unit["attempts"] == 0 for unit in journal["state"]["units"]), "--paused executed work")
            status, _ = recorder.command(("run", "status", "--", root), label="paused ordinary status")
            verify_envelope(root, recorder.fixture, status, run_id, "paused")
            control, receipt = recorder.command(("run", "pause", "--", root), label="persisted ordinary pause")
            require(receipt["elapsed_seconds"] <= 2, "pause request failed two-second bounded fixture target")
            verify_envelope(root, recorder.fixture, control, run_id, "paused")
            envelope, _ = recorder.command(("run", "resume", "--", recorder.fixture, root), label="same Run ordinary resume")
        checked = verify_finished(recorder, pack, root, envelope, run_id)
        require(checked["normalized_report"] == baseline_check["normalized_report"] and checked["files"] == baseline_check["files"], "restored and fresh normative content/stable IDs differ")
        before = inventory(root)
        status, _ = recorder.command(("run", "status", root), label="finished read-only status")
        verify_finished(recorder, pack, root, status, run_id)
        resumed, _ = recorder.command(("run", "resume", recorder.fixture, root), label="finished idempotent resume")
        verify_finished(recorder, pack, root, resumed, run_id)
        require(inventory(root) == before, "successful status/resume duplicated or rewrote product state")
        for operation in ("pause", "cancel"):
            recorder.command(("run", operation, root), 3, "RUN_TERMINAL", label="successful Run terminal control refused")
            require(inventory(root) == before, "terminal rejection changed published Run")
        recorder.command(("run", "start", recorder.fixture, root), 3, "COMPILE_OUTPUT_EXISTS", label="existing Run not overwritten")
        require(inventory(root) == before, "second start overwrote Run")
        cases.append({"case": name, "status": "passed", **checked})
        save_json(recorder.root / "public-progress.json", cases)
    cancelled = recorder.root / "run-cancelled-queued"
    envelope, _ = recorder.command(("run", "start", recorder.fixture, cancelled, "--paused"), label="ordinary cancellation setup")
    run_id = envelope["run_id"]
    envelope, _ = recorder.command(("run", "cancel", cancelled), 3, label="ordinary persisted cancel")
    journal = verify_envelope(cancelled, recorder.fixture, envelope, run_id, "cancelled")
    require(all(unit["attempts"] == 0 for unit in journal["state"]["units"]), "cancelled queued Run executed work")
    before = inventory(cancelled)
    recorder.command(("run", "resume", recorder.fixture, cancelled), 3, "RUN_TERMINAL", label="cancelled Run cannot resume")
    require(inventory(cancelled) == before, "terminal cancellation changed retained originals")
    cases.append({"case": "queued-cancel", "status": "passed", "run_id": run_id, "confirmed_units": 0})
    save_json(recorder.root / "public-progress.json", cases)
    require(inventory(pack) == frozen["before"], "author freeze changed during ordinary Run audit")
    return {"status": "passed", "producer": "independent_bounded_run_audit", "scope": "ordinary lifecycle only; forced interruption and active scheduling require selected test driver",
            "cases": cases, "fixture_freeze_sha256": frozen["sha256"]}


def mutate_json(path, change):
    value = read_json(path)
    change(value)
    save_json(path, value)


def mutate_checkpoint(database, change):
    """A fully rehashed hostile copy still has to satisfy independent semantics."""
    with closing(sqlite3.connect(database)) as db, db:
        sequence, raw = db.execute("SELECT sequence,payload_json FROM checkpoints ORDER BY sequence DESC LIMIT 1").fetchone()
        payload = strict_json(raw)
        change(payload)
        raw = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
        db.execute("UPDATE checkpoints SET payload_json=?,payload_sha256=? WHERE sequence=?", (raw, sha(raw.encode()), sequence))


def forge_empty_success(database):
    with closing(sqlite3.connect(database)) as db, db:
        first = strict_json(db.execute("SELECT payload_json FROM checkpoints WHERE sequence=1").fetchone()[0])
        state = copy.deepcopy(first["state"])
        events = [("created", copy.deepcopy(state))]
        for index in range(len(UNITS)):
            state["status"] = "running"
            state["units"][index].update(state="running", attempts=1)
            events.append(("unit_started", copy.deepcopy(state)))
            state["units"][index]["state"] = "confirmed"
            if index == len(UNITS) - 1:
                state["status"] = "succeeded"
            events.append(("unit_confirmed", copy.deepcopy(state)))
        db.execute("DELETE FROM checkpoints")
        previous = ""
        for sequence, (event, state) in enumerate(events, 1):
            payload = {**first, "sequence": sequence, "event": event, "state": state}
            raw = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
            digest = sha(raw.encode())
            db.execute("INSERT INTO checkpoints VALUES(?,?,?,?)", (sequence, previous, raw, digest))
            previous = digest


def replace_same_bytes(path):
    replacement = path.with_name(path.name + ".replacement")
    replacement.write_bytes(path.read_bytes())
    os.replace(replacement, path)


def forge_work_ack(database):
    with closing(sqlite3.connect(database)) as db, db:
        sequence, raw, digest = db.execute("SELECT sequence,payload_json,payload_sha256 FROM checkpoints ORDER BY sequence DESC LIMIT 1").fetchone()
        payload = strict_json(raw)
        db.execute("INSERT INTO controls VALUES(1,?,'pause',?)", (payload["run_id"], payload["at"]))
        state = payload["state"]
        state.update(status="running", acknowledged_control=1)
        state["units"][0].update(state="running", attempts=1)
        payload.update(sequence=sequence + 1, event="unit_started")
        changed = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
        db.execute("INSERT INTO checkpoints VALUES(?,?,?,?)", (sequence + 1, digest, changed, sha(changed.encode())))


def refusal_audit(recorder):
    rows = []
    mutations = (
        ("unknown-owner-field", lambda root: mutate_json(root / "control/owner.json", lambda value: value.update(extra=True))),
        ("unknown-owner-schema", lambda root: mutate_json(root / "control/owner.json", lambda value: value.update(schema_version=999))),
        ("missing-owner", lambda root: (root / "control/owner.json").unlink()),
        ("same-byte-owner-replaced", lambda root: replace_same_bytes(root / "control/owner.json")),
        ("same-byte-database-replaced", lambda root: replace_same_bytes(root / "control/run.sqlite")),
        ("same-byte-lock-replaced", lambda root: replace_same_bytes(root / "control/worker.lock")),
        ("unknown-top-member", lambda root: (root / "manual.md").write_bytes(b"foreign user note\n")),
        ("unknown-control-member", lambda root: (root / "control/manual.md").write_bytes(b"foreign control original\n")),
        ("unknown-checkpoint-schema-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value.update(schema_version=999))),
        ("unknown-checkpoint-state-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"].update(status="approved"))),
        ("missing-checkpoint-field-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"].pop("diagnostic"))),
        ("forged-confirmation-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"]["units"][0].update(state="confirmed", attempts=1))),
        ("empty-success-full-chain-rehashed", lambda root: forge_empty_success(root / "control/run.sqlite")),
        ("acknowledged-no-control-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"].update(acknowledged_control=1))),
        ("created-cancelled-seed-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"].update(status="cancelled"))),
        ("created-running-seed-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"].update(status="running"))),
        ("created-pending-intent-seed-rehashed", lambda root: mutate_checkpoint(root / "control/run.sqlite", lambda value: value["state"].update(pending_intent={"phase": "planned", "manifest": [], "files": {}}))),
        ("work-event-ack-skips-pause-rehashed", lambda root: forge_work_ack(root / "control/run.sqlite")),
        ("unknown-output-root", lambda root: (root / "output").mkdir()),
    )
    for name, change in mutations:
        root = recorder.root / ("refuse-" + name)
        envelope, _ = recorder.command(("run", "start", recorder.fixture, root, "--paused"), label="refusal setup: " + name)
        verify_envelope(root, recorder.fixture, envelope, envelope["run_id"], "paused")
        change(root)
        evidence, before = recorder.witness(name, root)
        for operation in ("status", "resume", "pause", "cancel"):
            arguments = ("run", "resume", recorder.fixture, root) if operation == "resume" else ("run", operation, root)
            value, _ = recorder.command(arguments, 3, label="hostile original refused: " + name + "/" + operation)
            require(value is None, "untrusted root returned a trusted lifecycle envelope: " + name + "/" + operation)
            after = inventory(root)
            save_json(evidence / (operation + "-after.json"), after)
            if after != before:
                shutil.copytree(root, evidence / (operation + "-after-bytes"), symlinks=True)
            require(after == before, "refusal modified untrusted original: " + name + "/" + operation)
        rows.append({"case": name, "status": "passed", "operations": ["status", "resume", "pause", "cancel"], "preserved_originals": True})
        save_json(recorder.root / "refusal-progress.json", rows)
    wrong_case = recorder.root / "wrong-resume-case"
    envelope, _ = recorder.command(("run", "start", recorder.fixture, wrong_case, "--paused"), label="wrong case argument setup")
    before = inventory(wrong_case)
    other_case = recorder.root / "different-selected-case"
    shutil.copytree(recorder.fixture, other_case)
    for case in (recorder.root / "missing-case", other_case):
        recorder.command(("run", "resume", case, wrong_case), 3, label="mispassed case does not invalidate original Run")
        require(inventory(wrong_case) == before, "mispassed case invalidated or changed original owned Run")
    rows.append({"case": "wrong-or-missing-resume-case", "status": "passed", "preserved_originals": True})
    save_json(recorder.root / "refusal-progress.json", rows)
    for name, relative in (("changed-plan-bytes", "note-plan.json"), ("changed-source-bytes", "sources/材料甲.md")):
        fixture = recorder.root / (name + "-input")
        shutil.copytree(recorder.fixture, fixture)
        root = recorder.root / ("invalidate-" + name)
        envelope, _ = recorder.command(("run", "start", fixture, root, "--paused"), label="input invalidation setup: " + name)
        run_id = envelope["run_id"]
        old = read_journal(root, run_id, fixture)
        path = fixture / relative
        path.write_bytes(path.read_bytes() + b"\n")
        input_before = inventory(fixture)
        recorder.command(("run", "resume", fixture, root), 3, label="changed input refused: " + name)
        require(inventory(fixture) == input_before, "resume rewrote changed selected input")
        changed = read_journal(root, run_id)
        require(changed["state"]["status"] == "invalidated" and changed["state"]["units"] == old["state"]["units"], "changed input was reused or prior confirmations changed")
        require(len(changed["checkpoints"]) == len(old["checkpoints"]) + 1 and changed["checkpoints"][-1]["event"] == "invalidated", "actual input invalidation not durably appended")
        require(not (root / "output").exists(), "invalidated queued input executed output work")
        rows.append({"case": name, "status": "passed", "run_id": run_id, "retained_confirmations": changed["confirmations"], "invalidated": True})
        save_json(recorder.root / "refusal-progress.json", rows)
    return rows


def sqlite_writer(recorder, database, label, cold=False, touch_metadata=True):
    """Kill a real SQLite transaction containing selected original beforeimages."""
    database = ordinary_path(database)
    require(database.name in {"run.sqlite", "compile.sqlite"} and database.is_relative_to(recorder.root), "writer database is outside the owned test root")
    directory = recorder.root / "sqlite-writers" / label
    directory.mkdir(parents=True)
    old_sha = sha(database.read_bytes())
    update = "UPDATE runs SET issues_json=issues_json || ' '"
    code = ("import sqlite3,sys\n"
            "db=sqlite3.connect(sys.argv[1])\n"
            "db.execute('PRAGMA journal_mode=DELETE')\n"
            f"db.execute('PRAGMA cache_size={2048 if cold else 1}')\n"
            "db.execute('BEGIN IMMEDIATE')\n")
    if touch_metadata:
        if database.name == "run.sqlite":
            code += ("original_metadata=db.execute('SELECT metadata_json FROM metadata').fetchone()[0]\n"
                     "db.execute('UPDATE metadata SET metadata_json=zeroblob(length(CAST(metadata_json AS BLOB)))')\n"
                     "db.execute('UPDATE metadata SET metadata_json=?',(original_metadata,))\n")
        else:
            code += f"db.execute({update!r})\n"
    require(touch_metadata or not cold, "unmodified transaction cannot generate an actual cold journal")
    if not cold:
        code += ("db.execute('CREATE TABLE interrupted_writer(payload BLOB)')\n"
                 "for _ in range(32): db.execute('INSERT INTO interrupted_writer VALUES(?)',(b'x'*262144,))\n")
    code += "print('actual_sqlite_writer_ready',flush=True)\nsys.stdin.readline()\n"
    (directory / "writer-source.py").write_text(code, encoding="utf-8", newline="\n")
    argv = [sys.executable, "-X", "utf8", "-c", code, str(database)]
    process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=clean_environment())
    output = queue.Queue()
    reader = threading.Thread(target=lambda: output.put(process.stdout.readline()), daemon=True)
    reader.start()
    receipt = {"producer": "independent_actual_sqlite_writer", "argv": argv, "pid": process.pid, "real_model_calls": 0,
               "started_ns": time.time_ns(), "old_database_sha256": old_sha, "cold_requested": cold,
               "original_metadata_touched_then_restored": touch_metadata and database.name == "run.sqlite", "writer_source_sha256": sha(code.encode())}
    line = None
    try:
        line = output.get(timeout=15)
        (directory / "handshake.bin").write_bytes(line)
        validate_sqlite_writer_handshake(line)
        require(process.poll() is None, "actual SQLite writer exited before its live transaction handshake")
        receipt["handshake_sha256"] = sha(line)
        receipt.update(alive_before_kill=True, termination_requested_ns=time.time_ns(), termination_method="TerminateProcess" if os.name == "nt" else "SIGKILL")
        process.kill()
        stdout, stderr = process.communicate(timeout=15)
        reader.join(timeout=1)
        stdout = line + stdout
        (directory / "stdout.bin").write_bytes(stdout)
        (directory / "stderr.bin").write_bytes(stderr)
        receipt.update(exit_code=process.returncode, stdout_sha256=sha(stdout), stderr_sha256=sha(stderr))
        journal = Path(str(database) + "-journal")
        raw = journal.read_bytes()
        require(len(raw) > 512 and raw[:8] == (b"\0" * 8 if cold else bytes.fromhex("d9d505f920a163d7")), "killed writer did not produce the selected actual hot/cold rollback journal")
        receipt.update(journal_sha256=sha(raw), journal_byte_length=len(raw), journal_header_hex=raw[:8].hex(),
                       actual_database_sha256=sha(database.read_bytes()), finished_ns=time.time_ns(), exit_code=process.returncode)
        save_json(directory / "receipt.json", receipt)
        recorder.auxiliary_calls.append(receipt)
        return receipt
    except Exception:
        if process.poll() is None:
            process.kill()
        stdout, stderr = process.communicate(timeout=15)
        reader.join(timeout=1)
        (directory / "failure-stdout.bin").write_bytes((line or b"") + stdout)
        (directory / "failure-stderr.bin").write_bytes(stderr)
        save_json(directory / "failure-receipt.json", {**receipt, "exit_code": process.returncode})
        raise


def hot_refusal_legacy_audit(recorder):
    """Historical pre-recovery contract; preserved old receipts stay unchanged."""
    root = recorder.root / "actual-hot-journal-legacy"
    recorder.command(("run", "start", recorder.fixture, root, "--paused"), label="legacy actual hot journal setup")
    writer = sqlite_writer(recorder, root / "control/run.sqlite", "legacy-control-hot")
    evidence, original = recorder.witness("actual-hot-journal-legacy", root)
    for operation in ("status", "resume", "pause", "cancel"):
        arguments = ("run", "resume", recorder.fixture, root) if operation == "resume" else ("run", operation, root)
        recorder.command(arguments, 3, label="legacy actual hot journal preserved: " + operation)
        require(inventory(root) == original, "legacy CLI modified actual hot journal")
    return {"case": "actual-hot-journal-legacy", "status": "passed", "writer_receipt": writer, "originals": str(evidence)}


class HeldDriver:
    """Own one test subprocess, wait for its durable handshake, then kill/release."""
    def __init__(self, recorder, driver, test_name, run_root, event, unit, occurrence=1, operation="start", fixture=None):
        self.recorder = recorder
        self.driver = ordinary_path(driver)
        self.before = inventory(self.driver)
        self.directory = recorder.root / "drivers" / f"{len(recorder.driver_calls) + 1:03d}"
        self.directory.mkdir(parents=True)
        self.argv = [str(self.driver), "--exact", test_name, "--nocapture", "--test-threads=1"]
        self.environment = {"NOETOME_RUN_TEST_CASE": str(fixture or recorder.fixture), "NOETOME_RUN_TEST_ROOT": str(run_root),
                            "NOETOME_RUN_TEST_EVENT": event, "NOETOME_RUN_TEST_UNIT": unit,
                            "NOETOME_RUN_TEST_OCCURRENCE": str(occurrence), "NOETOME_RUN_TEST_OPERATION": operation}
        self.expected = {"event": event, "unit": unit, "occurrence": occurrence}
        self.trace_mode = event == "trace"
        self.stdout, self.stderr, self.lines = bytearray(), bytearray(), queue.Queue()
        self.started_ns, self.started_monotonic = time.time_ns(), time.monotonic()
        self.process = subprocess.Popen(self.argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        env=clean_environment(self.environment))
        self.receipt = {"argv": self.argv, "test_environment": self.environment, "pid": self.process.pid,
                        "started_ns": self.started_ns, "driver_sha256": self.before["."]["sha256"], "handshake": None, "handshakes": []}
        self.readers = [threading.Thread(target=self._read, args=(self.process.stdout, self.stdout, True), daemon=True),
                        threading.Thread(target=self._read, args=(self.process.stderr, self.stderr, False), daemon=True)]
        for reader in self.readers:
            reader.start()
        try:
            self.handshake = self._handshake()
        except Exception:
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait(timeout=15)
            self._save("failed_before_handshake")
            raise

    def _read(self, pipe, output, enqueue):
        while True:
            data = pipe.readline()
            if not data:
                break
            output.extend(data)
            if enqueue:
                self.lines.put(data)
        pipe.close()

    def _handshake(self, allow_exit=False):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            try:
                line = self.lines.get(timeout=0.05)
            except queue.Empty:
                if self.process.poll() is not None:
                    if allow_exit and not self.readers[0].is_alive() and self.lines.empty():
                        return None
                    require(allow_exit, "test driver exited before its durable handshake")
                continue
            offset = line.find(b"{")
            if offset < 0:
                continue
            try:
                value = strict_json(line[offset:])
            except AuditError:
                continue
            if not isinstance(value, dict) or value.get("producer") != "run_checkpoint_test_handshake":
                continue
            exact_fields(value, ("producer", "event", "unit", "occurrence", "sequence", "run_id", "pid", "compiler_fingerprint"), "durable test handshake")
            owner = read_json(Path(self.environment["NOETOME_RUN_TEST_ROOT"]) / "control/owner.json")
            require(isinstance(value["compiler_fingerprint"], str) and re.fullmatch(r"[0-9a-f]{64}", value["compiler_fingerprint"])
                    and value["compiler_fingerprint"] == owner["compiler_fingerprint"], "actual driver and durable owner compiler fingerprint differ")
            if self.trace_mode:
                require(value["event"] in {"created", "resumed", "unit_started", "unit_prepared", "unit_progress", "unit_confirmed", "output_committed", "publication_committed", "completed_control"}
                        and value["unit"] in (*UNITS, "") and type(value["occurrence"]) is int and value["occurrence"] > 0, "unknown traced durable boundary")
            else:
                require(all(value[key] == wanted for key, wanted in self.expected.items()), "test handshake hit another durable boundary")
            require(value["pid"] == self.process.pid and self.process.poll() is None, "handshake does not identify a living owned child")
            require(type(value["sequence"]) is int and value["sequence"] > 0, "handshake checkpoint sequence is not durable")
            self.receipt.update(handshake=value, handshake_received_ns=time.time_ns(), alive_at_handshake=True)
            self.receipt["handshakes"].append(value)
            (self.directory / f"handshake-{len(self.receipt['handshakes']):03d}.bin").write_bytes(line)
            save_json(self.directory / "receipt-pending.json", self.receipt)
            return value
        raise AuditError("test driver durable handshake timeout")

    def force_stop(self):
        require(self.process.poll() is None, "owned child was already dead before forced termination")
        self.receipt.update(alive_before_termination=True, termination_requested_ns=time.time_ns(),
                            termination_method="TerminateProcess" if os.name == "nt" else "SIGKILL")
        self.process.kill()
        self.process.wait(timeout=15)
        require(self.process.returncode != 0, "forced termination unexpectedly reported success")
        return self._save("force_killed")

    def release(self, allow_failure=False):
        self.release_boundary()
        self.process.stdin.close()
        try:
            self.process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.force_stop()
            raise AuditError("released test driver did not finish in bounded time")
        result = self._save("released")
        require(allow_failure or self.process.returncode == 0, "released test driver failed; original output retained")
        return result

    def release_boundary(self):
        require(self.process.poll() is None, "owned child exited before release")
        self.receipt["release_requested_ns"] = time.time_ns()
        self.process.stdin.write(b"release\n")
        self.process.stdin.flush()

    def next_boundary(self):
        require(self.trace_mode, "next-boundary is only available for an explicit trace")
        self.release_boundary()
        self.handshake = self._handshake(allow_exit=True)
        return self.handshake

    def finish_trace(self):
        require(self.trace_mode and self.handshake is None, "trace has an unreleased boundary")
        self.process.wait(timeout=15)
        result = self._save("trace_finished")
        require(self.process.returncode == 0, "traced test driver failed")
        return result

    def _save(self, action):
        for reader in self.readers:
            reader.join(timeout=3)
        require(all(not reader.is_alive() for reader in self.readers), "test driver output reader still active")
        if self.process.stdin is not None and not self.process.stdin.closed:
            self.process.stdin.close()
        stdout, stderr = bytes(self.stdout), bytes(self.stderr)
        (self.directory / "stdout.bin").write_bytes(stdout)
        (self.directory / "stderr.bin").write_bytes(stderr)
        self.receipt.update(action=action, exit_code=self.process.returncode, finished_ns=time.time_ns(),
                            elapsed_seconds=time.monotonic() - self.started_monotonic,
                            stdout_sha256=sha(stdout), stderr_sha256=sha(stderr),
                            driver_before_after_equal=inventory(self.driver) == self.before)
        save_json(self.directory / "receipt.json", self.receipt)
        self.recorder.driver_calls.append(self.receipt)
        require(self.receipt["driver_before_after_equal"], "selected durable driver changed")
        return self.receipt


def trace_boundaries(recorder, pack, driver, test_name):
    root = recorder.root / "trace-run"
    held = HeldDriver(recorder, driver, test_name, root, "trace", "")
    boundaries, occurrences = [], {}
    try:
        while held.handshake is not None:
            hand = held.handshake
            pair = (hand["event"], hand["unit"])
            occurrences[pair] = occurrences.get(pair, 0) + 1
            require(hand["occurrence"] == occurrences[pair], "trace omitted/repeated a pair occurrence")
            journal = read_journal(root, hand["run_id"], recorder.fixture, validate_output=False)
            require(hand["sequence"] == len(journal["checkpoints"]), "trace handshake checkpoint was not actually committed")
            verify_partial_author(pack, root, journal)
            boundaries.append({key: hand[key] for key in ("event", "unit", "occurrence", "sequence")})
            save_json(recorder.root / "trace-progress.json", boundaries)
            held.next_boundary()
        held.finish_trace()
    except Exception:
        if held.process.poll() is None:
            held.force_stop()
        raise
    for unit in UNITS:
        require(("unit_started", unit) in occurrences and ("unit_confirmed", unit) in occurrences, "trace omitted fixed unit lifecycle: " + unit)
    require(("created", "") in occurrences and ("publication_committed", "publication") in occurrences, "trace omitted durable initial/final commit")
    require(any(event == "unit_prepared" for event, _ in occurrences) and any(event == "output_committed" for event, _ in occurrences), "trace omitted prepared/actual output commit windows")
    outcome, _ = recorder.command(("run", "status", root), label="ordinary status verifies completed test-driver trace")
    verify_finished(recorder, pack, root, outcome, held.receipt["handshakes"][0]["run_id"])
    save_json(recorder.root / "trace-boundaries.json", boundaries)
    return boundaries


def interruption_audit(recorder, pack, driver, test_name, baseline):
    boundaries = trace_boundaries(recorder, pack, driver, test_name)
    rows = []
    for index, boundary in enumerate(boundaries, 1):
        label = f"kill-{index:03d}-{boundary['event']}-{boundary['unit'] or 'run'}-{boundary['occurrence']}"
        root = recorder.root / label
        held = HeldDriver(recorder, driver, test_name, root, boundary["event"], boundary["unit"], boundary["occurrence"])
        try:
            run_id = held.handshake["run_id"]
            held.force_stop()
            old = read_journal(root, run_id, recorder.fixture, validate_output=False)
            require(held.handshake["sequence"] == len(old["checkpoints"]), "killed child did not leave its acknowledged checkpoint")
            verify_partial_author(pack, root, old)
            recorder.witness(label, root)
            outcome, receipt = recorder.command(("run", "resume", recorder.fixture, root), label="ordinary resume after " + label)
            checked = verify_finished(recorder, pack, root, outcome, run_id)
            require(checked["files"] == baseline["files"] and checked["normalized_report"] == baseline["normalized_report"], "forced interruption changed stable author content/IDs")
            for unit in old["state"]["units"]:
                if unit["state"] == "confirmed":
                    require(checked["attempts"][unit["unit_id"]] == unit["attempts"], "already-confirmed unit re-executed after kill")
            after = inventory(root)
            again, _ = recorder.command(("run", "resume", recorder.fixture, root), label="second successful resume after " + label)
            verify_finished(recorder, pack, root, again, run_id)
            require(inventory(root) == after, "second recovery duplicated publication or changed result bytes")
            rows.append({"case": label, "status": "passed", "boundary": boundary, "run_id": run_id,
                         "confirmed_before_kill": old["confirmations"], "attempts_after": checked["attempts"],
                         "confirmations_after": checked["confirmations"], "resume_receipt_cli_sha256": receipt["cli_sha256"]})
            save_json(recorder.root / "interruptions-progress.json", rows)
        except Exception:
            if held.process.poll() is None:
                held.force_stop()
            raise
    return rows


def publication_visibility_audit(recorder, pack, driver, test_name):
    root = recorder.root / "publication-commit-before-checkpoint-status"
    held = HeldDriver(recorder, driver, test_name, root, "publication_committed", "publication", 1)
    try:
        run_id = held.handshake["run_id"]
        evidence, before = recorder.witness("publication-commit-before-checkpoint-status", root)
        envelope, _ = recorder.command(("run", "status", root), label="readonly status binds actual publication COMMIT before final WorkUnit checkpoint")
        journal = verify_envelope(root, recorder.fixture, envelope, run_id, "running", committed_gap=True)
        report = envelope["validation_report"]
        actual = {"release_status": "published", "output_created": True,
                  "paths": {"json_report": "reports/validation.json", "markdown_report": "reports/validation.md"}, "report": report}
        checked = verify_published_output(root / "output", recorder.fixture, actual)
        verify_author_output(pack, root / "output", report)
        require(checked["run_id"] == run_id and validate_state(journal["state"]) == 5, "status confounded Run checkpoint with authoritative publication database")
        require(inventory(root) == before, "readonly status rewrote COMMIT or final checkpoint gap")
        held.release()
        final, _ = recorder.command(("run", "status", root), label="ordinary status after final publication unit confirmation")
        verify_finished(recorder, pack, root, final, run_id)
        return [{"case": "publication-commit-before-checkpoint-status", "status": "passed", "visible_work_status": "running",
                 "confirmed_units_before": 5, "authoritative_release_status": "published", "readonly_originals": str(evidence)}]
    except Exception:
        if held.process.poll() is None:
            held.force_stop()
        raise


def publication_control_audit(recorder, pack, driver, test_name):
    """A committed inflight release stays published while late controls are ACKed."""
    rows = []
    for actions in (("pause",), ("cancel",), ("cancel", "pause")):
        label = "publication-inflight-late-" + "-then-".join(actions)
        root = recorder.root / label
        held = HeldDriver(recorder, driver, test_name, root, "publication_committed", "publication", 1)
        try:
            run_id = held.handshake["run_id"]
            original = read_journal(root, run_id, recorder.fixture, validate_output=False)
            output_before = inventory(root / "output")
            receipts = []
            for action in actions:
                envelope, receipt = recorder.command(("run", action, root), label="late inflight committed publication " + action)
                require(receipt["elapsed_seconds"] <= 2, "late publication control exceeded two-second response")
                requested = verify_envelope(root, recorder.fixture, envelope, run_id, "running", committed_gap=True)
                require(requested["state"] == original["state"], "late control rewrote already committed inflight work")
                require(inventory(root / "output") == output_before, "late control rolled back or rewrote published bytes")
                receipts.append(receipt)
            held.release()
            envelope, _ = recorder.command(("run", "status", root), label="completed publication retains explicit late control acknowledgement")
            final = read_journal(root, run_id, recorder.fixture)
            require(final["state"]["status"] == "succeeded" and validate_state(final["state"]) == 6,
                    "late control pretended committed publication was cancelled or paused")
            require(final["state"]["acknowledged_control"] == len(actions), "completed publication lost late control acknowledgement")
            require(final["checkpoints"][-1]["event"] == "completed_control"
                    and final["state"]["diagnostic"] == "completed before control took effect",
                    "late control lacks explicit completed-before-effect result")
            checked = verify_finished(recorder, pack, root, envelope, run_id)
            require(inventory(root / "output") == output_before and all(attempt == 1 for attempt in checked["attempts"].values()),
                    "late control changed publication or repeated work")
            evidence, before = recorder.witness(label, root)
            for arguments in (("run", "status", root), ("run", "resume", recorder.fixture, root)):
                outcome, _ = recorder.command(arguments, label="late completed control idempotent " + arguments[1])
                verify_finished(recorder, pack, root, outcome, run_id)
                require(inventory(root) == before, "completed control replay appended duplicate result or publication")
            rows.append({"case": label, "status": "passed", "requested_actions": list(actions),
                         "control_receipts_seconds": [receipt["elapsed_seconds"] for receipt in receipts],
                         "acknowledged_control": len(actions), "run_status": "succeeded", "release_status": "published",
                         "completed_control": True, "confirmed_units": 6, "originals": str(evidence)})
            save_json(recorder.root / "publication-control-progress.json", rows)
        except Exception:
            if held.process.poll() is None:
                held.force_stop()
            raise
    return rows


def publication_control_interruption_audit(recorder, pack, driver, test_name):
    """Kill both actual late-control checkpoint gaps and resume the same Run."""
    rows = []
    for actions in (("pause",), ("cancel",), ("cancel", "pause")):
        for boundary in ("unit_confirmed", "completed_control"):
            label = "publication-late-" + "-then-".join(actions) + "-kill-" + boundary
            root = recorder.root / label
            held = HeldDriver(recorder, driver, test_name, root, "trace", "", 1)
            try:
                while held.handshake["event"] != "publication_committed":
                    require(len(held.receipt["handshakes"]) <= 64, "late publication trace exceeded fixed bounded graph")
                    require(held.next_boundary() is not None, "late-control trace finished before actual publication COMMIT")
                run_id = held.handshake["run_id"]
                before = read_journal(root, run_id, recorder.fixture, validate_output=False)
                output_before = inventory(root / "output")
                for action in actions:
                    envelope, receipt = recorder.command(("run", action, root), label="late publication request before kill: " + action)
                    require(receipt["elapsed_seconds"] <= 2, "late publication control exceeded two-second response")
                    requested = verify_envelope(root, recorder.fixture, envelope, run_id, "running", committed_gap=True)
                    require(requested["state"] == before["state"] and inventory(root / "output") == output_before,
                            "late request changed already committed inflight work")
                require(held.next_boundary() is not None and held.handshake["event"] == "unit_confirmed"
                        and held.handshake["unit"] == "publication", "missing actual final work confirmation before control acknowledgement")
                confirmed = read_journal(root, run_id, recorder.fixture)
                require(validate_state(confirmed["state"]) == 6 and confirmed["state"]["status"] == "succeeded"
                        and confirmed["state"]["acknowledged_control"] == 0, "late request was falsely ACKed by the work confirmation")
                if boundary == "completed_control":
                    require(held.next_boundary() is not None and held.handshake["event"] == "completed_control"
                            and held.handshake["unit"] == "publication", "missing actual durable completed-control boundary")
                    completed = read_journal(root, run_id, recorder.fixture)
                    require(completed["state"]["acknowledged_control"] == len(actions)
                            and completed["state"]["units"] == confirmed["state"]["units"],
                            "completed-control boundary lost requests or changed confirmed work")
                evidence, original = recorder.witness(label, root)
                stopped = held.force_stop()
                outcome, _ = recorder.command(("run", "resume", recorder.fixture, root), label="ordinary same-Run resume after actual late-control kill")
                checked = verify_finished(recorder, pack, root, outcome, run_id)
                restored = read_journal(root, run_id, recorder.fixture)
                require(restored["state"]["acknowledged_control"] == len(actions)
                        and restored["state"]["diagnostic"] == "completed before control took effect"
                        and sum(checkpoint["event"] == "completed_control" for checkpoint in restored["checkpoints"]) == 1,
                        "late-control recovery lost or duplicated its durable acknowledgement")
                require(restored["state"]["units"] == confirmed["state"]["units"]
                        and all(attempt == 1 for attempt in checked["attempts"].values()) and inventory(root / "output") == output_before,
                        "late-control recovery repeated published work or modified product bytes")
                if boundary == "completed_control":
                    require(inventory(root) == original, "resume after completed-control checkpoint rewrote any original")
                else:
                    require(len(restored["checkpoints"]) == len(confirmed["checkpoints"]) + 1,
                            "missing-control recovery appended more than its single completed-control event")
                once = inventory(root)
                again, _ = recorder.command(("run", "resume", recorder.fixture, root), label="second completed-control resume remains byte/mtime/identity identical")
                verify_finished(recorder, pack, root, again, run_id)
                require(inventory(root) == once, "second late-control resume rewrote or duplicated durable work")
                rows.append({"case": label, "status": "passed", "requested_actions": list(actions), "kill_boundary": boundary,
                             "actual_pid": stopped["pid"], "force_terminated": True, "acknowledged_control": len(actions),
                             "completed_control_count": 1, "confirmed_units": 6, "originals": str(evidence)})
                save_json(recorder.root / "publication-control-interruption-progress.json", rows)
            except Exception:
                if held.process.poll() is None:
                    held.force_stop()
                raise
    return rows


def partial_refusal_audit(recorder, pack, driver, test_name):
    rows = []
    for name, synchronised in (("truncated-pending-file", False), ("changed-value-pending-file-synchronised", True)):
        root = recorder.root / name
        held = HeldDriver(recorder, driver, test_name, root, "output_committed", "render", 1)
        try:
            held.force_stop()
            path = root / "output/staging/vault/笔记/甲容器.md"
            original = path.read_bytes()
            changed = original.replace(b"5 cm", b"6 cm") if synchronised else original[:20]
            require(changed != original, "pending mutation did not change actual authored bytes")
            path.write_bytes(changed)
            if synchronised:
                digest = sha(changed)
                def forged(checkpoint):
                    state = checkpoint["state"]
                    state["pending_intent"]["files"]["staging/vault/笔记/甲容器.md"] = {"sha256": digest, "byte_length": len(changed)}
                    for manifest in (state["manifest"], state["pending_intent"]["manifest"]):
                        for file in manifest:
                            if file["relative_path"] == "笔记/甲容器.md":
                                file.update(content_sha256=digest, byte_length=len(changed))
                mutate_checkpoint(root / "control/run.sqlite", forged)
            evidence, before = recorder.witness(name, root)
            for operation in ("status", "resume", "pause", "cancel"):
                arguments = ("run", "resume", recorder.fixture, root) if operation == "resume" else ("run", operation, root)
                recorder.command(arguments, 3, label="untrusted pending author bytes refused: " + name + "/" + operation)
                after = inventory(root)
                save_json(evidence / (operation + "-after.json"), after)
                require(after == before, "pending refusal changed corrupted/untrusted original: " + name)
            rows.append({"case": name, "status": "passed", "actual_known_boundary_killed": held.receipt["handshake"],
                         "synchronised_sha_manifest_and_checkpoint": synchronised, "author_source_expected_unchanged": True,
                         "preserved_originals": True, "originals": str(evidence)})
            save_json(recorder.root / "partial-refusal-progress.json", rows)
        except Exception:
            if held.process.poll() is None:
                held.force_stop()
            raise
    return rows


def active_control_audit(recorder, pack, driver, test_name, baseline):
    rows = []
    root = recorder.root / "active-pause-inflight"
    held = HeldDriver(recorder, driver, test_name, root, "unit_progress", "render", 1)
    try:
        run_id = held.handshake["run_id"]
        before = read_journal(root, run_id, recorder.fixture)
        pause, receipt = recorder.command(("run", "pause", root), label="active pause request")
        require(receipt["elapsed_seconds"] <= 2, "active pause response exceeded two seconds")
        request = verify_envelope(root, recorder.fixture, pause, run_id)
        require(request["state"]["units"][1]["state"] == "running" and request["state"]["acknowledged_control"] == 0,
                "pause concealed or pretended to flush the held inflight unit")
        # The explicit test callback holds one actual inflight unit. Request
        # acceptance and the two-second scheduling observation are separate.
        accepted = time.monotonic()
        time.sleep(2.05)
        deadline = read_journal(root, run_id, recorder.fixture)
        deadline_elapsed = time.monotonic() - accepted
        require(deadline["state"]["units"] == before["state"]["units"], "new unit scheduled during the pause deadline")
        held.release()
        status, _ = recorder.command(("run", "status", root), label="actual safely flushed paused Run")
        paused = verify_envelope(root, recorder.fixture, status, run_id, "paused")
        require(validate_state(paused["state"]) == 2 and paused["state"]["acknowledged_control"] == 1, "pause did not stop before scheduling the next unit")
        require(all(unit["attempts"] == 0 for unit in paused["state"]["units"][2:]), "paused Run scheduled downstream work")
        verify_partial_author(pack, root, paused)
        outcome, _ = recorder.command(("run", "resume", recorder.fixture, root), label="ordinary resume after safely flushed pause")
        checked = verify_finished(recorder, pack, root, outcome, run_id)
        require(checked["files"] == baseline["files"] and all(value == 1 for value in checked["attempts"].values()), "pause/resume re-executed confirmed work")
        rows.append({"case": "active-pause-inflight", "status": "passed", "pause_receipt_seconds": receipt["elapsed_seconds"],
                     "deadline_observation_seconds": deadline_elapsed, "inflight_unit": "render", "confirmed_when_paused": 2})
    except Exception:
        if held.process.poll() is None:
            held.force_stop()
        raise
    save_json(recorder.root / "active-control-progress.json", rows)
    root = recorder.root / "active-cancel-then-pause"
    held = HeldDriver(recorder, driver, test_name, root, "unit_confirmed", "vault_readback", 1)
    try:
        run_id = held.handshake["run_id"]
        confirmed = read_journal(root, run_id, recorder.fixture)
        for action in ("cancel", "pause"):
            envelope, _ = recorder.command(("run", action, root), label="held active " + action + " request")
            verify_envelope(root, recorder.fixture, envelope, run_id)
        held.release()
        status, _ = recorder.command(("run", "status", root), 3, label="cancel dominates later active pause")
        cancelled = verify_envelope(root, recorder.fixture, status, run_id, "cancelled")
        require(cancelled["state"]["acknowledged_control"] == 2 and cancelled["state"]["units"] == confirmed["state"]["units"], "cancel/pause race lost cancellation or changed retained confirmed units")
        require(validate_state(cancelled["state"]) == 4 and all(unit["attempts"] == 0 for unit in cancelled["state"]["units"][4:]), "cancel scheduled reports/publication")
        verify_partial_author(pack, root, cancelled)
        database = root / "output/state/compile.sqlite"
        with closing(sqlite3.connect(sqlite_file_uri(database), uri=True)) as db:
            require(db.execute("SELECT count(*) FROM publication_commits").fetchone() == (0,), "cancelled Run falsely published")
        evidence, original = recorder.witness("cancel-then-pause-terminal", root)
        recorder.command(("run", "resume", recorder.fixture, root), 3, "RUN_TERMINAL", label="cancelled partial Run cannot resume")
        require(inventory(root) == original, "terminal cancelled partial Run was modified")
        rows.append({"case": "active-cancel-then-pause", "status": "passed", "confirmed_retained": 4, "acknowledged_control": 2,
                     "terminal_originals": str(evidence), "publication_count": 0})
    except Exception:
        if held.process.poll() is None:
            held.force_stop()
        raise
    save_json(recorder.root / "active-control-progress.json", rows)
    return rows


def concurrent_audit(recorder, pack, driver, test_name, baseline):
    root = recorder.root / "concurrent-owned-run"
    held = HeldDriver(recorder, driver, test_name, root, "unit_started", "render", 1)
    try:
        run_id = held.handshake["run_id"]
        before = read_journal(root, run_id, recorder.fixture)
        evidence, original = recorder.witness("concurrent-owned-run", root)
        recorder.command(("run", "resume", recorder.fixture, root), 3, "RUN_BUSY", label="second actual worker refused while first own PID holds Run")
        require(inventory(root) == original, "concurrent rejected worker changed first worker state")
        status, _ = recorder.command(("run", "status", root), label="read-only status remains available while worker holds Run")
        verify_envelope(root, recorder.fixture, status, run_id, "running")
        other = recorder.root / "independent-run-not-globally-locked"
        queued, _ = recorder.command(("run", "start", recorder.fixture, other, "--paused"), label="independent Run starts while first Run is held")
        verify_envelope(other, recorder.fixture, queued, queued["run_id"], "paused")
        held.force_stop()
        outcome, _ = recorder.command(("run", "resume", recorder.fixture, root), label="stale OS worker lock released by real process death")
        checked = verify_finished(recorder, pack, root, outcome, run_id)
        require(checked["files"] == baseline["files"] and checked["attempts"]["source_contract"] == before["state"]["units"][0]["attempts"], "concurrent recovery re-executed confirmed source work")
        return [{"case": "same-run-holder-and-independent-run", "status": "passed", "owner_pid": held.process.pid,
                 "real_owner_killed": True, "retained_source_attempts": checked["attempts"]["source_contract"], "originals": str(evidence)}]
    except Exception:
        if held.process.poll() is None:
            held.force_stop()
        raise


def temporary_io_audit(recorder, pack, driver, test_name, baseline):
    if os.name == "nt" or not hasattr(os, "geteuid") or os.geteuid() == 0:
        return [{"case": "temporary-reports-permission", "status": "not_run", "reason": "requires ordinary non-root POSIX permission enforcement; no Windows sharing claim"}]
    root = recorder.root / "temporary-reports-permission"
    held = HeldDriver(recorder, driver, test_name, root, "unit_started", "reports", 1)
    reports = root / "output/reports"
    original_mode = stat.S_IMODE(reports.stat().st_mode)
    changed_mode = original_mode & ~0o222
    original_identity = actual_identity(reports)
    try:
        run_id = held.handshake["run_id"]
        before = read_journal(root, run_id, recorder.fixture)
        require(validate_state(before["state"]) == 4, "I/O failure did not begin after four real confirmations")
        reports.chmod(changed_mode)
        held.release(allow_failure=True)
        require(not (reports / "validation.json").exists(), "permission failure unexpectedly wrote a JSON report")
        failed = read_journal(root, run_id, recorder.fixture, validate_output=False)
        require(failed["state"]["status"] == "failed" and failed["state"]["units"] == before["state"]["units"], "actual I/O failure did not retain confirmed prefix/attempt")
        require(failed["state"]["diagnostic"] is not None and ("Permission denied" in failed["state"]["diagnostic"] or "permission denied" in failed["state"]["diagnostic"]), "no actual PermissionDenied I/O diagnostic")
        evidence, _ = recorder.witness("temporary-io-failed", root)
        reports.chmod(original_mode)
        require(actual_identity(reports) == original_identity, "permission restoration replaced the owned directory")
        outcome, _ = recorder.command(("run", "resume", recorder.fixture, root), label="same Run retries unconfirmed reports after actual I/O permission restored")
        checked = verify_finished(recorder, pack, root, outcome, run_id)
        require(checked["files"] == baseline["files"] and checked["normalized_report"] == baseline["normalized_report"], "I/O recovery changed independent author output")
        require(all(checked["attempts"][unit] == 1 for unit in UNITS[:4]) and checked["attempts"]["reports"] == 2
                and checked["attempts"]["publication"] == 1, "I/O recovery duplicated confirmed work or lost real failed attempt")
        final = read_journal(root, run_id, recorder.fixture)
        require(any(checkpoint["event"] == "failed" for checkpoint in final["checkpoints"]), "successful recovery deleted actual failed history")
        return [{"case": "temporary-reports-permission", "status": "passed", "original_mode": original_mode,
                 "failure_mode": changed_mode, "restored_same_directory_identity": True, "confirmed_retained": 4,
                 "reports_attempts": 2, "publication_count": 1, "failed_originals": str(evidence)}]
    finally:
        reports.chmod(original_mode)
        if held.process.poll() is None:
            held.force_stop()


def compiler_version_audit(recorder, pack, previous_cli):
    """Exercise two selected real binaries; never edit a version field as proof."""
    if previous_cli is None:
        return [{"case": "actual-compiler-version-change", "status": "not_run", "reason": "requires explicit earlier B07 --previous-cli that can create a Run"}]
    root = recorder.root / "previous-compiler-run"
    old, old_receipt = recorder.command(("run", "start", recorder.fixture, root, "--paused"), cli=previous_cli,
                                       label="earlier actual B07 compiler creates paused owned Run")
    require(old["run_status"] == "paused" and old["confirmed_units"] == 0, "previous CLI did not create a nonexecuted paused Run")
    previous_owner = read_json(root / "control/owner.json")
    original_metadata = None
    with closing(sqlite3.connect(sqlite_file_uri(root / "control/run.sqlite"), uri=True)) as db:
        original_metadata = db.execute("SELECT metadata_json FROM metadata").fetchone()[0]
        original_rows = db.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM checkpoints ORDER BY sequence").fetchall()
    evidence, original = recorder.witness("previous-compiler-original", root)
    current_root = recorder.root / "current-compiler-version-probe"
    current, _ = recorder.command(("run", "start", recorder.fixture, current_root, "--paused"), label="current compiler fingerprint independent probe")
    verify_envelope(current_root, recorder.fixture, current, current["run_id"], "paused")
    require(previous_owner["compiler_fingerprint"] != current["compiler_fingerprint"], "selected binaries have identical logical compiler fingerprint; no actual version-change evidence")
    recorder.command(("run", "resume", recorder.fixture, root), 3, label="current actual compiler refuses incompatible previous Run")
    after = inventory(root)
    require(not (root / "output").exists(), "incompatible previous Run executed work")
    if after == original:
        behavior = "strict refusal preserved complete old Run bytes"
    else:
        with closing(sqlite3.connect(sqlite_file_uri(root / "control/run.sqlite"), uri=True)) as db:
            require(db.execute("SELECT metadata_json FROM metadata").fetchone()[0] == original_metadata, "compiler change rewrote original metadata")
            rows = db.execute("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM checkpoints ORDER BY sequence").fetchall()
        require(rows[:-1] == original_rows and len(rows) == len(original_rows) + 1, "compiler change changed prior checkpoint history")
        last = strict_json(rows[-1][2])
        require(last["event"] == "invalidated" and last["state"]["status"] == "invalidated"
                and last["state"]["units"] == strict_json(original_rows[-1][2])["state"]["units"], "compiler change reused or modified original units")
        require((root / "control/owner.json").read_bytes() == (evidence / "before-bytes/control/owner.json").read_bytes(), "compiler change rewrote old owner")
        behavior = "one actual invalidation appended; prior owner/metadata/checkpoints/units retained"
    save_json(evidence / "after.json", after)
    outcome, _ = recorder.command(("run", "resume", recorder.fixture, current_root), label="new Run executes using the current actual compiler")
    checked = verify_finished(recorder, pack, current_root, outcome, current["run_id"])
    return [{"case": "actual-compiler-version-change", "status": "passed", "previous_cli_sha256": old_receipt["cli_sha256"],
             "current_cli_sha256": recorder.cli_before["."]["sha256"], "previous_compiler_fingerprint": previous_owner["compiler_fingerprint"],
             "current_compiler_fingerprint": current["compiler_fingerprint"], "behavior": behavior,
             "current_new_run_confirmations": checked["confirmations"], "originals": str(evidence)}]


def journal_fixture(recorder, pack, driver, test_name, label, kind):
    root = recorder.root / label
    if kind == "control":
        envelope, _ = recorder.command(("run", "start", recorder.fixture, root, "--paused"), label="control journal fixture: " + label)
        journal = verify_envelope(root, recorder.fixture, envelope, envelope["run_id"], "paused")
        database = root / "control/run.sqlite"
    else:
        if driver is None:
            envelope, _ = recorder.command(("run", "start", recorder.fixture, root), label="published output journal fixture: " + label)
            verify_finished(recorder, pack, root, envelope, envelope["run_id"])
            journal = read_journal(root, envelope["run_id"], recorder.fixture)
        else:
            held = HeldDriver(recorder, driver, test_name, root, "unit_confirmed", "vault_readback", 1)
            held.force_stop()
            journal = read_journal(root, held.handshake["run_id"], recorder.fixture)
            verify_partial_author(pack, root, journal)
            require(validate_state(journal["state"]) == 4, "output journal fixture has no four-unit durable snapshot")
        database = root / "output/state/compile.sqlite"
        require(sha(database.read_bytes()) == journal["state"]["output_snapshot"]["database"]["sha256"], "pre-writer database lacks its durable original SHA witness")
    return root, database, journal


def journal_readonly_refusal(recorder, root, label, include_resume=False):
    evidence, original = recorder.witness(label, root)
    for operation in (("status", "pause", "cancel", "resume") if include_resume else ("status", "pause", "cancel")):
        arguments = ("run", "resume", recorder.fixture, root) if operation == "resume" else ("run", operation, root)
        envelope, _ = recorder.command(arguments, 3, label="journal original refused: " + label + "/" + operation)
        require(envelope is None, "untrusted journal returned a trusted lifecycle envelope: " + label + "/" + operation)
        after = inventory(root)
        save_json(evidence / (operation + "-after.json"), after)
        require(after == original, "journal refusal modified observed original: " + label + "/" + operation)
    return evidence, original


def hot_journal_audit(recorder, pack=None, driver=None, test_name="fixture_run::tests::checkpoint_child"):
    """Known original real journals recover only on explicit selected-case resume."""
    pack = pack or recorder.fixture.parent
    rows = []
    for kind in ("control", "output"):
        label = "known-hot-" + kind
        root, database, original = journal_fixture(recorder, pack, driver, test_name, label, kind)
        writer = sqlite_writer(recorder, database, label)
        evidence, hot_original = journal_readonly_refusal(recorder, root, label)
        recorder.command(("run", "resume", recorder.root / "wrong-selected-hot-case", root), 3,
                         label="wrong case cannot trigger actual hot rollback: " + kind)
        require(inventory(root) == hot_original, "wrong case rolled back or invalidated original hot Run")
        outcome, _ = recorder.command(("run", "resume", recorder.fixture, root), label="explicit selected same-Run known hot rollback and full resume: " + kind)
        checked = verify_finished(recorder, pack, root, outcome, original["owner"]["run_id"])
        require(not Path(str(database) + "-journal").exists(), "known hot journal still remains after successful rollback/resume")
        for unit in original["state"]["units"]:
            if unit["state"] == "confirmed":
                require(checked["attempts"][unit["unit_id"]] == unit["attempts"], "hot recovery re-executed a confirmed unit")
        rows.append({"case": label, "status": "passed", "original_run_id": original["owner"]["run_id"],
                     "writer_receipt": writer, "readonly_and_wrong_case_preserved": True,
                     "confirmed_before": original["confirmations"], "confirmations_after": checked["confirmations"], "originals": str(evidence)})
        save_json(recorder.root / "hot-recovery-progress.json", rows)
    for kind in ("control", "output"):
        variants = ("cold", "truncated", "foreign-valid", "unknown-sibling", "shared-only-no-metadata") if kind == "control" else ("cold", "truncated", "foreign-valid", "unknown-sibling")
        for variant in variants:
            label = "refuse-journal-" + kind + "-" + variant
            root, database, original = journal_fixture(recorder, pack, driver, test_name, label, kind)
            writer = sqlite_writer(recorder, database, label, cold=variant == "cold", touch_metadata=variant != "shared-only-no-metadata")
            journal_path = Path(str(database) + "-journal")
            foreign_receipt = None
            if variant == "truncated":
                journal_path.write_bytes(journal_path.read_bytes()[:600])
            elif variant == "foreign-valid":
                foreign_root, foreign_database, _ = journal_fixture(recorder, pack, driver, test_name, label + "-foreign-source", kind)
                foreign_receipt = sqlite_writer(recorder, foreign_database, label + "-foreign-source")
                journal_path.write_bytes(Path(str(foreign_database) + "-journal").read_bytes())
                require(foreign_root != root and journal_path.read_bytes()[:8] == bytes.fromhex("d9d505f920a163d7"), "foreign journal is not another actual valid hot writer original")
            elif variant == "unknown-sibling":
                (database.parent / "manual-original.md").write_bytes(b"unknown original must survive\n")
            evidence, _ = journal_readonly_refusal(recorder, root, label, include_resume=True)
            rows.append({"case": label, "status": "passed", "writer_receipt": writer, "foreign_writer_receipt": foreign_receipt,
                         "preserved_originals": True, "originals": str(evidence)})
            save_json(recorder.root / "hot-recovery-progress.json", rows)
    label = "refuse-journal-output-reforged-snapshot"
    root, database, original = journal_fixture(recorder, pack, driver, test_name, label, "output")
    writer = sqlite_writer(recorder, database, label)
    bad_sha = sha(database.read_bytes())
    require(bad_sha != original["state"]["output_snapshot"]["database"]["sha256"], "actual writer did not change database bytes for forged snapshot test")
    mutate_checkpoint(root / "control/run.sqlite", lambda checkpoint: checkpoint["state"]["output_snapshot"]["database"].update(sha256=bad_sha, byte_length=database.stat().st_size))
    evidence, _ = journal_readonly_refusal(recorder, root, label, include_resume=True)
    rows.append({"case": label, "status": "passed", "writer_receipt": writer, "synchronised_checkpoint_sha": True,
                 "preserved_originals": True, "originals": str(evidence)})
    save_json(recorder.root / "hot-recovery-progress.json", rows)
    if driver is not None:
        for kind in ("control", "output"):
            label = "known-hot-exclusive-holder-" + kind
            root = recorder.root / label
            held = HeldDriver(recorder, driver, test_name, root, "unit_confirmed", "vault_readback", 1)
            try:
                original = read_journal(root, held.handshake["run_id"], recorder.fixture)
                database = root / ("control/run.sqlite" if kind == "control" else "output/state/compile.sqlite")
                writer = sqlite_writer(recorder, database, label)
                evidence, before = recorder.witness(label, root)
                outcome, _ = recorder.command(("run", "resume", recorder.fixture, root), 3,
                                             label="live exclusive holder forbids hot rollback: " + kind)
                require(outcome is None and held.process.poll() is None and inventory(root) == before,
                        "second worker rolled back or altered another live holder's hot journal")
                stopped = held.force_stop()
                outcome, _ = recorder.command(("run", "resume", recorder.fixture, root),
                                             label="known hot explicit recovery after actual exclusive holder termination: " + kind)
                checked = verify_finished(recorder, pack, root, outcome, original["owner"]["run_id"])
                require(all(checked["attempts"][unit["unit_id"]] == unit["attempts"]
                            for unit in original["state"]["units"] if unit["state"] == "confirmed"),
                        "exclusive hot recovery repeated previously confirmed work")
                rows.append({"case": label, "status": "passed", "writer_receipt": writer,
                             "live_holder_pid": stopped["pid"], "live_holder_force_terminated": True,
                             "preserved_while_held": True, "confirmed_retained": 4, "originals": str(evidence)})
                save_json(recorder.root / "hot-recovery-progress.json", rows)
            except Exception:
                if held.process.poll() is None:
                    held.force_stop()
                raise
    return rows


class Recorder:
    def __init__(self, root, cli, fixture):
        self.root = ordinary_path(root, leaf_missing=True)
        require(not self.root.exists(), "artifact root must be absent")
        self.cli = ordinary_path(cli)
        require(self.cli.is_file(), "CLI must be an ordinary selected file")
        self.fixture = ordinary_path(fixture)
        require(not self.root.is_relative_to(self.fixture) and not self.fixture.is_relative_to(self.root),
                "artifact root overlaps author package")
        self.author_before = author_check(self.fixture)
        self.cli_before = inventory(self.cli)
        self.root.mkdir()
        (self.root / "commands").mkdir()
        self.sentinel = self.root / "outside.md"
        self.sentinel.write_bytes("包外人工原件必须保持\n".encode("utf-8"))
        self.sentinel_before = inventory(self.sentinel)
        self.calls = []
        self.driver_calls = []
        self.auxiliary_calls = []
        self.source_before = selected_source_pins()
        save_json(self.root / "source-before.json", self.author_before)
        save_json(self.root / "cli-before.json", self.cli_before)
        save_json(self.root / "production-source-before.json", self.source_before)

    def witness(self, label, product_root):
        directory = self.root / "originals" / label
        directory.mkdir(parents=True)
        before = inventory(product_root)
        save_json(directory / "before.json", before)
        shutil.copytree(product_root, directory / "before-bytes", symlinks=True, copy_function=copy_observed_file)
        require(inventory(product_root) == before, "original archival read changed product")
        return directory, before

    def command(self, arguments, expected=0, diagnostic=None, label=None, timeout=30, cli=None):
        index = len(self.calls) + 1
        directory = self.root / "commands" / f"{index:03d}"
        directory.mkdir()
        selected_cli = ordinary_path(cli) if cli is not None else self.cli
        selected_before = inventory(selected_cli) if cli is not None else self.cli_before
        argv = [str(selected_cli), *map(str, arguments)]
        start_ns, start = time.time_ns(), time.monotonic()
        timed_out = False
        try:
            process = subprocess.run(argv, capture_output=True, check=False, timeout=timeout, env=clean_environment())
        except subprocess.TimeoutExpired as error:
            timed_out = True
            process = subprocess.CompletedProcess(argv, None, error.stdout or b"", error.stderr or b"")
        finished = time.monotonic()
        (directory / "stdout.bin").write_bytes(process.stdout)
        (directory / "stderr.bin").write_bytes(process.stderr)
        receipt = {"argv": argv, "label": label, "exit_code": process.returncode,
                   "started_ns": start_ns, "finished_ns": time.time_ns(), "elapsed_seconds": finished - start,
                   "timed_out": timed_out,
                   "stdout_sha256": sha(process.stdout), "stderr_sha256": sha(process.stderr),
                   "cli_sha256": selected_before["."]["sha256"]}
        save_json(directory / "receipt.json", receipt)
        self.calls.append(receipt)
        require(not timed_out, "actual CLI exceeded bounded subprocess timeout")
        require(process.returncode == expected, f"CLI exit {process.returncode}, expected {expected}: {argv[1:]}")
        if diagnostic is not None:
            require(diagnostic.encode("utf-8") in process.stderr, "missing actual CLI diagnostic: " + diagnostic)
        elif expected == 0:
            require(process.stderr == b"", "unexpected successful CLI stderr")
        require(inventory(self.fixture) == self.author_before, "ordinary CLI changed author source")
        require(inventory(self.cli) == self.cli_before, "selected CLI changed")
        if cli is not None:
            require(inventory(selected_cli) == selected_before, "explicit previous CLI changed")
        require(inventory(self.sentinel) == self.sentinel_before, "outside sentinel changed")
        return strict_json(process.stdout) if process.stdout else None, receipt

    def finish(self, result):
        author_after, cli_after = inventory(self.fixture), inventory(self.cli)
        save_json(self.root / "source-after.json", author_after)
        save_json(self.root / "cli-after.json", cli_after)
        source_after = selected_source_pins()
        save_json(self.root / "production-source-after.json", source_after)
        require(author_after == self.author_before and cli_after == self.cli_before, "selected originals changed")
        require(inventory(self.sentinel) == self.sentinel_before, "outside original changed")
        result.update(actual_cli_calls=len(self.calls), commands=self.calls, actual_driver_calls=len(self.driver_calls), drivers=self.driver_calls, real_model_calls=0,
                      actual_auxiliary_process_calls=len(self.auxiliary_calls), auxiliary_processes=self.auxiliary_calls,
                      author_pins=AUTHOR_PINS, cli_sha256=self.cli_before["."]["sha256"],
                      source_scope="workspace production Rust sources/manifests/build scripts, selected checker and its tests; separate actual build receipts required",
                      source_before_after_equal=source_after == self.source_before)
        save_json(self.root / "actual.json", result)
        return result


def old_cli_red(recorder):
    owned = recorder.root / "old-run"
    rows = []
    for arguments in (("start", recorder.fixture, owned), ("start", recorder.fixture, owned, "--paused"),
                      ("status", owned), ("pause", owned), ("resume", recorder.fixture, owned), ("cancel", owned)):
        before = inventory(recorder.root)
        _, receipt = recorder.command(("run", *arguments), 2, "CLI_INVALID_ARGUMENT", label="old missing run interface")
        require(not owned.exists(), "old CLI unexpectedly created a Run")
        rows.append({"operation": arguments[0], "exit_code": receipt["exit_code"], "root_created": False})
        # Command receipts are intentionally appended; no selected product root is modified.
        require(before["outside.md"] == inventory(recorder.root)["outside.md"], "RED changed original sentinel")
    return recorder.finish({"status": "observed_red", "producer": "independent_bounded_run_audit",
                            "reason": "ordinary old CLI has no Run start/status/pause/resume/cancel interface", "cases": rows})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=os.environ.get("CLI_BIN"), help="Selected CLI; alternatively CLI_BIN")
    parser.add_argument("--root", type=Path, required=True, help="Absent new artifact root with ordinary existing parent")
    parser.add_argument("--author-pack", type=Path, default=Path(__file__).resolve().parent.parent / "fixtures/run-checkpoint-v1")
    parser.add_argument("--fixture-sha256", help="Explicit frozen-sha256.json SHA256 anchor")
    parser.add_argument("--fixture", type=Path, help="Selected fixture; defaults to author-pack/fixture")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--old-cli-red", action="store_true", help="Preserve actual missing-interface failures of the old CLI")
    modes.add_argument("--public-only", action="store_true", help="Only ordinary lifecycle/refusal audit; no all-boundary interruption claim")
    parser.add_argument("--test-driver", type=Path, help="Explicit same-source Rust test binary with durable callback child")
    parser.add_argument("--test-name", default="fixture_run::tests::checkpoint_child")
    modes.add_argument("--active-controls-only", action="store_true", help="Actual held inflight pause/cancel and holder audit")
    modes.add_argument("--interruptions-only", action="store_true", help="Actual trace/each durable boundary kill and ordinary resume")
    modes.add_argument("--hot-journal-only", action="store_true", help="Actual hot recovery, cold/foreign refusal and exclusive-holder audit")
    modes.add_argument("--temporary-io-only", action="store_true", help="Actual temporary permission failure and ordinary same-Run recovery")
    modes.add_argument("--refusals-only", action="store_true", help="Strict unknown/owner/seed/ACK/input rejection submatrix")
    modes.add_argument("--publication-controls-only", action="store_true", help="Actual final publication COMMIT with late pause/cancel requests")
    parser.add_argument("--previous-cli", type=Path, help="Explicit earlier actual B07 binary for compiler-version incompatibility")
    args = parser.parse_args()
    parser.error("--cli or CLI_BIN is required") if args.cli is None else None
    args.root = canonical_selected_path(args.root, leaf_missing=True)
    args.author_pack = canonical_selected_path(args.author_pack)
    if args.fixture is not None:
        args.fixture = canonical_selected_path(args.fixture)
    fixture = args.fixture or args.author_pack / "fixture"
    if not args.old_cli_red:
        frozen_pack_check(args.author_pack, args.fixture_sha256)
    recorder = Recorder(args.root, args.cli, fixture)
    try:
        if args.old_cli_red:
            result = old_cli_red(recorder)
        elif args.hot_journal_only:
            result = recorder.finish({"status": "passed", "producer": "independent_bounded_run_audit", "scope": "selected original hot rollback and original-preserving journal refusal submatrix only",
                                      "hot_journal_cases": hot_journal_audit(recorder, args.author_pack, args.test_driver, args.test_name)})
        elif args.refusals_only:
            result = recorder.finish({"status": "passed", "producer": "independent_bounded_run_audit", "scope": "strict original-preserving refusal submatrix only",
                                      "refusal_cases": refusal_audit(recorder)})
        elif args.publication_controls_only:
            require(args.test_driver is not None, "Publication control audit requires explicit same-source --test-driver")
            result = recorder.finish({"status": "passed", "producer": "independent_bounded_run_audit", "scope": "actual inflight publication late control submatrix only",
                                      "publication_control_cases": publication_control_audit(recorder, args.author_pack, args.test_driver, args.test_name),
                                      "publication_control_interruption_cases": publication_control_interruption_audit(recorder, args.author_pack, args.test_driver, args.test_name)})
        elif args.public_only:
            result = public_audit(recorder, args.author_pack)
            result["refusal_cases"] = refusal_audit(recorder)
            result["hot_journal_cases"] = hot_journal_audit(recorder, args.author_pack, args.test_driver, args.test_name)
            result["compiler_version_cases"] = compiler_version_audit(recorder, args.author_pack, args.previous_cli)
            result = recorder.finish(result)
        else:
            require(args.test_driver is not None, "Full/active audit requires explicit same-source --test-driver")
            if args.active_controls_only or args.interruptions_only or args.temporary_io_only:
                baseline_root = recorder.root / "fresh-old-compile"
                outcome, _ = recorder.command(("case", "compile", recorder.fixture, baseline_root), label="active-control ordinary independent baseline")
                checked = verify_published_output(baseline_root, recorder.fixture, outcome)
                baseline = {"files": checked["files"], "normalized_report": checked["normalized_report"]}
                result = {"status": "passed", "producer": "independent_bounded_run_audit", "scope": "selected bounded diagnostic submatrix only"}
            else:
                result = public_audit(recorder, args.author_pack)
                result["refusal_cases"] = refusal_audit(recorder)
                baseline = result["cases"][0]
            if not args.interruptions_only and not args.temporary_io_only:
                result["active_controls"] = active_control_audit(recorder, args.author_pack, args.test_driver, args.test_name, baseline)
                result["concurrency_cases"] = concurrent_audit(recorder, args.author_pack, args.test_driver, args.test_name, baseline)
                result["publication_control_cases"] = publication_control_audit(recorder, args.author_pack, args.test_driver, args.test_name)
                result["publication_control_interruption_cases"] = publication_control_interruption_audit(recorder, args.author_pack, args.test_driver, args.test_name)
            if not args.interruptions_only:
                result["temporary_io_cases"] = temporary_io_audit(recorder, args.author_pack, args.test_driver, args.test_name, baseline)
            if not args.active_controls_only and not args.temporary_io_only:
                if not args.interruptions_only:
                    result["hot_journal_cases"] = hot_journal_audit(recorder, args.author_pack, args.test_driver, args.test_name)
                    result["compiler_version_cases"] = compiler_version_audit(recorder, args.author_pack, args.previous_cli)
                result["interruption_cases"] = interruption_audit(recorder, args.author_pack, args.test_driver, args.test_name, baseline)
                result["publication_visibility_cases"] = publication_visibility_audit(recorder, args.author_pack, args.test_driver, args.test_name)
                result["partial_refusal_cases"] = partial_refusal_audit(recorder, args.author_pack, args.test_driver, args.test_name)
                result["scope"] = "ordinary CLI lifecycle/refusal, actual active control/holder, traced durable callbacks individually killed then ordinary CLI resumed"
            result = recorder.finish(result)
    except Exception as error:
        recorder.finish({"status": "failed", "producer": "independent_bounded_run_audit", "error": str(error), "error_type": type(error).__name__})
        raise
    print(json.dumps({"status": result["status"], "actual_cli_calls": result["actual_cli_calls"],
                      "actual_driver_calls": result["actual_driver_calls"], "actual": str(recorder.root / "actual.json"),
                      "actual_auxiliary_process_calls": result["actual_auxiliary_process_calls"],
                      "cli_sha256": result["cli_sha256"], "source_before_after_equal": result["source_before_after_equal"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
