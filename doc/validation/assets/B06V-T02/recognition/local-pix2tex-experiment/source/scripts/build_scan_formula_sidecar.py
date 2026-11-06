"""Build/check the trusted, byte-bound compressed scan adapter for native argv."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zlib


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    directory = Path(__file__).resolve().parents[1] / "sidecars/scan_formula"
    source = (directory / "adapter.py").read_bytes()
    compressed = zlib.compress(source, level=9)
    source_sha = hashlib.sha256(source).hexdigest()
    wrapper = (
        "import hashlib,zlib\n"
        f"_noetome_scan_source=zlib.decompress(bytes.fromhex({compressed.hex()!r}))\n"
        f"assert len(_noetome_scan_source)=={len(source)}\n"
        f"assert hashlib.sha256(_noetome_scan_source).hexdigest()=={source_sha!r}\n"
        "exec(compile(_noetome_scan_source,'<noetome-scan-formula-v1>','exec'))\n"
    ).encode("utf-8")
    metadata = {
        "schema_version": 1, "encoding": "zlib-hex-source-assert-exec-v1",
        "source_sha256": source_sha, "source_bytes": len(source),
        "compressed_sha256": hashlib.sha256(compressed).hexdigest(),
        "compressed_bytes": len(compressed),
        "wrapper_sha256": hashlib.sha256(wrapper).hexdigest(), "wrapper_bytes": len(wrapper),
    }
    wrapper_path = directory / "adapter_payload.py"
    metadata_path = directory / "adapter-payload.json"
    assert zlib.decompress(compressed) == source
    if args.check:
        assert wrapper_path.read_bytes() == wrapper, "scan payload differs from source; rebuild"
        assert json.loads(metadata_path.read_text(encoding="utf-8")) == metadata, "scan payload fingerprint differs"
        print(f"Scan payload byte binding verified: {len(source)} source bytes, {len(wrapper)} argv bytes.")
    else:
        wrapper_path.write_bytes(wrapper)
        metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(f"Built scan payload: {len(source)} source bytes, {len(wrapper)} argv bytes.")


if __name__ == "__main__":
    main()
