"""Offline, one physical page OCR adapter for the pinned native MinerU runtime.

The host owns original SHA verification, output directory ownership, timeouts,
cancellation and retries. This process only opens the PDF by path and renders
one page; MinerU receives that PNG and never the whole original book.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict
import importlib.metadata
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import sys
import time


ADAPTER_VERSION = "noetome-corpus-ocr-v1"


def helper(name):
    spec = importlib.util.spec_from_file_location("noetome_ocr_" + name,
                                               Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def configure(root):
    os.environ.update({
        "MINERU_HOME": str(root / "mineru-home"),
        "MINERU_CONFIG": str(root / "mineru-home" / "config.yaml"),
        "MINERU_MODEL_BASE_DIR": str(root / "models"),
        "MINERU_MODEL_SOURCE": "local",
        "MINERU_MODEL_SMALL_BACKEND": "onnx",
        "MINERU_MODEL_ONNX_DEVICE": "gpu",
        "MINERU_MODEL_VLM_ENGINE": "llama-cpp",
        "MINERU_MODEL_VLM_SERVER_URL": "",
        "MINERU_LLAMA_CPP_BACKEND": "vulkan",
        "HF_HUB_OFFLINE": "1", "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1",
        "HF_HUB_DISABLE_TELEMETRY": "1",
    })
    if sys.platform != "win32" or sys.version.split()[0] != "3.12.14":
        raise ValueError("The locked worker requires native Windows CPython 3.12.14")
    if importlib.metadata.version("mineru") != "4.2.1":
        raise ValueError("The worker requires MinerU 4.2.1")
    owner = json.loads((root / "setup-owner.json").read_text(encoding="utf-8-sig"))
    if owner.get("producer") != "noetome-corpus-ocr-setup-v1":
        raise ValueError("Runtime owner differs")
    receipt_path = root / "model-receipt.json"
    receipt = json.loads(receipt_path.read_text(encoding="utf-8-sig"))
    if receipt.get("models_ready") is not True:
        raise ValueError("Revision-pinned models have not been prepared")
    # Check file sizes at page start; installation records and verifies full
    # upstream SHA256 once. The immutable runtime profile binds that receipt.
    for model in receipt["models"]:
        for item in model["files"]:
            candidate = root / item["path"]
            if candidate.stat().st_size != item["size"]:
                raise ValueError("Prepared model file changed size")
    return receipt, helper("render").sha256(receipt_path)


def content_text(value):
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        return "".join(content_text(item) for item in value)
    if isinstance(value, dict):
        return content_text(value.get("content", ""))
    return ""


def normalize(middle, geometry, raw_root, output):
    pages = middle.get("pages", [])
    if len(pages) != 1 or pages[0].get("page_idx") != 0:
        raise ValueError("MinerU must return exactly image page_idx 0")
    regions, issues = [], {}
    width, height = geometry["width_pt"], geometry["height_pt"]

    def visit(block, parent=None):
        bounds = block.get("bbox") or parent
        if bounds is None:
            raise ValueError("OCR block has no source region")
        if len(bounds) != 4 or any(type(n) not in (int, float) or
                                   not math.isfinite(n) or not 0 <= n <= 1 for n in bounds):
            raise ValueError("MinerU normalized bbox is invalid")
        kind = block.get("type", "unknown")
        children = block.get("blocks", block.get("children", []))
        if kind in ("list", "index", "image", "table", "chart", "code"):
            children = block.get("content", [])
        if children:
            for child in children:
                visit(child, bounds)
            return
        text = content_text(block.get("content", ""))
        region = {"ordinal": len(regions), "kind": kind, "text": text,
                  "bbox": [bounds[0] * width, bounds[1] * height,
                           bounds[2] * width, bounds[3] * height]}
        if block.get("bbox") is None:
            region["bbox_scope"] = "parent_block"
            issues["OCR_PARENT_BBOX"] = "A nested block uses its parent region; a finer location was not provided by MinerU."
        if bounds[2] <= bounds[0] or bounds[3] <= bounds[1]:
            raise ValueError("OCR bbox has no area")
        image_path = block.get("image_path")
        if image_path:
            asset = raw_root / image_path
            if not asset.resolve().is_relative_to(raw_root.resolve()) or not asset.is_file():
                raise ValueError("OCR asset is absent or outside the raw output")
            region["assets"] = [{"path": str(asset.relative_to(output)).replace("\\", "/"),
                                 "sha256": helper("render").sha256(asset)}]
        # Do not invent a calibrated confidence from language-model output.
        if "confidence" in block and type(block["confidence"]) in (int, float):
            region["confidence"] = block["confidence"]
        regions.append(region)
        spans = block.get("content", [])
        spans = spans if isinstance(spans, list) else []
        if kind in ("table_body", "equation", "image_body", "chart_body") or any(
                isinstance(span, dict) and span.get("type") == "equation_inline"
                for span in spans):
            issues["OCR_VISUAL_REVIEW"] = "Table, formula or figure semantics require independent review; raw structures and original page are retained."
        if "\ufffd" in text:
            issues["OCR_TEXT_ENCODING"] = "OCR contains an undecoded replacement character."

    for block in pages[0].get("blocks", []):
        visit(block)
    text = "\n".join(region["text"] for region in regions if region["text"])
    if not text.strip():
        issues["OCR_EMPTY_RESULT"] = "OCR returned no readable text; the original page remains available for review."
    return text, regions, [{"code": code, "message": message} for code, message in sorted(issues.items())]


def run(request, output, runtime):
    render = helper("render")
    receipt, receipt_sha = configure(runtime)
    number = request["page_number"]
    source_sha = request["source_sha256"]
    if not isinstance(source_sha, str) or not re.fullmatch(r"[0-9a-f]{64}", source_sha):
        raise ValueError("Host-verified original source_sha256 is required")
    source = Path(request["pdf_path"])
    geometry = request.get("geometry")
    before = source.stat()
    started = time.perf_counter()
    raster = render.render_page(source, number, output / "page.png",
                                request.get("render_dpi", 144), geometry)
    geometry = raster["geometry"]
    raster["source_sha256"] = source_sha
    (output / "render.json").write_text(json.dumps(raster, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    from mineru import MinerUParser
    from mineru.parser.writer import FileBasedDataWriter
    from mineru.model.runtime import onnx as ort_factory
    sessions = []
    original_factory = ort_factory.create_ort_session

    def observed_factory(*args, **kwargs):
        session = original_factory(*args, **kwargs)
        sessions.append({"model": Path(args[0]).name, "model_kind": kwargs.get("model_kind", ""),
                         "providers": session.get_providers(), "provider_options": session.get_provider_options()})
        return session

    ort_factory.create_ort_session = observed_factory
    parser = MinerUParser(tier="standard", parse_mode="ocr", image_analysis=True)
    parsed = parser.parse(output / "page.png")
    raw_root = output / "raw"
    parsed.save(FileBasedDataWriter(str(raw_root)))
    middle = json.loads((raw_root / "middle_json.json").read_text(encoding="utf-8"))
    text, regions, issues = normalize(middle, geometry, raw_root, output)
    after = source.stat()
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise ValueError("Original source changed during OCR")
    from mineru.model.runtime.device_state import cached_runtime_device
    device = cached_runtime_device()
    identity = {"engine": "mineru", "engine_version": "4.2.1",
                "adapter_version": ADAPTER_VERSION, "python_version": "3.12.14",
                "tier": "standard", "small_backend": "onnx", "vlm_engine": "llama-cpp",
                "llama_cpp_backend_requested": "vulkan", "render_dpi": raster["render_dpi"],
                "model_receipt_sha256": receipt_sha,
                "models": [{"repository": m["repository"], "revision": m["revision"]} for m in receipt["models"]],
                "onnx_sessions": sessions, "runtime_device": asdict(device) if device else None,
                "scientific_validation": "not_verified"}
    return {"schema_version": 1, "page_number": number, "source_sha256": source_sha,
            "width_pt": geometry["width_pt"], "height_pt": geometry["height_pt"],
            "rotation": geometry["rotation"], "geometry": geometry,
            "status": "needs_review" if issues else "ocr_ready", "text": text,
            "regions": regions, "issues": issues, "parser": identity,
            "assets": [{"path": "page.png", "sha256": raster["render_sha256"]}],
            "raw_output": "raw/middle_json.json", "elapsed_seconds": time.perf_counter() - started}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    request = json.loads(args.request.read_text(encoding="utf-8-sig"))
    args.output.mkdir(parents=True, exist_ok=False)
    try:
        result = run(request, args.output, args.runtime.resolve())
        code = 0
    except Exception as exc:
        import traceback
        traceback.print_exc()
        geometry = request.get("geometry") or {}
        result = {"schema_version": 1, "page_number": request.get("page_number"),
                  "source_sha256": request.get("source_sha256"),
                  "width_pt": geometry.get("width_pt"), "height_pt": geometry.get("height_pt"),
                  "rotation": geometry.get("rotation"), "geometry": geometry or None,
                  "status": "failed", "text": "", "regions": [],
                  "issues": [{"code": "OCR_PAGE_FAILED", "message": str(exc)[:500]}],
                  "parser": {"engine": "mineru", "engine_version": "4.2.1", "adapter_version": ADAPTER_VERSION}}
        code = 1
    (args.output / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2,
                                                       allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps({"status": result["status"], "page_number": result["page_number"]}), flush=True)
    return code


if __name__ == "__main__":
    raise SystemExit(main())
