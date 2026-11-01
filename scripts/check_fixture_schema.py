"""Check the authored fixture with an optional, independent JSON Schema validator.

Install jsonschema into a separate tools directory, then pass --module-dir.
This checks structure and selected negative mutations; case verify checks semantics.
"""

import argparse
import copy
import hashlib
import importlib.metadata
import json
from pathlib import Path
import sys


PACKAGES = {
    "case.json": "Manifest",
    "ir.json": "KnowledgeIr",
    "note-plan.json": "PlanPackage",
    "expected.json": "ExpectedPackage",
}


def main():
    repo = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--module-dir", type=Path)
    parser.add_argument("--case", type=Path, default=repo / "fixtures/text-compare-v1")
    parser.add_argument("--schema", type=Path, default=repo / "schemas/fixture-v1.json")
    args = parser.parse_args()
    if args.module_dir:
        sys.path.insert(0, str(args.module_dir.resolve()))
    from jsonschema import Draft202012Validator

    schema_bytes = args.schema.read_bytes()
    schema = json.loads(schema_bytes)
    Draft202012Validator.check_schema(schema)
    validators = {
        name: Draft202012Validator(
            {"$schema": schema["$schema"], "$defs": schema["$defs"], "$ref": f"#/$defs/{kind}"}
        )
        for name, kind in PACKAGES.items()
    }
    packages = {name: json.loads((args.case / name).read_bytes()) for name in PACKAGES}
    positive = []
    negative = []
    for name, package in packages.items():
        validators[name].validate(package)
        # Also exercise the published root oneOf, rather than only the selected definition.
        Draft202012Validator(schema).validate(package)
        positive.append(name)
        for label, change in [
            ("future-version", lambda value: value.update(schema_version=2)),
            ("non-fixture-producer", lambda value: value.update(producer="model")),
            ("unknown-field", lambda value: value.update(undeclared=True)),
            ("missing-producer", lambda value: value.pop("producer")),
        ]:
            altered = copy.deepcopy(package)
            change(altered)
            if validators[name].is_valid(altered):
                raise AssertionError(f"{name}: {label} unexpectedly accepted")
            negative.append(f"{name}:{label}")

    def reject(name, label, path, value=None, remove=False):
        altered = copy.deepcopy(packages[name])
        target = altered
        for part in path[:-1]:
            target = target[part]
        if remove:
            del target[path[-1]]
        else:
            target[path[-1]] = value
        if validators[name].is_valid(altered):
            raise AssertionError(f"{name}: {label} unexpectedly accepted")
        negative.append(f"{name}:{label}")

    for path in ["../ir.json", "/ir.json", "C:/ir.json", "CON.json", "sub\\ir.json", "ir.txt"]:
        reject("case.json", f"invalid-path-{path}", ["ir_path"], path)
    reject("ir.json", "negative-revision", ["revision"], -1)
    reject("ir.json", "invalid-id", ["atoms", 0, "atom_id"], "bad id")
    reject("ir.json", "invalid-sha", ["evidence", 0, "excerpt_sha256"], "0" * 63)
    reject("ir.json", "missing-nullable", ["coverage", 0, "exclusion_reason"], remove=True)
    reject("note-plan.json", "unknown-node-kind", ["notes", 0, "sections", 0, "ordered_nodes", 0, "kind"], "html")
    reject("note-plan.json", "invalid-output-suffix", ["notes", 0, "output_path"], "note.txt")
    reject("expected.json", "missing-evidence", ["evidence"], remove=True)
    reject("expected.json", "missing-canonical-atoms", ["canonical_placements", 0, "atom_ids"], remove=True)
    reject("expected.json", "empty-canonical-atoms", ["canonical_placements", 0, "atom_ids"], [])
    print(json.dumps({
        "validator": "jsonschema",
        "validator_version": importlib.metadata.version("jsonschema"),
        "draft": "2020-12",
        "schema_sha256": hashlib.sha256(schema_bytes).hexdigest(),
        "schema_valid": True,
        "accepted_packages": positive,
        "rejected_mutations": negative,
        "semantic_validation": "not_run_by_this_tool",
    }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
