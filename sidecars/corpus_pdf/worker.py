"""Resumable text-PDF worker. The host owns source snapshots and cancellation.

Unlike the historical fixture adapter, this worker never reads a whole PDF into
bytes or builds pdfplumber's all-pages cache. Only page-sized extraction objects
are retained; PDF cross-reference/page-tree metadata still scales with the file.
"""
from __future__ import annotations

import argparse
from collections import Counter, OrderedDict
import gc
import importlib.metadata
import json
import logging
import math
import os
from pathlib import Path
import re
import sys
import tempfile


DEFAULT_BATCH_PAGES = 8
MAX_PAGE_JSON_BYTES = 16 * 1024 * 1024
PARSER = {
    "engine": "pdfplumber", "engine_version": "0.11.9",
    "engine_dependency": "pdfminer.six", "engine_dependency_version": "20251230",
    "adapter_version": "noetome-corpus-pdf-v1", "python_version": "3.12.14",
}
LIMITS = {
    "max_file_bytes": None, "max_pages": None,
    "default_batch_pages": DEFAULT_BATCH_PAGES,
    "batch_timeout_seconds": 180,
    "max_page_json_bytes": MAX_PAGE_JSON_BYTES,
    "ocr_available": False, "timeout_managed_by_host": True,
}


class WorkerError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def issue(code, message):
    return {"code": code, "message": message}


class MetadataCache(OrderedDict):
    """Cache small PDF metadata without retaining decoded image/content streams.

    Disabling every cache repeatedly decompresses the same object stream for
    each page dictionary. A bounded metadata cache avoids that quadratic work.
    """
    def __init__(self, max_entries, max_bytes):
        super().__init__()
        self.max_entries, self.max_bytes, self.used = max_entries, max_bytes, 0
        self.sizes = {}

    def __getitem__(self, key):
        value = super().__getitem__(key)
        self.move_to_end(key)
        return value

    def __setitem__(self, key, value):
        from pdfminer.pdftypes import PDFStream
        if isinstance(value, tuple) and value and isinstance(value[0], PDFStream):
            return
        seen = set()
        def size(item):
            if id(item) in seen:
                return 0
            seen.add(id(item))
            result = sys.getsizeof(item)
            if isinstance(item, dict):
                result += sum(size(k) + size(v) for k, v in item.items())
            elif isinstance(item, (list, tuple)):
                result += sum(size(v) for v in item)
            return result
        count = size(value)
        if count > self.max_bytes:
            return
        if key in self:
            self.used -= self.sizes.pop(key)
            super().__delitem__(key)
        while self and (len(self) >= self.max_entries or self.used + count > self.max_bytes):
            old, _ = self.popitem(last=False)
            self.used -= self.sizes.pop(old)
        super().__setitem__(key, value)
        self.sizes[key] = count
        self.used += count

    def clear(self):
        super().clear()
        self.sizes.clear()
        self.used = 0


def check_runtime():
    actual_python = ".".join(map(str, sys.version_info[:3]))
    if actual_python != PARSER["python_version"]:
        raise WorkerError("PDF_RUNTIME", "This worker requires Python 3.12.14.")
    for distribution, expected in (("pdfplumber", PARSER["engine_version"]),
                                    ("pdfminer.six", PARSER["engine_dependency_version"])):
        try:
            actual = importlib.metadata.version(distribution)
        except importlib.metadata.PackageNotFoundError:
            actual = "missing"
        if actual != expected:
            raise WorkerError("PDF_RUNTIME", f"{distribution} requires {expected}; found {actual}.")


def finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise WorkerError("PDF_PAGE_GEOMETRY", "Page coordinates must be finite numbers.")
    return float(value)


