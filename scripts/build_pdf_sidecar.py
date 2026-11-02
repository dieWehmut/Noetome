"""Build/check a byte-bound embedded Python payload for native Windows argv.

The payload is trusted repository code, never document-controlled data.
No parser dependencies are imported here; only Python's standard library.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zlib


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    directory = Path(__file__).resolve().parents[1] / "sidecars/pdf_parser"
    source = (directory / "adapter.py").read_bytes()
    payload_path = directory / "adapter.zlib"
    metadata_path = directory / "adapter-payload.json"
    if args.check:
        payload = payload_path.read_bytes()
        metadata = json.loads(metadata_path.read_text(encoding="utf8"))
        assert zlib.decompress(payload) == source, "embedded PDF adapter differs from source; rebuild payload"
        assert metadata == {
            "schema_version": 1, "encoding": "zlib-hex-exec-v1",
            "source_sha256": hashlib.sha256(source).hexdigest(),
            "payload_sha256": hashlib.sha256(payload).hexdigest(),
            "source_bytes": len(source), "payload_bytes": len(payload),
        }, "PDF payload metadata/hash differs; rebuild payload"
        print(f"PDF payload byte binding verified: source {len(source)} bytes, payload {len(payload)} bytes.")
        return
    payload = zlib.compress(source, level=9)
    assert zlib.decompress(payload) == source
    metadata = {
        "schema_version": 1, "encoding": "zlib-hex-exec-v1",
        "source_sha256": hashlib.sha256(source).hexdigest(),
        "payload_sha256": hashlib.sha256(payload).hexdigest(),
        "source_bytes": len(source), "payload_bytes": len(payload),
    }
    payload_path.write_bytes(payload)
    metadata_path.write_text(json.dumps(metadata, indent=2)+"\n", encoding="utf8", newline="\n")
    print(f"Built PDF payload: {len(source)} source bytes -> {len(payload)} payload bytes; native code argument ~{len(payload)*2+120} chars.")


if __name__ == "__main__":
    main()
