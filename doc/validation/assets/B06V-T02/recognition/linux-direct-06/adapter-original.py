"""One selected PNG through pinned local pix2tex; Core owns process cancellation."""
from __future__ import annotations

import contextlib
import hashlib
import importlib.metadata
import io
import json
import os
from pathlib import Path
import random
import re
import socket
import stat
import sys
import time

PYTHON_VERSION = "3.12.14"
PACKAGE_PINS = {
    "pix2tex": "0.1.4", "torch": "2.5.1+cpu", "torchvision": "0.20.1+cpu",
    "numpy": "1.26.4", "albumentations": "1.3.1",
    "opencv-python-headless": "4.10.0.84", "pandas": "2.2.3",
    "transformers": "4.49.0", "timm": "0.5.4", "x-transformers": "0.15.0",
    "tokenizers": "0.21.4", "Pillow": "13.0.0",
}
MODEL_PINS = {
    "weights.pth": {"sha256": "a63d9141c53d266cb682fb5a8bd83bd5cbe283145e0e78ebdc0f895195a1dfaa", "byte_length": 102113875},
    "image_resizer.pth": {"sha256": "1c3820659985ad142b526490bb25c23d977176ac2073591b3bddada692718458", "byte_length": 19441973},
    "config.yaml": {"sha256": "a9b18acf6718ffed79ab669c9957c46936714f4b983705f0b1361d90f4ea89aa", "byte_length": 917},
    "tokenizer.json": {"sha256": "1dc27b18d6a518d0d5ff3f4bb7bd98521fe80ad39e5b2a246d4109f1bb9d5019", "byte_length": 24174},
}
POLICY = {"device": "cpu", "seed": 42, "temperature": 0.2, "resize": True,
          "max_seq_len": 512, "network_calls": 0, "paid_calls": 0,
          "automatic_downloads": False, "clipboard": False, "decoding": "greedy_argmax_v1", "preprocessing": "resizer_aspect_detail_guard_v1", "max_downsample": 2.0}
MAX_REQUEST = 65536
MAX_INPUT = 8 * 1024 * 1024
STARTED = time.monotonic()


class Failure(Exception):
    def __init__(self, code, message):
        self.code, self.message = code, message
        super().__init__(message)


def require(ok, code, message):
    if not ok:
        raise Failure(code, message)


def exact(value, fields):
    require(type(value) is dict and set(value) == set(fields),
            "SCAN_REQUEST", "Unknown or missing request fields.")
    return value


def unique(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "SCAN_REQUEST", "Duplicate JSON field.")
        value[key] = item
    return value


def number(value, maximum):
    require(type(value) is int and 0 < value <= maximum,
            "SCAN_REQUEST", "Invalid positive resource limit.")


def path(value):
    require(type(value) is str and 0 < len(value) <= 32768 and "\0" not in value,
            "SCAN_REQUEST", "Invalid selected path.")
    p = Path(value)
    require(p.is_absolute() and ".." not in p.parts,
            "SCAN_REQUEST", "Selected paths must be absolute and have no traversal.")
    return p


def ordinary(p, directory=False):
    for current in [*reversed(p.parents), p]:
        info = current.lstat()
        require(not stat.S_ISLNK(info.st_mode)
                and not getattr(info, "st_file_attributes", 0) & 0x400,
                "SCAN_RESOURCE", "Selected resource has a link or reparse ancestor.")
        if current != p or directory:
            require(stat.S_ISDIR(info.st_mode), "SCAN_RESOURCE", "Expected an ordinary directory.")
        else:
            require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
                    "SCAN_RESOURCE", "Expected an ordinary file with one link.")


def read(p, maximum):
    ordinary(p)
    before = p.lstat()
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    with os.fdopen(os.open(p, flags), "rb") as handle:
        actual = os.fstat(handle.fileno())
        require(stat.S_ISREG(actual.st_mode) and actual.st_nlink == 1
                and (before.st_dev, before.st_ino) == (actual.st_dev, actual.st_ino),
                "SCAN_RESOURCE", "Selected file changed before reading.")
        data = handle.read(maximum + 1)
        after = os.fstat(handle.fileno())
    require(len(data) <= maximum, "SCAN_RESOURCE_LIMIT", "Selected resource exceeds its limit.")
    final = p.lstat()
    require((actual.st_dev, actual.st_ino, actual.st_size, actual.st_mtime_ns)
            == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
            == (final.st_dev, final.st_ino, final.st_size, final.st_mtime_ns),
            "SCAN_RESOURCE", "Selected file changed while reading.")
    return data


