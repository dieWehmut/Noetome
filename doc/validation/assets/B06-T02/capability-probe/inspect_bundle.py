"""Read only the installed Obsidian ASAR header and bundled Mermaid metadata."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("installed_obsidian_asar", type=Path)
parser.add_argument("new_output", type=Path)
args = parser.parse_args()
body = args.installed_obsidian_asar.read_bytes()
header = struct.unpack("<IIII", body[:16])
index = json.loads(body[16:16 + header[3]])


def member(name):
    entry = index
    for part in name.split("/"):
        entry = entry["files"][part]
    start = 8 + header[1] + int(entry["offset"])
    data = body[start:start + entry["size"]]
    sha = hashlib.sha256(data).hexdigest()
    assert len(data) == entry["size"] and sha == entry["integrity"]["hash"]
    return data, {"member": name, "sha256": sha, "byte_length": len(data), "asar_member_integrity_matches": True}


mermaid, mermaid_pin = member("lib/mermaid.min.js")
package, package_pin = member("package.json")
text = mermaid.decode("utf-8")
source_line = text.splitlines()[0]
version = re.search(r"mermaid@(\d+\.\d+\.\d+)", source_line).group(1)
literal = re.search(r'const [A-Za-z0-9_$]+="' + re.escape(version) + r'"', text)
assert literal is not None
result = {
    "scope": "Installed package metadata; runtime global does not expose .version. Pair with actual clean-app capture, not a remote library lookup.",
    "asar_path": str(args.installed_obsidian_asar),
    "asar_sha256": hashlib.sha256(body).hexdigest(), "asar_byte_length": len(body),
    "bundled_mermaid_version": version, "source_header": source_line,
    "internal_version_literal": literal.group(0), "literal_character_offset": literal.start(),
    "mermaid_member": mermaid_pin,
    "obsidian_package_version": json.loads(package)["version"], "package_member": package_pin,
}
with args.new_output.open("x", encoding="utf-8") as stream:
    json.dump(result, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
print(version, mermaid_pin["sha256"])
