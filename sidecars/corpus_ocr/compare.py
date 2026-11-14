"""Capture two pinned OCR engines against the same frozen original-page PNGs.

This is an offline validation harness, not the production page scheduler. Each
engine loads once, records every attempted page, and retains raw structures.
Content review is deliberately separate from successful process completion.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import traceback


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2,
                                    allow_nan=False) + "\n", encoding="utf-8")
    temporary.replace(path)


def load_adapter():
    path = Path(__file__).with_name("worker.py")
    spec = importlib.util.spec_from_file_location("noetome_compare_adapter", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class GpuSamples:
    def __init__(self, output):
        self.output = output
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.sample, daemon=True)
        self.samples = []

    def sample(self):
        query = ["nvidia-smi", "--query-gpu=timestamp,name,memory.total,memory.used,utilization.gpu",
                 "--format=csv,noheader,nounits"]
        while not self.stop.is_set():
            try:
                captured = subprocess.run(query, capture_output=True, text=True,
                                          timeout=10, creationflags=0x08000000)
                item = {"elapsed_seconds": time.perf_counter() - self.started,
                        "exit_code": captured.returncode, "csv": captured.stdout.strip()}
            except Exception as exc:
                item = {"elapsed_seconds": time.perf_counter() - self.started,
                        "error": str(exc)}
            self.samples.append(item)
            with self.output.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(item, ensure_ascii=False) + "\n")
            self.stop.wait(2)

    def __enter__(self):
        self.started = time.perf_counter()
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.thread.join(timeout=12)


def mineru_engine(runtime):
    adapter = load_adapter()
    receipt, receipt_sha = adapter.configure(runtime)
    from mineru import MinerUParser
    from mineru.parser.writer import FileBasedDataWriter
    from mineru.model.runtime import onnx as ort_factory
    sessions = []
    original = ort_factory.create_ort_session

    def observe(*args, **kwargs):
        session = original(*args, **kwargs)
        sessions.append({"model": Path(args[0]).name,
                         "model_kind": kwargs.get("model_kind", ""),
                         "providers": session.get_providers()})
        return session

    ort_factory.create_ort_session = observe
    parser = MinerUParser(tier="standard", parse_mode="ocr", image_analysis=True)

    def page(image, record, output):
        parsed = parser.parse(image)
        raw = output / "raw"
        parsed.save(FileBasedDataWriter(str(raw)))
        middle = json.loads((raw / "middle_json.json").read_text(encoding="utf-8"))
        geometry = {"width_pt": record["size_pt"][0],
                    "height_pt": record["size_pt"][1]}
        text, regions, issues = adapter.normalize(middle, geometry, raw, output)
        from mineru.model.runtime.device_state import cached_runtime_device
        device = cached_runtime_device()
        return {"text": text, "regions": regions, "issues": issues,
                "status": "needs_review" if issues else "ocr_ready",
                "parser": {"engine": "mineru", "engine_version": "4.2.1",
                           "model_receipt_sha256": receipt_sha,
                           "models": [{"repository": m["repository"], "revision": m["revision"]}
                                      for m in receipt["models"]],
                           "onnx_sessions": sessions,
                           "runtime_device": asdict(device) if device else None}}

    return page, {"engine": "mineru", "engine_version": "4.2.1"}


def paddle_engine(runtime):
    os.environ.update({"PADDLE_PDX_CACHE_HOME": str(runtime / "model-cache"),
                       "HF_HOME": str(runtime / "huggingface-cache"),
                       "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1", "HF_HUB_DISABLE_TELEMETRY": "1"})
    if importlib.metadata.version("paddleocr") != "3.7.0":
        raise ValueError("PaddleOCR version differs from 3.7.0")
    import paddle
    from paddleocr import PaddleOCRVL
    from paddlex.inference import load_pipeline_config
    if paddle.__version__ != "3.2.1" or not paddle.is_compiled_with_cuda():
        raise ValueError("The native Paddle 3.2.1 CUDA runtime is required")
    configuration = load_pipeline_config("PaddleOCR-VL-1.6")
    # The default recognizer batch_size=-1 groups all detected blocks. The
    # dense-page comparison saturated this 12 GiB GPU, so bind an explicit
    # one-block inference batch without reducing the decoder token ceiling.
    configuration["SubModules"]["VLRecognition"]["batch_size"] = 1
    pipeline = PaddleOCRVL(pipeline_version="v1.6", device="gpu:0", paddlex_config=configuration,
                          use_doc_orientation_classify=False, use_doc_unwarping=False,
                          use_layout_detection=True, use_chart_recognition=True,
                          use_ocr_for_image_block=True, markdown_ignore_labels=[],
                          use_queues=False)

    def page(image, record, output):
        results = list(pipeline.predict(str(image)))
        if len(results) != 1:
            raise ValueError("Paddle must return exactly one image page")
        results[0].save_to_json(save_path=str(output))
        results[0].save_to_markdown(save_path=str(output))
        path = output / (image.stem + "_res.json")
        data = json.loads(path.read_text(encoding="utf-8"))
        blocks = data.get("parsing_res_list", [])
        regions = []
        for block in blocks:
            bounds = block["block_bbox"]
            if len(bounds) != 4:
                raise ValueError("Paddle block lacks a rectangle")
            regions.append({"kind": block["block_label"],
                            "text": block["block_content"],
                            "bbox": [bounds[0] * record["size_pt"][0] / record["width_px"],
                                     bounds[1] * record["size_pt"][1] / record["height_px"],
                                     bounds[2] * record["size_pt"][0] / record["width_px"],
                                     bounds[3] * record["size_pt"][1] / record["height_px"]]})
        return {"text": "\n".join(r["text"] for r in regions), "regions": regions,
                "issues": [{"code": "OCR_REVIEW_REQUIRED",
                            "message": "Text and visual semantics require independent source review."}],
                "status": "needs_review",
                "parser": {"engine": "paddleocr", "engine_version": "3.7.0",
                           "paddle_version": "3.2.1", "pipeline_version": "v1.6",
                           "vl_recognition_batch_size": 1,
                           "device": paddle.device.get_device(),
                           "gpu_name": paddle.device.cuda.get_device_name()}}

    return page, {"engine": "paddleocr", "engine_version": "3.7.0", "pipeline_version": "v1.6",
                  "vl_recognition_batch_size": 1, "decoder_token_ceiling": "upstream_default_4096"}


def paddle_structure_engine(runtime, device="gpu:0"):
    """Official PP-StructureV3 with explicit small OCR/formula models."""
    os.environ.update({"PADDLE_PDX_CACHE_HOME": str(runtime / "model-cache"),
                       "HF_HOME": str(runtime / "huggingface-cache"),
                       "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1", "HF_HUB_DISABLE_TELEMETRY": "1"})
    if importlib.metadata.version("paddleocr") != "3.7.0":
        raise ValueError("PaddleOCR version differs from 3.7.0")
    import paddle
    from paddleocr import PPStructureV3
    from paddlex.inference import load_pipeline_config
    if paddle.__version__ != "3.2.1" or not paddle.is_compiled_with_cuda():
        raise ValueError("The native Paddle 3.2.1 CUDA runtime is required")
    configuration = load_pipeline_config("PP-StructureV3")
    def single_batches(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key == "batch_size":
                    value[key] = 1
                else:
                    single_batches(child)
    single_batches(configuration)
    general = configuration["SubPipelines"]["GeneralOCR"]
    general["SubModules"]["TextDetection"]["model_name"] = "PP-OCRv5_mobile_det"
    general["SubModules"]["TextRecognition"]["model_name"] = "PP-OCRv5_mobile_rec"
    configuration["SubPipelines"]["FormulaRecognition"]["SubModules"]["FormulaRecognition"]["model_name"] = "PP-FormulaNet_plus-S"
    pipeline = PPStructureV3(device=device, paddlex_config=configuration,
                             use_doc_orientation_classify=False, use_doc_unwarping=False,
                             use_textline_orientation=False, use_seal_recognition=False,
                             use_table_recognition=True, use_formula_recognition=True,
                             use_chart_recognition=False, use_region_detection=True,
                             markdown_ignore_labels=[], use_queues=False)
    configuration_sha = hashlib.sha256(json.dumps(configuration, sort_keys=True).encode()).hexdigest()
    identity = {"engine": "paddleocr", "engine_version": "3.7.0", "paddle_version": "3.2.1",
                "pipeline_version": "PP-StructureV3", "configuration_sha256": configuration_sha,
                "text_detection_model": "PP-OCRv5_mobile_det", "text_recognition_model": "PP-OCRv5_mobile_rec",
                "formula_model": "PP-FormulaNet_plus-S", "all_model_batches": 1,
                "table_recognition": True, "formula_recognition": True, "chart_recognition": False}
    def page(image, record, output):
        results = list(pipeline.predict(str(image)))
        if len(results) != 1:
            raise ValueError("PP-StructureV3 must return exactly one image page")
        results[0].save_to_json(save_path=str(output))
        results[0].save_to_markdown(save_path=str(output))
        data = json.loads((output / (image.stem + "_res.json")).read_text(encoding="utf-8"))
        regions = []
        for block in data.get("parsing_res_list", []):
            bounds = block["block_bbox"]
            if len(bounds) != 4:
                raise ValueError("PP-StructureV3 block lacks a rectangle")
            regions.append({"kind": block["block_label"], "text": block["block_content"],
                            "bbox": [bounds[0] * record["size_pt"][0] / record["width_px"],
                                     bounds[1] * record["size_pt"][1] / record["height_px"],
                                     bounds[2] * record["size_pt"][0] / record["width_px"],
                                     bounds[3] * record["size_pt"][1] / record["height_px"]]})
        runtime_identity = dict(identity, device=paddle.device.get_device())
        if device.startswith("gpu"):
            runtime_identity["gpu_name"] = paddle.device.cuda.get_device_name()
        return {"text": "\n".join(region["text"] for region in regions), "regions": regions,
                "issues": [{"code": "OCR_REVIEW_REQUIRED", "message": "Text, tables, formulas and figure relations require source review."}],
                "status": "needs_review", "parser": runtime_identity}
    return page, identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", choices=["mineru", "paddleocr"], required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--page-timeout", type=int, default=600)
    parser.add_argument("--paddle-pipeline", choices=["vl16", "ppstructurev3"], default="vl16")
    args = parser.parse_args()
    if sys.platform != "win32" or sys.version.split()[0] != "3.12.14":
        raise ValueError("The comparison requires native Windows CPython 3.12.14")
    if not 60 <= args.page_timeout <= 1800:
        raise ValueError("page-timeout must be between 60 and 1800 seconds")
    args.output.mkdir(parents=True, exist_ok=args.resume)
    manifest_path = args.input / "render-receipt.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    report = {"schema_version": 1, "engine": args.engine,
              "input_receipt_sha256": digest(manifest_path), "expected_pages": len(manifest["items"]),
              "scientific_validation": "not_verified", "items": []}
    report_path = args.output / "report.json"
    if args.resume and report_path.exists():
        previous = json.loads(report_path.read_text(encoding="utf-8"))
        if any(previous[k] != report[k] for k in ("engine", "input_receipt_sha256", "expected_pages")):
            raise ValueError("Existing comparison belongs to different inputs or an engine")
        report = previous
        if report.get("paddle_pipeline", "vl16") != args.paddle_pipeline:
            raise ValueError("Existing comparison belongs to a different Paddle pipeline")
    if args.engine == "paddleocr":
        report["paddle_pipeline"] = args.paddle_pipeline
    completed = {item["case_id"] for item in report["items"]}
    known_cases = {item["case_id"] for item in manifest["items"]}
    if len(completed) != len(report["items"]) or not completed <= known_cases:
        raise ValueError("Existing comparison contains duplicate or unknown page cases")
    for item in report["items"]:
        result_path = args.output / item["case_id"] / "normalized.json"
        if not result_path.is_file() or digest(result_path) != item["result_sha256"]:
            raise ValueError("Previously captured OCR page changed; resume refused")
    for record in manifest["items"]:
        if digest(args.input / (record["case_id"] + ".png")) != record["image_sha256"]:
            raise ValueError("Frozen original page PNG changed")
    report["page_timeout_seconds"] = args.page_timeout
    report.setdefault("harness_versions_sha256", []).append(digest(Path(__file__)))
    started = time.perf_counter()
    with GpuSamples(args.output / "gpu-samples.jsonl"):
        selected_engine = mineru_engine if args.engine == "mineru" else (paddle_structure_engine if args.paddle_pipeline == "ppstructurev3" else paddle_engine)
        page_engine, identity = selected_engine(args.runtime)
        report["identity"] = identity
        construction = time.perf_counter() - started
        report.setdefault("construction_attempts_seconds", [report["construct_seconds"]] if "construct_seconds" in report else []).append(construction)
        report["construct_seconds"] = report["construction_attempts_seconds"][0]
        # The process supervisor needs a durable report even if the first page
        # never returns. Model construction is separate from page confirmation.
        report["total_seconds"] = sum(report["construction_attempts_seconds"]) + sum(r["elapsed_seconds"] for r in report["items"])
        write_json(report_path, report)
        for record in manifest["items"]:
            image = args.input / (record["case_id"] + ".png")
            if digest(image) != record["image_sha256"]:
                raise ValueError("Frozen original page PNG changed")
            if record["case_id"] in completed:
                continue
            output = args.output / record["case_id"]
            output.mkdir(exist_ok=args.resume)
            begin = time.perf_counter()
            result = dict(record)
            write_json(args.output / "in-progress.json", {"case_id": record["case_id"], "pid": os.getpid(),
                       "started_unix_seconds": time.time(), "timeout_seconds": args.page_timeout})

            def timed_out(record=record, output=output, begin=begin):
                failure = dict(record, status="failed", text="", regions=[],
                               issues=[{"code": "OCR_COMPARE_PAGE_TIMEOUT", "message": "Page exceeded its explicit inference deadline."}],
                               elapsed_seconds=time.perf_counter() - begin)
                write_json(output / "normalized.json", failure)
                report["items"].append({"case_id": record["case_id"], "status": "failed",
                    "elapsed_seconds": failure["elapsed_seconds"], "characters": 0, "regions": 0,
                    "issue_codes": ["OCR_COMPARE_PAGE_TIMEOUT"], "result_sha256": digest(output / "normalized.json")})
                report["total_seconds"] = sum(report["construction_attempts_seconds"]) + sum(r["elapsed_seconds"] for r in report["items"])
                write_json(report_path, report)
                # A blocked native inference cannot be safely interrupted from
                # a Python thread. Exit this owned worker; --resume keeps every
                # captured page and continues with the next unconfirmed case.
                os._exit(124)

            watchdog = threading.Timer(args.page_timeout, timed_out)
            watchdog.daemon = True
            watchdog.start()
            try:
                if digest(image) != record["image_sha256"]:
                    raise ValueError("Frozen original page PNG changed")
                result.update(page_engine(image, record, output))
            except Exception as exc:
                traceback.print_exc()
                result.update(status="failed", text="", regions=[],
                              issues=[{"code": "OCR_COMPARE_PAGE_FAILED", "message": str(exc)[:500]}])
            finally:
                watchdog.cancel()
            result["elapsed_seconds"] = time.perf_counter() - begin
            write_json(output / "normalized.json", result)
            report["items"].append({"case_id": record["case_id"], "status": result["status"],
                                    "elapsed_seconds": result["elapsed_seconds"],
                                    "characters": len(result["text"]), "regions": len(result["regions"]),
                                    "issue_codes": [i["code"] for i in result["issues"]],
                                    "result_sha256": digest(output / "normalized.json")})
            report["total_seconds"] = sum(report["construction_attempts_seconds"]) + sum(r["elapsed_seconds"] for r in report["items"])
            write_json(args.output / "report.json", report)
            print(json.dumps(report["items"][-1]), flush=True)
    report["completed_pages"] = len(report["items"])
    (args.output / "in-progress.json").unlink(missing_ok=True)
    write_json(args.output / "report.json", report)
    return 0 if all(r["status"] != "failed" for r in report["items"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
