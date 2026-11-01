"""Bounded single-request pdfplumber adapter for the frozen Noetome PDF v2.
The host owns and verifies the workspace. No reports, projects, OCR or downloads.
"""
from __future__ import annotations
import hashlib
import importlib.metadata
import io
import json
import logging
import math
import os
from pathlib import Path
import platform
import re
import stat
import sys
import uuid

PROTOCOL = "noetome-pdf-single-v2"
OPTIONS = "pdfplumber-glyph-lines-v1"
RUNTIME = {"python": "3.12.14", "pdfplumber": "0.11.9", "pdfminer": "20251230", "pypdfium2": "5.14.0", "pillow": "12.3.0", "cryptography": "50.0.2", "cffi": "2.1.1", "pycparser": "3.11", "charset_normalizer": "3.5.2"}
DISTRIBUTIONS = {"pdfplumber": "pdfplumber", "pdfminer": "pdfminer.six", "pypdfium2": "pypdfium2", "pillow": "Pillow", "cryptography": "cryptography", "cffi": "cffi", "pycparser": "pycparser", "charset_normalizer": "charset-normalizer"}
MAX_REQUEST_BYTES = 65536
MAX_INPUT_BYTES = 67108864
MAX_STDOUT_BYTES = 4194304
MAX_ARTIFACT = {"max_file_bytes": 16777216, "max_total_bytes": 67108864, "max_files": 64}
MAX_PDF = {"max_pages": 63, "max_glyphs": 32768, "max_regions": 4096, "max_objects": 4096}
FORMULA_CHARACTERS = frozenset("=^_∑∫√±²³×÷≤≥≠")

class AdapterError(Exception):
    def __init__(self, code, message, exit_code=3):
        super().__init__(message)
        self.code, self.exit_code = code, exit_code

def exact_object(value, fields, label):
    if type(value) is not dict or set(value) != set(fields):
        raise AdapterError("PDF_REQUEST", f"{label} requires exactly its named fields.", 2)

def unsigned(value, maximum, label, positive=False):
    if type(value) is not int or value < int(positive) or value > maximum:
        raise AdapterError("PDF_REQUEST", f"{label} is outside the permitted integer range.", 2)

def unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise AdapterError("PDF_REQUEST", "Duplicate JSON field.", 2)
        value[key] = item
    return value

def read_request():
    raw = sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1)
    if len(raw) > MAX_REQUEST_BYTES:
        raise AdapterError("PDF_REQUEST_LIMIT", "Request exceeds the 64 KiB policy.", 2)
    try:
        request = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object,
                             parse_constant=lambda _: (_ for _ in ()).throw(ValueError()))
    except AdapterError:
        raise
    except (ValueError, UnicodeError):
        raise AdapterError("PDF_REQUEST", "One strict UTF8 JSON request is required.", 2) from None
    exact_object(request, ["schema_version", "protocol", "request_id", "operation", "input", "workspace", "artifact_policy", "pdf_policy", "options_version"], "Request")
    if type(request["schema_version"]) is not int or request["schema_version"] != 2 or request["protocol"] != PROTOCOL or request["operation"] != "parse_pdf" or request["options_version"] != OPTIONS:
        raise AdapterError("PDF_REQUEST_VERSION", "Only the frozen PDF v2 operation and options are accepted.", 2)
    try:
        identifier = uuid.UUID(request["request_id"])
        valid_uuid = identifier.version == 4 and identifier.variant == uuid.RFC_4122 and str(identifier) == request["request_id"]
    except (ValueError, TypeError, AttributeError):
        valid_uuid = False
    if not valid_uuid:
        raise AdapterError("PDF_REQUEST_IDENTITY", "A canonical UUIDv4 request_id is required.", 2)
    exact_object(request["input"], ["relative_path", "byte_length", "sha256"], "Input")
    exact_object(request["workspace"], ["artifact_directory"], "Workspace")
    if request["input"]["relative_path"] != "input/raw.bin" or request["workspace"]["artifact_directory"] != "artifacts":
        raise AdapterError("PDF_REQUEST_PATH", "Only fixed input and artifact capabilities are accepted.", 2)
    unsigned(request["input"]["byte_length"], MAX_INPUT_BYTES, "Input byte_length")
    if type(request["input"]["sha256"]) is not str or not re.fullmatch("[0-9a-f]{64}", request["input"]["sha256"]):
        raise AdapterError("PDF_REQUEST_IDENTITY", "Input SHA must be lowercase SHA256.", 2)
    for field, maxima in [("artifact_policy", MAX_ARTIFACT), ("pdf_policy", MAX_PDF)]:
        exact_object(request[field], maxima, field)
        for name, maximum in maxima.items():
            unsigned(request[field][name], maximum, f"{field}.{name}", positive=True)
    return request

