"""Read installed package bytes; pair the metadata with an actual app capture."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path


def pin(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("installed_asar", type=Path)
parser.add_argument("new_output", type=Path)
args = parser.parse_args()
body = args.installed_asar.read_bytes()
header = struct.unpack("<IIII", body[:16])
index = json.loads(body[16:16 + header[3]])


def member(name):
    entry = index
    for part in name.split("/"):
        entry = entry["files"][part]
    start = 8 + header[1] + int(entry["offset"])
    data = body[start:start + entry["size"]]
    fingerprint = pin(data)
    assert len(data) == entry["size"]
    assert fingerprint["sha256"] == entry["integrity"]["hash"]
    return data, {"member": name, **fingerprint, "asar_integrity_matches": True}


package, package_pin = member("package.json")
mathjax, mathjax_pin = member("lib/mathjax/tex-chtml-full.js")
text = mathjax.decode("utf-8")
match = re.search(r'\.VERSION="(\d+\.\d+\.\d+)"', text)
assert match
auxiliary = re.search(r'mathjaxVersion="(\d+\.\d+\.\d+)"', text)
result = {
    "scope": "read-only installed package metadata; actual runtime and rendering separately required",
    "installed_asar": str(args.installed_asar),
    "asar_fingerprint": pin(body),
    "obsidian_package_version": json.loads(package)["version"],
    "package": package_pin,
    "mathjax": mathjax_pin,
    "bundled_mathjax_version": match.group(1),
    "literal": match.group(0),
    "literal_character_offset": match.start(),
    "auxiliary_module_mathjax_version": auxiliary.group(1) if auxiliary else None,
    "version_method": "core VERSION export; auxiliary mathjaxVersion literal is separately recorded and does not identify the runtime engine",
}
with args.new_output.open("x", encoding="utf-8") as stream:
    json.dump(result, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
print(json.dumps({"status": "passed", "obsidian": result["obsidian_package_version"], "mathjax": match.group(1)}))