def geometry_for(raw_page, resolve):
    def box(value):
        value = resolve(value)
        if not isinstance(value, (list, tuple)) or len(value) != 4:
            raise WorkerError("PDF_PAGE_GEOMETRY", "Invalid PDF page box.")
        x0, y0, x1, y1 = [finite(resolve(n)) for n in value]
        return [min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1)]

    media = box(raw_page.attrs.get("MediaBox"))
    crop = box(raw_page.attrs.get("CropBox", media))
    if not (media[0] <= crop[0] < crop[2] <= media[2]
            and media[1] <= crop[1] < crop[3] <= media[3]):
        raise WorkerError("PDF_PAGE_GEOMETRY", "CropBox must have area inside MediaBox.")
    rotation = finite(resolve(raw_page.attrs.get("Rotate", 0)))
    if rotation % 90:
        raise WorkerError("PDF_PAGE_GEOMETRY", "Rotate must be a multiple of 90 degrees.")
    rotation = int(rotation) % 360
    unit = finite(resolve(raw_page.attrs.get("UserUnit", 1)))
    if unit <= 0:
        raise WorkerError("PDF_PAGE_GEOMETRY", "UserUnit must be positive.")
    left, bottom, right, top = crop
    matrices = {
        0: [unit, 0, 0, -unit, -unit * left, unit * top],
        90: [0, unit, unit, 0, -unit * bottom, -unit * left],
        180: [-unit, 0, 0, unit, unit * right, -unit * bottom],
        270: [0, -unit, -unit, 0, unit * top, unit * right],
    }
    width, height = (right - left) * unit, (top - bottom) * unit
    if rotation in (90, 270):
        width, height = height, width
    if not all(math.isfinite(n) for n in [width, height, *matrices[rotation]]):
        raise WorkerError("PDF_PAGE_GEOMETRY", "Scaled geometry is not finite.")
    return {"media_box": media, "crop_box": crop, "user_unit": unit,
            "coordinate_space": "crop-top-left-rotated-points",
            "pdf_to_display": matrices[rotation], "rotation": rotation,
            "width_pt": width, "height_pt": height}


def display_bbox(item, page, geometry):
    """Convert pdfplumber's rotated MediaBox coordinates to visible CropBox pt."""
    ml, mb, mr, mt = geometry["media_box"]
    cl, cb, cr, ct = geometry["crop_box"]
    offsets = {0: (cl - ml, mt - ct), 90: (cb - mb, cl - ml),
               180: (mr - cr, cb - mb), 270: (mt - ct, mr - cr)}
    dx, dy = offsets[geometry["rotation"]]
    x0, top, x1, bottom = [finite(item[key]) for key in ("x0", "top", "x1", "bottom")]
    scale = geometry["user_unit"]
    raw = [(x0 - page.mediabox[0] - dx) * scale,
           (top - page.mediabox[1] - dy) * scale,
           (x1 - page.mediabox[0] - dx) * scale,
           (bottom - page.mediabox[1] - dy) * scale]
    if not all(math.isfinite(n) for n in raw):
        raise WorkerError("PDF_PAGE_GEOMETRY", "Object coordinates are not finite.")
    width, height = geometry["width_pt"], geometry["height_pt"]
    if raw[2] < 0 or raw[3] < 0 or raw[0] > width or raw[1] > height:
        return None, False
    clipped = [max(0, raw[0]), max(0, raw[1]), min(width, raw[2]), min(height, raw[3])]
    if clipped[0] > clipped[2] or clipped[1] > clipped[3]:
        raise WorkerError("PDF_PAGE_GEOMETRY", "Object bbox is reversed.")
    return clipped, any(abs(a - b) > 0.01 for a, b in zip(raw, clipped))


def page_records(document):
    """Enumerate the page tree without constructing/decoding every PDFPage.

    Construction errors in one Page must not end PDFPage.create_pages' generator
    and hide all subsequent physical pages. A broken page tree remains a file
    error because its physical inventory cannot be confirmed.
    """
    from pdfminer.pdfpage import PDFPage
    from pdfminer.pdftypes import resolve1
    root = document.catalog.get("Pages")
    if root is None:
        raise WorkerError("PDF_PAGE_TREE", "PDF catalog has no page tree.")
    stack = [(root, {}, frozenset())]
    while stack:
        reference, inherited, ancestors = stack.pop()
        object_id = getattr(reference, "objid", id(reference))
        if object_id in ancestors:
            raise WorkerError("PDF_PAGE_TREE", "Cyclic PDF page tree.")
        value = resolve1(reference)
        if not isinstance(value, dict):
            raise WorkerError("PDF_PAGE_TREE", "Page tree entry is not a dictionary.")
        attrs = dict(inherited)
        attrs.update(value)
        kind = getattr(attrs.get("Type"), "name", None)
        if kind == "Pages":
            children = resolve1(attrs.get("Kids"))
            if not isinstance(children, list):
                raise WorkerError("PDF_PAGE_TREE", "Page tree node has no Kids array.")
            parent = {k: attrs[k] for k in PDFPage.INHERITABLE_ATTRS if k in attrs}
            stack.extend((child, parent, ancestors | {object_id}) for child in reversed(children))
        elif kind == "Page":
            yield object_id, attrs
        else:
            raise WorkerError("PDF_PAGE_TREE", "Unknown page tree entry type.")


def failed_page(number, code, message, geometry=None):
    geometry = geometry or {}
    return {"schema_version": 1, "page_number": number,
            "width_pt": geometry.get("width_pt"), "height_pt": geometry.get("height_pt"),
            "rotation": geometry.get("rotation"), "geometry": geometry or None,
            "status": "failed", "text": "", "regions": [],
            "issues": [issue(code, message)], "parser": dict(PARSER)}