def runtime_identity():
    actual = {"python": platform.python_version()}
    actual.update({key: importlib.metadata.version(name) for key, name in DISTRIBUTIONS.items()})
    if actual != RUNTIME:
        raise AdapterError("PDF_RUNTIME_MISMATCH", "Selected Python/parser dependency lock does not match.", 2)
    return {"producer": "parser", "adapter_id": "pdfplumber-text", "adapter_version": "1.0.0", "options_version": OPTIONS, "runtime": actual}

def safe_entry(path, directory=False):
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 1024):
        raise AdapterError("PDF_WORKSPACE_PATH", "Linked or reparse entries are not accepted.")
    if directory:
        if not stat.S_ISDIR(info.st_mode):
            raise AdapterError("PDF_WORKSPACE_PATH", "Fixed workspace directory is missing.")
    elif not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise AdapterError("PDF_INPUT_IDENTITY", "Input must be ordinary and singly linked.")
    return info

def read_input(request):
    for name in (Path.cwd(), Path("input"), Path("artifacts")):
        safe_entry(name, directory=True)
    if list(Path("artifacts").iterdir()):
        raise AdapterError("PDF_ARTIFACT_EXISTS", "Owned artifacts directory must be empty.")
    path = Path("input/raw.bin")
    before = safe_entry(path)
    expected = request["input"]
    if before.st_size != expected["byte_length"]:
        raise AdapterError("PDF_INPUT_FINGERPRINT", "Actual input length differs from fixed fingerprint.")
    digest, chunks, length = hashlib.sha256(), [], 0
    identity = lambda i: (i.st_dev, i.st_ino, i.st_size, i.st_mtime_ns, i.st_nlink)
    with path.open("rb") as source:
        opened = os.fstat(source.fileno())
        if identity(opened) != identity(before):
            raise AdapterError("PDF_INPUT_CHANGED", "Input identity changed before reading.")
        while True:
            chunk = source.read(min(262144, MAX_INPUT_BYTES - length + 1))
            if not chunk:
                break
            length += len(chunk)
            if length > MAX_INPUT_BYTES or length > expected["byte_length"]:
                raise AdapterError("PDF_INPUT_LIMIT", "Input grew or exceeded permitted complete length.")
            digest.update(chunk)
            chunks.append(chunk)
        if identity(before) != identity(os.fstat(source.fileno())) or identity(before) != identity(safe_entry(path)):
            raise AdapterError("PDF_INPUT_CHANGED", "Input changed during complete reading.")
    if length != expected["byte_length"] or digest.hexdigest() != expected["sha256"]:
        raise AdapterError("PDF_INPUT_FINGERPRINT", "Actual complete input SHA or length differs from request.")
    return b"".join(chunks)

def issue(code, message, page_index=None):
    return {"code": code, "message": message, "page_indices": [] if page_index is None else [page_index]}

def exception_name(error):
    if error.args and isinstance(error.args[0], Exception):
        error = error.args[0]
    return type(error).__name__[:80]

def nonblank(text):
    # Rust char::is_whitespace uses Unicode White_Space. Python additionally
    # treats U+001C..U+001F as whitespace; preserving these avoids a Core split.
    return any(not ch.isspace() or ch in "\x1c\x1d\x1e\x1f" for ch in text)

