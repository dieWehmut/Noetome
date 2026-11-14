"""Prepare the public, revision-pinned MinerU models inside an owned runtime.

Only setup downloads. The actual page worker is offline and never installs assets.
MinerU's downloader creates and validates its own completion markers; this script
additionally checks the downloaded bytes against the frozen upstream revisions.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
from urllib.request import urlopen


PINS = (
    ("MinerU-4_models_onnx", "opendatalab/MinerU-4_models_onnx", "358310b4f64b95f9fefc372ad899356e4111f376"),
    ("MinerU2.5-Pro-2605-1.2B-GGUF", "jinzhenj/MinerU2.5-Pro-2605-1.2B-GGUF", "9185688a0495e1577d521a757c7c0b62dd38ca48"),
)


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for data in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def environment(root: Path) -> None:
    config = root / "mineru-home" / "config.yaml"
    config.parent.mkdir(parents=True, exist_ok=True)
    if not config.exists():
        config.write_text("{}\n", encoding="utf-8")
    os.environ.update({
        "MINERU_HOME": str(root / "mineru-home"),
        "MINERU_CONFIG": str(root / "mineru-home" / "config.yaml"),
        "MINERU_MODEL_BASE_DIR": str(root / "models"),
        "MINERU_MODEL_SMALL_BACKEND": "onnx",
        "MINERU_MODEL_VLM_ENGINE": "llama-cpp",
        "MINERU_MODEL_SOURCE": "huggingface",
        "HF_HOME": str(root / "huggingface-cache"),
        "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1",
        "HF_HUB_DISABLE_TELEMETRY": "1",
        "HF_HUB_DISABLE_XET": "1",
        "HF_HUB_DOWNLOAD_TIMEOUT": "120",
    })


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    root = args.runtime.resolve()
    owner = json.loads((root / "setup-owner.json").read_text(encoding="utf-8-sig"))
    if owner.get("producer") != "noetome-corpus-ocr-setup-v1":
        raise ValueError("Runtime is not owned by the Noetome OCR setup")
    environment(root)
    provenance = root / "model-provenance"
    provenance.mkdir(exist_ok=True)
    from mineru.model.download import download_model_repo, verify_model_repo
    from mineru.model.registry import get_model_repo

    receipts = []
    for name, repo_id, revision in PINS:
        print(f"MODEL {name} revision {revision}", flush=True)
        with urlopen(f"https://huggingface.co/api/models/{repo_id}/revision/{revision}?blobs=true", timeout=45) as response:
            metadata = json.load(response)
        if metadata["sha"] != revision:
            raise ValueError(f"Upstream revision changed for {repo_id}; review a new pin before downloading")
        (provenance / f"{name}.upstream.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        repo = get_model_repo(name)
        target = download_model_repo(repo, source="huggingface", reuse_ready=True)
        verified = verify_model_repo(repo)
        if not verified.ready:
            raise ValueError(f"MinerU verification failed: {verified}")
        files = []
        for entry in metadata["siblings"]:
            relative = entry["rfilename"]
            path = target / relative
            if not path.is_file():
                # The required-path bundle omits documentation; preserve it here.
                if relative in ("README.md", "LICENSE", "LICENSE.md", "manifest.json"):
                    with urlopen(f"https://huggingface.co/{repo_id}/resolve/{revision}/{relative}", timeout=45) as response:
                        raw = response.read()
                    path = provenance / f"{name}.{relative}"
                    path.write_bytes(raw)
                else:
                    continue
            actual = digest(path)
            expected = (entry.get("lfs") or {}).get("sha256")
            if expected and actual != expected:
                raise ValueError(f"Model SHA256 differs from frozen revision: {repo_id}/{relative}")
            if path.stat().st_size != entry["size"]:
                raise ValueError(f"Model size differs from frozen revision: {repo_id}/{relative}")
            files.append({"path": str(path.relative_to(root)).replace("\\", "/"), "size": path.stat().st_size, "sha256": actual, "upstream_lfs_sha256": expected})
        receipts.append({"name": name, "repository": repo_id, "revision": revision, "card_license": (metadata.get("cardData") or {}).get("license"), "files": files})
        print(f"VERIFIED {name}: {len(files)} files", flush=True)
    result = {"schema_version": 1, "producer": "noetome-corpus-ocr-models", "models_ready": True, "inference_verified": False, "models": receipts, "license_notes": ["MinerU code: Apache-2.0 with additional terms; preserve its LICENSE.md.", "ONNX bundle keeps upstream licenses; preserve source manifest and model cards.", "GGUF conversion repository has no independent license metadata; record Apache-2.0 upstream VLM provenance separately before distribution."]}
    (root / "model-receipt.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("MODEL PREPARATION COMPLETE; inference remains unverified", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