def fingerprint(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def packages():
    result = {}
    for name in PACKAGE_PINS:
        try:
            result[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            result[name] = "missing"
    return result


def model_files(directory):
    ordinary(directory, directory=True)
    require({p.name for p in directory.iterdir()} == set(MODEL_PINS),
            "SCAN_MODEL", "Model directory must contain exactly the four pinned files.")
    result = {name: fingerprint(read(directory / name, pin["byte_length"]))
              for name, pin in MODEL_PINS.items()}
    require(result == MODEL_PINS, "SCAN_MODEL", "Model bytes do not match the selected release.")
    return result


def request():
    data = sys.stdin.buffer.read(MAX_REQUEST + 1)
    require(len(data) <= MAX_REQUEST, "SCAN_REQUEST_LIMIT", "Request exceeds its limit.")
    value = json.loads(data.decode("utf-8", "strict"), object_pairs_hook=unique,
                       parse_constant=lambda _: (_ for _ in ()).throw(Failure("SCAN_REQUEST", "Non-finite JSON number.")))
    exact(value, {"schema_version", "request_id", "input", "model_directory", "limits"})
    require(type(value["schema_version"]) is int and value["schema_version"] == 1,
            "SCAN_REQUEST", "Unsupported request version.")
    require(type(value["request_id"]) is str and 0 < len(value["request_id"]) <= 128
            and re.fullmatch(r"[A-Za-z0-9_-]+", value["request_id"]),
            "SCAN_REQUEST", "Invalid request identity.")
    source = exact(value["input"], {"path", "sha256", "byte_length"})
    path(source["path"])
    number(source["byte_length"], MAX_INPUT)
    require(type(source["sha256"]) is str and re.fullmatch(r"[0-9a-f]{64}", source["sha256"]),
            "SCAN_REQUEST", "Invalid input fingerprint.")
    path(value["model_directory"])
    limits = exact(value["limits"], {"response_bytes", "diagnostic_bytes", "timeout_ms", "max_tex_bytes"})
    for name, maximum in {"response_bytes": 131072, "diagnostic_bytes": 65536,
                          "timeout_ms": 120000, "max_tex_bytes": 65536}.items():
        number(limits[name], maximum)
    return value


def deny_network(*args, **kwargs):
    raise Failure("SCAN_NETWORK_DISABLED", "Network and automatic model downloads are disabled.")


def predict(data, directory):
    os.environ.update({"HF_HUB_OFFLINE": "1", "TRANSFORMERS_OFFLINE": "1",
                       "HF_HUB_DISABLE_TELEMETRY": "1", "HF_HUB_DISABLE_XET": "1",
                       "TOKENIZERS_PARALLELISM": "false"})
    socket.socket.connect = deny_network
    socket.socket.connect_ex = deny_network
    socket.create_connection = deny_network
    with contextlib.redirect_stdout(sys.stderr):
        import numpy as np
        import torch
        from PIL import Image
        from munch import Munch
        import pix2tex.cli as cli

        cli.download_checkpoints = deny_network
        cli.clipboard.copy = lambda _: None
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        random.seed(POLICY["seed"])
        np.random.seed(POLICY["seed"])
        torch.manual_seed(POLICY["seed"])
        torch.use_deterministic_algorithms(True)
        real_load = torch.load
        allowed = {str(directory / name) for name in ("weights.pth", "image_resizer.pth")}

        def load_weights(selected, *args, **kwargs):
            require(str(selected) in allowed, "SCAN_MODEL", "Unexpected model weight selection.")
            kwargs["weights_only"] = True
            return real_load(selected, *args, **kwargs)

        torch.load = load_weights
        image = Image.open(io.BytesIO(data))
        require(image.format == "PNG" and getattr(image, "n_frames", 1) == 1
                and all(0 < axis <= 2048 for axis in image.size),
                "SCAN_INPUT", "Expected one bounded selected PNG region.")
        image.load()
        Image.MAX_IMAGE_PIXELS = 2048 * 2048
        arguments = Munch({"config": str(directory / "config.yaml"),
                           "checkpoint": str(directory / "weights.pth"),
                           "tokenizer": str(directory / "tokenizer.json"),
                           "no_cuda": True, "no_resize": False, "temperature": 0.2,
                           "max_seq_len": 512})
        engine = cli.LatexOCR(arguments)
        import base64
        source_dimensions = list(image.size)
        initial = cli.minmax_size(cli.pad(image), engine.args.max_dimensions, engine.args.min_dimensions).convert("RGB")
        original_width, original_height = initial.size
        target_width = original_width
        minimum_width = max(1, (original_width + 1) // 2)
        steps = []
        with torch.no_grad():
            for _ in range(10):
                target_height = max(1, round(original_height * target_width / original_width))
                method = Image.Resampling.BILINEAR if target_width > original_width else Image.Resampling.LANCZOS
                resized = initial.resize((target_width, target_height), method)
                prepared = cli.pad(cli.minmax_size(resized, engine.args.max_dimensions, engine.args.min_dimensions))
                pixels = np.array(prepared.convert("RGB"))
                tensor = cli.test_transform(image=pixels)["image"][:1].unsqueeze(0)
                predicted_width = (engine.image_resizer(tensor).argmax(-1).item()+1)*32
                clamped_width = max(predicted_width, minimum_width)
                steps.append({"resize_target": [target_width,target_height], "actual_pad": list(prepared.size), "predicted_width": predicted_width, "clamped_width": clamped_width, "pixels_sha256": hashlib.sha256(pixels.tobytes()).hexdigest()})
                if clamped_width == prepared.width:
                    break
                target_width = clamped_width
            real_sample = torch.multinomial
            greedy_calls = [0]
            def argmax_one(probs, num_samples, *args, **kwargs):
                require(num_samples == 1 and probs.ndim == 2, "SCAN_DECODING", "Unexpected sampling shape.")
                greedy_calls[0] += 1
                return probs.argmax(dim=-1, keepdim=True)
            torch.multinomial = argmax_one
            try:
                dec = engine.model.generate(tensor, temperature=0.2)
            finally:
                torch.multinomial = real_sample
            result = cli.post_process(cli.token2str(dec, engine.tokenizer)[0])
        buffer = io.BytesIO()
        prepared.save(buffer, format="PNG", compress_level=9)
        processed = buffer.getvalue()
        sys.stderr.write(json.dumps({"diagnostic": "actual_inference_pipeline", "source_dimensions": source_dimensions, "initial_pad": [original_width, original_height], "steps": steps, "greedy_calls": greedy_calls[0], "final_tensor_shape": list(tensor.shape), "final_tensor_sha256": hashlib.sha256(tensor.numpy().tobytes()).hexdigest(), "final_png_sha256": hashlib.sha256(processed).hexdigest(), "final_png_base64": base64.b64encode(processed).decode("ascii")}) + "\n")
        return result


def main():
    value = None
    response = None
    try:
        value = request()
        runtime = {"python": ".".join(map(str, sys.version_info[:3])), "packages": packages()}
        response = {"schema_version": 1, "producer": "pix2tex", "package_version": "0.1.4",
                    "request_id": value["request_id"],
                    "input": {k: value["input"][k] for k in ("sha256", "byte_length")},
                    "model": {"release": "v0.0.1", "files": {}},
                    "recognized_tex": None, "issues": [], "elapsed_ms": 0,
                    "status": "failed", "confidence": None, "runtime": runtime, "policy": POLICY}
        require(runtime == {"python": PYTHON_VERSION, "packages": PACKAGE_PINS},
                "SCAN_RUNTIME", "Python or inference package versions do not match the selected runtime.")
        directory = path(value["model_directory"])
        response["model"]["files"] = model_files(directory)
        selected = path(value["input"]["path"])
        data = read(selected, MAX_INPUT)
        require(fingerprint(data) == response["input"], "SCAN_INPUT", "Input bytes do not match the selected region.")
        recognized = predict(data, directory)
        require(type(recognized) is str and recognized.strip() and "\0" not in recognized,
                "SCAN_EMPTY", "Model returned no usable expression.")
        require(len(recognized.encode("utf-8", "strict")) <= value["limits"]["max_tex_bytes"],
                "SCAN_TEX_LIMIT", "Model expression exceeds its limit.")
        require(model_files(directory) == response["model"]["files"]
                and fingerprint(read(selected, MAX_INPUT)) == response["input"],
                "SCAN_RESOURCE", "Selected source or model changed during inference.")
        require((time.monotonic() - STARTED) * 1000 <= value["limits"]["timeout_ms"],
                "SCAN_TIMEOUT", "Inference exceeded its time limit.")
        response.update(recognized_tex=recognized, status="recognized")
    except Exception as exc:
        code, message = (exc.code, exc.message) if isinstance(exc, Failure) else ("SCAN_ADAPTER", "Local model or request could not be processed.")
        if response is None:
            sys.stderr.write(code + ": " + message + "\n")
            return 2
        response.update(recognized_tex=None, status="failed", issues=[{"code": code, "message": message}])
    response["elapsed_ms"] = int((time.monotonic() - STARTED) * 1000)
    payload = (json.dumps(response, ensure_ascii=False, allow_nan=False, separators=(",", ":")) + "\n").encode("utf-8")
    if len(payload) > value["limits"]["response_bytes"]:
        sys.stderr.write("SCAN_RESPONSE_LIMIT: Response exceeds its limit.\n")
        return 2
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