def finite_number(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("Non-finite or non-number geometry")
    return float(value)

def box(value):
    if type(value) not in (tuple, list) or len(value) != 4:
        raise ValueError("Geometry box requires four values")
    result = [finite_number(v) for v in value]
    if result[0] >= result[2] or result[1] >= result[3]:
        raise ValueError("Geometry box must be non-empty")
    return result

def geometry(raw):
    media, crop = box(raw["media_box"]), box(raw["crop_box"])
    rotation, unit = raw["rotation"], finite_number(raw["user_unit"])
    if type(rotation) is not int or rotation not in (0, 90, 180, 270) or not 0 < unit <= 64:
        raise ValueError("Unsupported Rotate or UserUnit")
    if crop[0] < media[0] or crop[1] < media[1] or crop[2] > media[2] or crop[3] > media[3]:
        raise ValueError("CropBox is outside MediaBox")
    x0, y0, x1, y1 = media
    def transform(x, y):
        return {0: (x-x0, y1-y), 90: (y-y0, x-x0), 180: (x1-x, y-y0), 270: (y1-y, x1-x)}[rotation]
    corners = [transform(x, y) for x in (crop[0], crop[2]) for y in (crop[1], crop[3])]
    visible = [min(p[0] for p in corners), min(p[1] for p in corners), max(p[0] for p in corners), max(p[1] for p in corners)]
    width, height = (visible[2]-visible[0])*unit, (visible[3]-visible[1])*unit
    if not math.isfinite(width) or not math.isfinite(height) or width <= 0 or height <= 0:
        raise ValueError("Invalid display dimensions")
    return {**raw, "display_width_pt": width, "display_height_pt": height}, visible

def derive_page(source):
    index = source["physical_page_index"]
    page = {"physical_page_index": index, "printed_page_label": None, "status": "failed", "geometry": None, "glyph_count": None, "excluded_glyph_ordinals": [], "text_artifact": None, "regions": []}
    try:
        display, crop = geometry(source["geometry"])
        page["geometry"] = display
    except (ValueError, TypeError, KeyError):
        return page, None, [issue("PDF_PAGE_GEOMETRY", "Raw page geometry cannot use the frozen coordinates.", index)]
    if source["decode_error"] is not None:
        return page, None, [issue("PDF_PAGE_DECODE", source["decode_error"], index)]
    glyphs = source["glyphs"]
    page["glyph_count"] = len(glyphs)
    visible, reasons = [], set()
    for glyph in glyphs:
        bounds = glyph["media_bbox"]
        clipped = [max(bounds[0], crop[0]), max(bounds[1], crop[1]), min(bounds[2], crop[2]), min(bounds[3], crop[3])]
        if clipped[0] >= clipped[2] or clipped[1] >= clipped[3]:
            page["excluded_glyph_ordinals"].append(glyph["ordinal"])
            continue
        if clipped != bounds:
            reasons.add("PDF_CLIPPED_TEXT_REVIEW")
        if not glyph["upright"]:
            reasons.add("PDF_READING_ORDER_REVIEW")
        if "(cid:" in glyph["text"] or "\ufffd" in glyph["text"]:
            reasons.add("PDF_GLYPH_REVIEW")
        if any(c in FORMULA_CHARACTERS for c in glyph["text"]) or any(n in glyph["font_name"].lower() for n in ("math", "symbol")):
            reasons.add("PDF_FORMULA_REVIEW")
        visible.append((glyph, clipped))
    visible.sort(key=lambda p: (p[0]["media_bbox"][1], p[0]["media_bbox"][0], p[0]["ordinal"]))
    lines = []
    for pair in visible:
        if not lines or abs(pair[0]["media_bbox"][1] - lines[-1][0][0]["media_bbox"][1]) > 3:
            lines.append([])
        lines[-1].append(pair)
    if not any(nonblank(g["text"]) for g, _ in visible):
        reasons.add("PDF_EMPTY_TEXT_REVIEW")
    if any(source["visual_objects"].values()):
        reasons.add("PDF_VISUAL_REVIEW")
    text, artifact_id = bytearray(), f"P{index}_text"
    for ordinal, line in enumerate(lines):
        line.sort(key=lambda p: (p[0]["media_bbox"][0], p[0]["ordinal"]))
        contents, previous = [], None
        for glyph, _ in line:
            if previous is not None and nonblank(previous["text"]) and nonblank(glyph["text"]):
                gap = glyph["media_bbox"][0] - previous["media_bbox"][2]
                if gap > 3:
                    contents.append(" ")
                if gap > 72:
                    reasons.add("PDF_READING_ORDER_REVIEW")
            contents.append(glyph["text"])
            previous = glyph
        sizes = [g["font_size"] for g, _ in line if nonblank(g["text"])]
        if sizes and max(sizes)-min(sizes) > 1:
            reasons.add("PDF_FONT_SIZE_REVIEW")
        start = len(text)
        text.extend("".join(contents).encode("utf-8", errors="strict"))
        end = len(text)
        text.extend(b"\n")
        united = [min(c[0] for _, c in line), min(c[1] for _, c in line), max(c[2] for _, c in line), max(c[3] for _, c in line)]
        unit = display["user_unit"]
        region_box = [(united[0]-crop[0])*unit, (united[1]-crop[1])*unit, (united[2]-crop[0])*unit, (united[3]-crop[1])*unit]
        page["regions"].append({"region_id": f"P{index}_R{ordinal}", "ordinal": ordinal, "locator": {"kind": "pdf_page_region", "physical_page_index": index, "bbox_pt": region_box}, "content": {"artifact_id": artifact_id, "sha256": "", "start_byte": start, "end_byte": end}, "glyph_ordinals": [g["ordinal"] for g, _ in line]})
    text_bytes = bytes(text)
    sha = hashlib.sha256(text_bytes).hexdigest()
    page["text_artifact"] = {"artifact_id": artifact_id, "sha256": sha}
    for region in page["regions"]:
        region["content"]["sha256"] = sha
    page["status"] = "needs_review" if reasons else "parsed"
    return page, text_bytes, [issue(code, "Page retains extracted evidence and needs review under declared finite capabilities.", index) for code in sorted(reasons)]

def source_geometry(page_object):
    from pdfminer.pdftypes import resolve1
    attrs = page_object.attrs
    media = box(resolve1(attrs["MediaBox"]))
    raw = {"media_box": media, "crop_box": box(resolve1(attrs.get("CropBox", media))), "rotation": resolve1(attrs.get("Rotate", 0)), "user_unit": finite_number(resolve1(attrs.get("UserUnit", 1)))}
    geometry(raw)
    return raw

def extract_page(pdf, page_object, index):
    from pdfplumber.page import Page
    source = {"physical_page_index": index, "geometry": None, "glyphs": None, "visual_objects": None, "decode_error": None}
    try:
        source["geometry"] = source_geometry(page_object)
    except Exception as error:
        source["decode_error"] = f"Raw page geometry failed ({exception_name(error)})."
        return source
    page = None
    try:
        page = Page(pdf, page_object, index+1)
        offset_x, offset_top = page.mediabox[:2]
        glyphs = []
        for ordinal, char in enumerate(page.chars):
            text, name = char["text"], char["fontname"]
            if type(text) is not str or type(name) is not str or type(char["upright"]) is not bool:
                raise ValueError("Invalid glyph text, font or upright primitive")
            text.encode("utf-8", errors="strict")
            bounds = box([char["x0"]-offset_x, char["top"]-offset_top, char["x1"]-offset_x, char["bottom"]-offset_top])
            glyphs.append({"ordinal": ordinal, "text": text, "media_bbox": bounds, "upright": char["upright"], "font_size": finite_number(char["size"]), "font_name": name})
        visuals = {"images": len(page.images), "lines": len(page.lines), "rectangles": len(page.rects), "curves": len(page.curves), "annotations": len(page.annots)}
        source["glyphs"], source["visual_objects"] = glyphs, visuals
    except Exception as error:
        source["decode_error"] = f"Page content decoding failed ({exception_name(error)})."
    finally:
        if page is not None:
            page.close()
    return source

def encode_json(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8", errors="strict")

def parse_document(request, identity, raw):
    response = {"schema_version": 2, "protocol": PROTOCOL, "request_id": request["request_id"], "adapter": identity, "input": request["input"], "status": "failed", "page_count": None, "inventory": None, "pages": [], "artifacts": [], "issues": []}
    import pdfplumber
    from pdfminer.pdfpage import PDFPage
    from pdfminer.pdftypes import resolve1
    logging.getLogger("pdfminer").setLevel(logging.CRITICAL)
    stream = io.BytesIO(raw)
    try:
        pdf = pdfplumber.open(stream, unicode_norm=None, strict_metadata=True)
    except Exception as error:
        stream.close()
        reason = exception_name(error)
        code = "PDF_ENCRYPTED" if reason in ("PDFPasswordIncorrect", "PDFEncryptionError") else "PDF_DOCUMENT_DECODE"
        response["issues"] = [issue(code, f"Document opening failed ({reason}).")]
        return response, []
    # Do not call PDF.close/__exit__: 0.11.9 flushes its page cache then
    # enumerates self.pages again, destroying an earlier enumeration failure.
    # All Page caches and this external in-memory stream are closed explicitly.
    try:
        if pdf.doc.encryption is not None:
            response["issues"] = [issue("PDF_ENCRYPTED", "Encrypted PDFs are outside the declared adapter capabilities.")]
            return response, []
        try:
            if "Pages" not in pdf.doc.catalog:
                raise ValueError("No complete page tree")
            pages = []
            for page in PDFPage.create_pages(pdf.doc):
                pages.append(page)
                if len(pages) > request["pdf_policy"]["max_pages"]:
                    response["issues"] = [issue("PDF_PAGE_LIMIT", "Complete page inventory exceeds requested page limit.")]
                    return response, []
            declared_count = resolve1(resolve1(pdf.doc.catalog["Pages"]).get("Count"))
            if type(declared_count) is not int or declared_count != len(pages) or not pages:
                raise ValueError("Page tree count differs from complete enumeration")
        except Exception as error:
            response["issues"] = [issue("PDF_DOCUMENT_ENUMERATION", f"Complete page enumeration failed ({exception_name(error)}).")]
            return response, []
        sources, derived, outputs, all_issues = [], [], [], []
        totals = {"max_glyphs": 0, "max_objects": 0, "max_regions": 0}
        for index, page_object in enumerate(pages):
            source = extract_page(pdf, page_object, index)
            sources.append(source)
            page, text, problems = derive_page(source)
            derived.append(page)
            all_issues.extend(problems)
            totals["max_glyphs"] += len(source["glyphs"] or [])
            totals["max_objects"] += sum((source["visual_objects"] or {}).values())
            totals["max_regions"] += len(page["regions"])
            for name, count in totals.items():
                if count > request["pdf_policy"][name]:
                    response["issues"] = [issue(f"PDF_{name[4:].upper()}_LIMIT", "Complete decoded inventory exceeds requested PDF policy.")]
                    return response, []
            if text is not None:
                outputs.append((f"P{index}_text", "utf8_text", text))
        inventory = {"schema_version": 1, "input": request["input"], "page_count": len(pages), "pages": sources}
        inventory_bytes = encode_json(inventory)
        outputs.insert(0, ("pdf_inventory", "binary", inventory_bytes))
        response.update(page_count=len(pages), inventory={"artifact_id": "pdf_inventory", "sha256": hashlib.sha256(inventory_bytes).hexdigest()}, pages=derived, issues=all_issues, status="complete" if all(p["status"] == "parsed" for p in derived) else "partial")
        return response, outputs
    finally:
        stream.close()

def write_outputs(request, response, outputs):
    policy = request["artifact_policy"]
    if len(outputs) > policy["max_files"] or any(len(data) > policy["max_file_bytes"] for _, _, data in outputs) or sum(len(data) for _, _, data in outputs) > policy["max_total_bytes"]:
        response.update(status="failed", page_count=None, inventory=None, pages=[], artifacts=[], issues=[issue("PDF_ARTIFACT_LIMIT", "Complete resources exceed requested artifact policy.")])
        outputs = []
    manifest = []
    for artifact_id, kind, data in outputs:
        suffix = ".txt" if kind == "utf8_text" else ".bin"
        manifest.append({"artifact_id": artifact_id, "kind": kind, "relative_path": f"artifacts/{artifact_id}{suffix}", "byte_length": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    response["artifacts"] = manifest
    serialized = encode_json(response)
    if len(serialized) > MAX_STDOUT_BYTES:
        raise AdapterError("PDF_RESPONSE_LIMIT", "Response exceeds bounded stdout policy.")
    safe_entry(Path("artifacts"), directory=True)
    if list(Path("artifacts").iterdir()):
        raise AdapterError("PDF_ARTIFACT_EXISTS", "Artifact directory changed before exclusive publication.")
    for item, (_, _, data) in zip(manifest, outputs):
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(item["relative_path"], flags, 0o600)
        with os.fdopen(descriptor, "wb") as target:
            if os.fstat(target.fileno()).st_nlink != 1:
                raise AdapterError("PDF_ARTIFACT_IDENTITY", "Resource is not singly linked.")
            target.write(data)
            target.flush()
    return serialized

def main():
    try:
        request = read_request()
        identity = runtime_identity()
        raw = read_input(request)
        response, outputs = parse_document(request, identity, raw)
        serialized = write_outputs(request, response, outputs)
        sys.stdout.buffer.write(serialized + b"\n")
        sys.stdout.buffer.flush()
        return 0
    except AdapterError as error:
        sys.stderr.write(f"{error.code}: {str(error)[:256]}\n")
        return error.exit_code
    except OSError as error:
        sys.stderr.write(f"PDF_IO: Fixed input/resource operation failed ({exception_name(error)}).\n")
        return 3
    except Exception as error:
        sys.stderr.write(f"PDF_ADAPTER_INTERNAL: Adapter failed ({exception_name(error)}).\n")
        return 1

if __name__ == "__main__":
    sys.exit(main())
