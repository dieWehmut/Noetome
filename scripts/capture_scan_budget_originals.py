"""Capture actual scan helper originals for the independent budget checker.

This utility reads one explicitly selected, completed helper invocation. It
never launches the CLI, reads credentials, sends a request, or changes a ledger.
The new archive is an immutable byte capture; its printed manifest SHA is the
anchor to retain separately and pass to verify_scan_formula.py.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_scan_formula as V


SELECTED_BINARY_SHA256 = "91fe246b522f8261da8ad678f465021bf76660ee402ad35e3a6bb4c4c2762be7"
PHASES = ("before", "reserved", "launch", "settled")
RECEIPT_FIELDS = (
    "schema_version kind invocation_id phase recorded_at usage_ledger_reference "
    "ledger_snapshot selected_binary selected_config case_directory profile "
    "output_root reservation launch result settlement"
)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


class SourcePaths:
    """Map only recorded explicit paths; no directory discovery is performed."""

    def __init__(self, mappings=()):
        self.mappings = []
        for recorded, local in mappings:
            source = V.absolute_saved_path(recorded)
            target = Path(local).absolute()
            V.absolute_saved_path(str(target))
            V.guard_path(target, directory=True)
            self.mappings.append((source, target))

    def local(self, recorded):
        source = V.absolute_saved_path(recorded)
        matches = []
        for prefix, target in self.mappings:
            if type(source) is not type(prefix):
                continue
            try:
                suffix = source.relative_to(prefix)
            except ValueError:
                continue
            matches.append((len(prefix.parts), target.joinpath(*suffix.parts)))
        if matches:
            longest = max(length for length, _ in matches)
            paths = {path for length, path in matches if length == longest}
            assert len(paths) == 1, "ambiguous explicit source mapping"
            return paths.pop()
        assert not isinstance(source, PureWindowsPath) or os.name == "nt", "recorded Windows source requires --path-map on this host"
        return Path(recorded)


def checked_read(path, maximum):
    before = V.file_pin(path)
    assert before["byte_length"] <= maximum, "bounded selected original"
    with Path(path).open("rb") as original:
        raw = original.read(before["byte_length"] + 1)
    assert len(raw) <= before["byte_length"], "original exceeds fixed byte length"
    V.exact(V.fingerprint(raw), before, "original unchanged while reading")
    V.exact(V.file_pin(path), before, "original unchanged after reading")
    return raw, before


def copy_unchanged(source, destination, expected):
    """Copy held original bytes once, and reject source replacement or mutation."""
    V.exact(V.file_pin(source), expected, "original unchanged before copy")
    before = Path(source).stat()
    state = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns)
    digest, count = hashlib.sha256(), 0
    with Path(source).open("rb") as origin, destination.open("xb") as saved:
        V.exact(state(os.fstat(origin.fileno())), state(before), "same selected source identity")
        while chunk := origin.read(min(1024 * 1024, expected["byte_length"] - count + 1)):
            assert count + len(chunk) <= expected["byte_length"], "original exceeds fixed byte length before copy"
            saved.write(chunk)
            digest.update(chunk)
            count += len(chunk)
        V.exact(state(os.fstat(origin.fileno())), state(before), "source identity unchanged during copy")
    pin = dict(sha256=digest.hexdigest(), byte_length=count)
    V.exact(pin, expected, "copied actual original bytes")
    V.exact(V.file_pin(source), expected, "original unchanged after copy")
    V.exact(V.file_pin(destination), expected, "saved original copy pin")
    return dict(file=str(destination), **pin)


def capture(settled_receipt, output, mappings=(), expected_binary_sha256=SELECTED_BINARY_SHA256):
    paths = SourcePaths(mappings)
    output = Path(output).absolute()
    V.absolute_saved_path(str(output))
    V.guard_path(output.parent, directory=True)
    assert not output.exists(), "capture output must be a brand-new directory"
    raw, settled_pin = checked_read(settled_receipt, 8 * 1024 * 1024)
    settled = V.strict_json(raw)
    V.fields(settled, RECEIPT_FIELDS)
    V.uuid_string(settled["invocation_id"])
    V.exact(settled["phase"], "settled", "selected completed helper phase")
    uid = settled["invocation_id"]
    usage = settled["usage_ledger_reference"]
    V.absolute_saved_path(usage, "tangmao-deepseek.usage.json")
    prefix = usage + ".scan-" + uid
    expected_settled = paths.local(prefix + ".settled.json").absolute()
    V.exact(Path(settled_receipt).absolute(), expected_settled, "explicit selected original settled receipt path")
    phases, receipt_sources, originals = {}, {}, {}

    def add(recorded, maximum=8 * 1024 * 1024, expected=None):
        local = paths.local(recorded)
        pin = V.file_pin(local)
        assert pin["byte_length"] <= maximum, "bounded actual original"
        if expected is not None:
            V.exact(pin, expected, "original reference fingerprint")
        if recorded in originals:
            V.exact(originals[recorded], (local, pin), "consistent repeated original reference")
        else:
            originals[recorded] = (local, pin)
        return local

    def reference(ref, maximum=8 * 1024 * 1024):
        V.fields(ref, "path sha256 byte_length")
        return add(ref["path"], maximum, {key: ref[key] for key in ("sha256", "byte_length")})

    for phase in PHASES:
        recorded_receipt = prefix + "." + phase + ".json"
        selected = paths.local(recorded_receipt)
        phase_raw, phase_pin = checked_read(selected, 8 * 1024 * 1024)
        receipt = V.strict_json(phase_raw)
        V.fields(receipt, RECEIPT_FIELDS)
        for field, value in (("schema_version", 1), ("kind", "scan_api_credential_invocation"), ("invocation_id", uid), ("phase", phase), ("usage_ledger_reference", usage)):
            V.exact(receipt[field], value, "actual helper phase " + field)
        V.fields(receipt["ledger_snapshot"], "path sha256 byte_length")
        V.same_saved_path(receipt["ledger_snapshot"]["path"], prefix + "." + phase + "-ledger.json")
        ledger_path = reference(receipt["ledger_snapshot"], 1024 * 1024)
        V.ledger_schema(V.strict_json(checked_read(ledger_path, 1024 * 1024)[0]))
        reference(receipt["selected_binary"], 512 * 1024 * 1024)
        V.exact(receipt["selected_binary"]["sha256"], expected_binary_sha256, "selected reviewed binary SHA")
        config_path = reference(receipt["selected_config"], 65536)
        V.api_config_schema(V.strict_json(checked_read(config_path, 65536)[0]))
        profile_path = add(receipt["profile"], 65536)
        V.profile_schema(V.strict_json(checked_read(profile_path, 65536)[0]))
        phases[phase] = receipt
        receipt_sources[phase] = (selected, phase_pin)
    V.exact(receipt_sources["settled"][1], settled_pin, "selected settled receipt unchanged")
    first = phases["before"]
    for phase in PHASES:
        for field in ("usage_ledger_reference", "selected_binary", "selected_config", "case_directory", "profile", "output_root", "reservation"):
            V.exact(phases[phase][field], first[field], "same actual phase input " + field)
    result = phases["settled"]["result"]
    V.fields(result, "cli_exit_code stdout stderr")
    for stream in ("stdout", "stderr"):
        entry = result[stream]
        V.fields(entry, "original_sha256 original_byte_length omission_reason saved_copy")
        V.same_saved_path(entry["saved_copy"]["path"], prefix + "." + stream + ".txt")
        reference(entry["saved_copy"], 16 * 1024 * 1024 if stream == "stdout" else 1024 * 1024)
    assert result["stdout"]["omission_reason"] is None, "redacted stdout cannot establish complete scan report"
    report = V.strict_json(checked_read(paths.local(result["stdout"]["saved_copy"]["path"]), 8 * 1024 * 1024)[0])
    V.fields(report, V.REPORT_FIELDS)
    output_path = V.absolute_saved_path(first["output_root"])
    request_recorded = str(output_path / "request.json")
    add(request_recorded)

    # All original references and JSON gates above are checked before creating
    # any destination. A failed copy remains reviewable and is never overwritten.
    output.mkdir()
    copies_directory = output / "copies"
    receipts_directory = output / "receipts"
    copies_directory.mkdir()
    receipts_directory.mkdir()
    copies = {}
    for index, (recorded, (local, pin)) in enumerate(sorted(originals.items())):
        destination = copies_directory / (f"{index:02d}-" + V.absolute_saved_path(recorded).name)
        copies[recorded] = copy_unchanged(local, destination, pin)
    receipts = {}
    for phase, (local, pin) in receipt_sources.items():
        receipts[phase] = copy_unchanged(local, receipts_directory / (phase + ".json"), pin)
    for local, pin in [*originals.values(), *receipt_sources.values()]:
        V.exact(V.file_pin(local), pin, "all selected originals unchanged at capture completion")
    binding = dict(schema_version=1, producer="independent_scan_budget_originals", invocation_id=uid, receipts=receipts, copies=copies)
    binding_raw = encoded(binding)
    anchor = V.fingerprint(binding_raw)["sha256"]
    checked = V.verify_budget_binding(report, binding_raw, anchor)
    binding_path = output / "budget-binding.json"
    with binding_path.open("xb") as saved:
        saved.write(binding_raw)
    invocation = dict(command=[first["selected_binary"]["path"], "knowledge", "scan-formula", first["case_directory"], first["selected_config"]["path"], first["profile"], first["output_root"]], exit_code=result["cli_exit_code"], producer="derived_from_actual_scan_helper_phases", invocation_id=uid)
    invocation_path = output / "invocation-receipt.json"
    with invocation_path.open("xb") as saved:
        saved.write(encoded(invocation))
    summary = dict(schema_version=1, producer="scan_budget_original_capture", captured_at=datetime.now(timezone.utc).isoformat(), invocation_id=uid, budget_binding=str(binding_path), budget_binding_sha256=anchor, invocation_receipt=str(invocation_path), source_originals_unchanged=True, budget_check=checked, selected_binary_sha256=expected_binary_sha256, helper_receipts=4, copied_originals=len(copies), capture_script=dict(path=str(Path(__file__).absolute()), **V.file_pin(__file__)), budget_checker=dict(path=str(Path(V.__file__).absolute()), **V.file_pin(V.__file__)), network_calls_from_capture=0, credential_reads=0, ledger_writes=0, scientific_quality="not_run")
    with (output / "capture-receipt.json").open("xb") as saved:
        saved.write(encoded(summary))
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--settled-receipt", required=True, help="explicit original settled helper receipt on this host")
    parser.add_argument("--output", required=True, help="brand-new independent originals archive")
    parser.add_argument("--path-map", nargs=2, action="append", default=[], metavar=("RECORDED_ROOT", "LOCAL_ROOT"), help="explicit source mapping, e.g. Windows D:/ to /mnt/d")
    parser.add_argument("--expected-binary-sha256", default=SELECTED_BINARY_SHA256, help="reviewed selected PE SHA; override only for an explicitly different reviewed binary")
    args = parser.parse_args()
    try:
        V.fp(dict(sha256=args.expected_binary_sha256, byte_length=0))
        summary = capture(args.settled_receipt, args.output, args.path_map, args.expected_binary_sha256)
    except (AssertionError, OSError, ValueError, UnicodeError, KeyError, TypeError) as error:
        print(json.dumps(dict(status="rejected", reason=str(error)), ensure_ascii=False))
        return 2
    print(json.dumps(dict(status="captured", **summary), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