def extract_page(pdf, number, object_id, attrs):
    from pdfminer.pdfpage import PDFPage
    from pdfminer.pdftypes import resolve1
    from pdfplumber.page import Page
    page = None
    geometry = None
    try:
        raw = PDFPage(pdf.doc, object_id, attrs, None)
        geometry = geometry_for(raw, resolve1)
        page = Page(pdf, raw, page_number=number, initial_doctop=0)
        regions, flags = [], {}
        words = []
        for word in page.extract_words():
            bounds, clipped = display_bbox(word, page, geometry)
            if bounds is None:
                continue
            if clipped:
                flags["PDF_CROP_TEXT"] = "Some text crosses the visible CropBox boundary."
            words.append({"text": word["text"], "bbox": bounds})
        words.sort(key=lambda word: (word["bbox"][1], word["bbox"][0]))
        # Keep wide-gap line segments separate; a row across two columns must
        # never be presented as a confidently reconstructed reading order.
        lines = []
        for word in words:
            if not lines or abs(word["bbox"][1] - lines[-1][0]["bbox"][1]) > 3 * geometry["user_unit"]:
                lines.append([])
            lines[-1].append(word)
        for line in lines:
            line.sort(key=lambda word: word["bbox"][0])
            segments = [[]]
            for word in line:
                if segments[-1] and word["bbox"][0] - segments[-1][-1]["bbox"][2] > 18 * geometry["user_unit"]:
                    flags["PDF_COMPLEX_LAYOUT"] = "Wide text gaps may indicate columns or a table; reading order requires review."
                    segments.append([])
                segments[-1].append(word)
            for segment in segments:
                bounds = [word["bbox"] for word in segment]
                regions.append({"ordinal": len(regions), "kind": "text",
                                "text": " ".join(word["text"] for word in segment),
                                "bbox": [min(b[0] for b in bounds), min(b[1] for b in bounds),
                                         max(b[2] for b in bounds), max(b[3] for b in bounds)]})
        # A two-column page can stagger its baselines, leaving no same-row gap.
        # Detect repeated left/right line starts with overlapping vertical spans.
        # Preserve positioned regions and require review instead of claiming the
        # top-to-bottom interleaving is a confirmed reading order.
        for left in regions:
            left_group = [r for r in regions if abs(r["bbox"][0] - left["bbox"][0]) <= 12 * geometry["user_unit"]]
            if len(left_group) < 2:
                continue
            right_group = [r for r in regions if r["bbox"][0] - left["bbox"][0] >= geometry["width_pt"] * .25
                           and r["bbox"][0] > max(l["bbox"][2] for l in left_group) + 12 * geometry["user_unit"]]
            if len(right_group) >= 2 and max(min(r["bbox"][1] for r in left_group), min(r["bbox"][1] for r in right_group)) < min(max(r["bbox"][3] for r in left_group), max(r["bbox"][3] for r in right_group)):
                flags["PDF_COMPLEX_LAYOUT"] = "Overlapping left/right text columns have an unconfirmed reading order."
                break
        text = "\n".join(region["text"] for region in regions)
        if "\ufffd" in text or "(cid:" in text.lower() or any(ord(c) < 32 and not c.isspace() for c in text):
            flags["PDF_TEXT_ENCODING"] = "The text layer contains undecoded or invalid characters."
        if any(not char.get("upright", True) for char in page.chars):
            flags["PDF_TEXT_ORIENTATION"] = "Nonhorizontal text requires reading-order review."
        if geometry["rotation"]:
            flags["PDF_PAGE_ROTATION"] = "Rotated page coordinates are retained; text reading order requires review."
        if any(re.search(r"symbol|math|cmsy|cmmi|cmex|stix|msam|msbm", char.get("fontname", ""), re.I)
               for char in page.chars):
            flags["PDF_FORMULA_TEXT"] = "Formula font content requires formula/meaning review."
        visual_count = 0
        image_count = 0
        largest_image_fraction = 0
        for kind, objects in (("image", page.images), ("vector", page.lines),
                              ("vector", page.rects), ("vector", page.curves)):
            for item in objects:
                bounds, _ = display_bbox(item, page, geometry)
                if bounds is None:
                    continue
                regions.append({"ordinal": len(regions), "kind": kind, "text": "", "bbox": bounds})
                visual_count += 1
                image_count += kind == "image"
                if kind == "image":
                    largest_image_fraction = max(largest_image_fraction,
                        (bounds[2] - bounds[0]) * (bounds[3] - bounds[1])
                        / (geometry["width_pt"] * geometry["height_pt"]))
        annotations = resolve1(attrs.get("Annots")) or []
        if annotations:
            flags["PDF_ANNOTATIONS"] = "Page annotations are retained in the source PDF and require review."
        if visual_count:
            flags["PDF_VISUAL_CONTENT"] = "Image/vector content requires review; regions refer to the retained source PDF."
        if not text.strip() and image_count:
            status = "needs_ocr"
            flags["PDF_NO_TEXT_LAYER"] = "Visible image content has no usable text layer; OCR is required."
        elif largest_image_fraction >= 0.5 and len(text.strip()) < 80:
            status = "needs_ocr"
            flags["PDF_SPARSE_TEXT_LAYER"] = "A large page image has little text (possibly only a page number); OCR is required."
        elif not text.strip() and not visual_count and not annotations:
            status = "blank"
        elif flags or not text.strip():
            status = "needs_review"
        else:
            status = "text_ready"
        return {"schema_version": 1, "page_number": number,
                "width_pt": geometry["width_pt"], "height_pt": geometry["height_pt"],
                "rotation": geometry["rotation"], "geometry": geometry, "status": status,
                "text": text, "regions": regions,
                "issues": [issue(code, message) for code, message in sorted(flags.items())],
                "parser": dict(PARSER)}
    except Exception as exc:
        return failed_page(number, getattr(exc, "code", "PDF_PAGE_PARSE"),
                           str(exc)[:500] or type(exc).__name__, geometry)
    finally:
        if page is not None:
            page.close()


def outline_entries(document, page_ids):
    from pdfminer.pdfdocument import PDFNoOutlines
    from pdfminer.pdftypes import resolve1
    entries, issues = [], []
    try:
        for level, title, dest, action, _ in document.get_outlines():
            number = None
            try:
                if dest is None and action:
                    action = resolve1(action)
                    if getattr(action.get("S"), "name", None) == "GoTo":
                        dest = action.get("D")
                if isinstance(dest, (str, bytes)):
                    dest = document.get_dest(dest)
                dest = resolve1(dest)
                if isinstance(dest, dict):
                    dest = resolve1(dest.get("D"))
                if isinstance(dest, list) and dest:
                    target = dest[0]
                    if hasattr(target, "objid"):
                        number = page_ids.get(target.objid)
                    elif type(target) is int and target + 1 in page_ids.values():
                        number = target + 1
            except Exception:
                pass
            entries.append({"level": level, "title": title, "page_number": number})
            if number is None:
                issues.append(issue("PDF_OUTLINE_DESTINATION", "An outline destination could not be resolved."))
    except PDFNoOutlines:
        pass
    except Exception as exc:
        issues.append(issue("PDF_OUTLINE", "Outline metadata could not be fully read: " + type(exc).__name__))
    return entries, issues


def atomic_json(path, value, maximum=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix="." + path.name + ".", suffix=".tmp", delete=False) as stream:
            temporary = Path(stream.name)
            size = 0
            for part in json.JSONEncoder(ensure_ascii=False, allow_nan=False, separators=(",", ":")).iterencode(value):
                encoded = part.encode("utf-8")
                size += len(encoded)
                if maximum is not None and size + 1 > maximum:
                    raise WorkerError("PDF_PAGE_JSON_LIMIT", "Page JSON exceeds 16 MiB; page retained as failed, without truncation.")
                stream.write(encoded)
            stream.write(b"\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def emit(event, **fields):
    print(json.dumps({"event": event, **fields}, ensure_ascii=False, separators=(",", ":")), flush=True)


def file_identity(stream):
    stat = os.fstat(stream.fileno())
    return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns


def run(action, source, output, start=1, end=None):
    check_runtime()
    import pdfplumber
    from pdfminer.pdfinterp import PDFResourceManager
    from pdfminer.pdfdocument import PDFPasswordIncorrect
    output.mkdir(parents=True, exist_ok=True)
    preflight = {"schema_version": 1, "kind": "preflight", "status": "ready",
                 "byte_length": source.stat().st_size, "page_count": None,
                 "encrypted": False, "password_required": False, "text_sample": [],
                 "outline": [], "limits": dict(LIMITS), "parser": dict(PARSER), "issues": []}
    pdf = None
    with source.open("rb") as stream:
        identity = file_identity(stream)
        try:
            pdf = pdfplumber.PDF(stream, stream_is_external=True)
            # Keep bounded page-tree/object metadata; decoded image/content
            # streams and per-page font resources are not retained.
            pdf.doc.caching = True
            pdf.doc._cached_objs = MetadataCache(512, 8 * 1024 * 1024)
            pdf.doc._parsed_objs = MetadataCache(32, 16 * 1024 * 1024)
            pdf.rsrcmgr = PDFResourceManager(caching=False)
            preflight["encrypted"] = pdf.doc.encryption is not None
            if not pdf.doc.is_extractable:
                preflight["issues"].append(issue("PDF_EXTRACTION_PERMISSION", "PDF metadata disallows extraction; text availability is not guaranteed."))
            page_ids, counts = {}, Counter()
            count = 0
            end = end if end is not None else start + DEFAULT_BATCH_PAGES - 1
            for count, (object_id, attrs) in enumerate(page_records(pdf.doc), 1):
                if action == "preflight":
                    page_ids.setdefault(object_id, count)
                    if count <= 3:
                        page = extract_page(pdf, count, object_id, attrs)
                        preflight["text_sample"].append({"page_number": count, "status": page["status"],
                                                         "text_characters": len(page["text"]), "issues": page["issues"]})
                        del page
                elif start <= count <= end:
                    page = extract_page(pdf, count, object_id, attrs)
                    if file_identity(stream) != identity:
                        raise WorkerError("PDF_INPUT_CHANGED", "Source PDF changed during extraction.")
                    path = Path("pages") / f"{count:06d}.json"
                    try:
                        atomic_json(output / path, page, MAX_PAGE_JSON_BYTES)
                    except WorkerError as exc:
                        page = failed_page(count, exc.code, str(exc), page["geometry"])
                        atomic_json(output / path, page, MAX_PAGE_JSON_BYTES)
                    counts[page["status"]] += 1
                    emit("page_failed" if page["status"] == "failed" else "page_written",
                         page_number=count, status=page["status"], path=path.as_posix())
                    del page
                    if count == end:
                        break
                if count % DEFAULT_BATCH_PAGES == 0:
                    gc.collect()
                if action == "preflight" and count % 128 == 0:
                    emit("preflight_progress", pages_seen=count)
            if file_identity(stream) != identity:
                raise WorkerError("PDF_INPUT_CHANGED", "Source PDF changed during extraction.")
            if action == "preflight":
                preflight["page_count"] = count
                preflight["outline"], outline_issues = outline_entries(pdf.doc, page_ids)
                preflight["issues"].extend(outline_issues)
                if count == 0:
                    raise WorkerError("PDF_PAGE_TREE", "PDF has no physical pages.")
                atomic_json(output / "preflight.json", preflight)
                emit("preflight", status=preflight["status"], path="preflight.json", page_count=count)
            else:
                if start > count:
                    raise WorkerError("PDF_PAGE_RANGE", "Start page is beyond the physical page count.")
                emit("completed", start_page=start, end_page=min(end, count),
                     processed_pages=sum(counts.values()), counts=dict(counts))
            return 0
        except Exception as exc:
            underlying = exc
            while underlying.__cause__ is not None:
                underlying = underlying.__cause__
            password_required = isinstance(underlying, PDFPasswordIncorrect) or any(
                isinstance(arg, PDFPasswordIncorrect) for arg in exc.args)
            code = "PDF_PASSWORD_REQUIRED" if password_required else getattr(exc, "code", "PDF_DOCUMENT")
            message = "PDF requires a password; password input is not supported by this worker." if password_required else (str(exc)[:500] or type(exc).__name__)
            if action == "preflight":
                preflight.update(status="password_required" if password_required else "failed",
                                 encrypted=preflight["encrypted"] or password_required,
                                 password_required=password_required)
                preflight["issues"].append(issue(code, message))
                atomic_json(output / "preflight.json", preflight)
            emit("error", code=code, message=message, **({"path": "preflight.json"} if action == "preflight" else {}))
            return 2
        finally:
            # PDF.close() accesses PDF.pages and would instantiate the entire
            # document. We own the stream and close page objects individually.
            if pdf is not None:
                pdf.flush_cache()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("preflight", "pages"))
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--start-page", type=int, default=1)
    parser.add_argument("--end-page", type=int)
    args = parser.parse_args(argv)
    if args.start_page < 1 or (args.end_page is not None and args.end_page < args.start_page):
        parser.error("page range must be an increasing inclusive 1-based range")
    logging.getLogger("pdfminer").setLevel(logging.ERROR)
    logging.getLogger("pdfplumber").setLevel(logging.ERROR)
    try:
        return run(args.action, args.input, args.output_dir, args.start_page, args.end_page)
    except Exception as exc:
        emit("error", code=getattr(exc, "code", "PDF_WORKER"), message=str(exc)[:500] or type(exc).__name__)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
