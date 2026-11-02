"""Embedded local text PDF adapter; Core owns cancellation and final validation."""

from __future__ import annotations

import hashlib
import importlib.metadata
import io
import json
import logging
import math
import os
from pathlib import Path
import re
import stat
import sys
import time
import uuid
import warnings


MAX_REQUEST_BYTES = 64 * 1024
MAX_RAW_BYTES = 16 * 1024 * 1024
MAX_PAGES = 128
MAX_PAGE_GLYPHS = 50000
MAX_TOTAL_GLYPHS = 200000
PINS = {
    'pdfplumber': '0.11.9',
    'pdfminer.six': '20251230',
    'pypdfium2': '5.14.0',
    'Pillow': '12.3.0',
    'charset-normalizer': '3.5.2',
    'cryptography': '50.0.2',
    'cffi': '2.1.1',
    'pycparser': '3.11',
}
PARSER_IDENTITY = {
    'engine': 'pdfplumber',
    'engine_version': '0.11.9',
    'engine_dependency': 'pdfminer.six',
    'engine_dependency_version': '20251230',
    'adapter_version': 'noetome-pdf-text-v1',
    'options_version': 'crop-pt-lines-v1',
    'python_version': '3.12.14',
}
LIMIT_FIELDS = {
    'request_bytes', 'response_bytes', 'diagnostic_bytes', 'input_bytes',
    'resource_bytes', 'total_resource_bytes', 'resources', 'units', 'issues',
    'timeout_ms',
}
STARTED = time.monotonic()
DIAGNOSTIC_LIMIT = 64 * 1024


class AdapterFailure(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


class PageFailure(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def require(condition: bool, code: str, message: str) -> None:
    if not condition:
        raise AdapterFailure(code, message)


def exact_object(value, fields, description):
    require(type(value) is dict and set(value) == set(fields), 'PDF_REQUEST',
            f"unknown/missing {description} fields")
    return value


def unsigned(value, maximum: int, description: str, *, positive=False):
    require(type(value) is int and (1 if positive else 0) <= value <= maximum,
            'PDF_REQUEST', f"invalid unsigned {description}")
    return value


def uuid4(value, description):
    require(type(value) is str, 'PDF_REQUEST', f"invalid {description}")
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError):
        raise AdapterFailure('PDF_REQUEST', f"invalid {description}") from None
    require(parsed.version == 4 and parsed.variant == uuid.RFC_4122
            and str(parsed) == value, 'PDF_REQUEST', f"invalid lowercase UUIDv4 {description}")


def selected_path(value, description):
    require(type(value) is str and 0 < len(value) <= 32768 and '\x00' not in value,
            'PDF_REQUEST', f"invalid capability {description}")
    try:
        value.encode('utf-8', 'strict')
    except UnicodeError:
        raise AdapterFailure('PDF_REQUEST', f"{description} is not UTF8") from None
    path = Path(value)
    require(path.is_absolute(), 'PDF_REQUEST', f"relative {description}")
    require('..' not in path.parts, 'PDF_REQUEST', f"traversal in {description}")
    if os.name == 'nt':
        require(all(':' not in component and not component.endswith(('.', ' '))
                    for component in path.parts[1:]), 'PDF_REQUEST',
                f"unsafe Windows {description}")
    return path


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'PDF_REQUEST', 'duplicate JSON field')
        result[key] = value
    return result


def read_request():
    global DIAGNOSTIC_LIMIT
    data = bytearray()
    while True:
        chunk = sys.stdin.buffer.read(min(8192, MAX_REQUEST_BYTES + 1 - len(data)))
        if not chunk:
            break
        data.extend(chunk)
        require(len(data) <= MAX_REQUEST_BYTES, 'PDF_LIMIT', 'request exceeds 65536 bytes')
    try:
        request = json.loads(data.decode('utf-8', 'strict'), object_pairs_hook=unique_object,
                             parse_constant=lambda _: (_ for _ in ()).throw(
                                 AdapterFailure('PDF_REQUEST', 'nonfinite JSON number')))
    except (ValueError, UnicodeError):
        raise AdapterFailure('PDF_REQUEST', 'expected one UTF8 JSON request') from None
    if type(request) is dict and type(request.get('limits')) is dict:
        cap = request['limits'].get('diagnostic_bytes')
        if type(cap) is int and 0 < cap < (1 << 64):
            DIAGNOSTIC_LIMIT = min(cap, 64 * 1024)
    exact_object(request, {'protocol_version', 'request_id', 'input', 'workspace', 'limits'}, 'request')
    require(type(request['protocol_version']) is int and request['protocol_version'] == 1,
            'PDF_REQUEST', 'unsupported protocol_version')
    uuid4(request['request_id'], 'request_id')
    source = exact_object(request['input'], {'path', 'access', 'fingerprint'}, 'input')
    workspace = exact_object(request['workspace'], {'path', 'access', 'capability_id'}, 'workspace')
    require(source['access'] == 'read_only_single_file', 'PDF_REQUEST', 'unsupported input access')
    require(workspace['access'] == 'exclusive_new_directory', 'PDF_REQUEST', 'unsupported workspace access')
    selected_path(source['path'], 'input.path')
    selected_path(workspace['path'], 'workspace.path')
    uuid4(workspace['capability_id'], 'workspace.capability_id')
    expected = exact_object(source['fingerprint'], {'sha256', 'byte_length'}, 'input fingerprint')
    require(type(expected['sha256']) is str and re.fullmatch(r'[0-9a-f]{64}', expected['sha256']) is not None,
            'PDF_REQUEST', 'invalid lowercase input SHA256')
    unsigned(expected['byte_length'], (1 << 64) - 1, 'input byte_length')
    limits = exact_object(request['limits'], LIMIT_FIELDS, 'limits')
    for name, limit in limits.items():
        unsigned(limit, (1 << (32 if name in {'resources', 'units', 'issues'} else 64)) - 1,
                 'limits.' + name, positive=True)
    DIAGNOSTIC_LIMIT = min(limits['diagnostic_bytes'], 64 * 1024)
    require(len(data) <= limits['request_bytes'], 'PDF_LIMIT', 'request byte limit')
    return request


class Control:
    def __init__(self, limits):
        self.limits = limits
        self.deadline = STARTED + limits['timeout_ms'] / 1000

    def check(self):
        require(time.monotonic() < self.deadline, 'PDF_TIMEOUT', 'adapter runtime limit')


def check_runtime(control):
    require(tuple(sys.version_info[:3]) == (3, 12, 14), 'PDF_RUNTIME',
            'Python 3.12.14 is required')
    for name, expected in PINS.items():
        control.check()
        try:
            actual = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            raise AdapterFailure('PDF_RUNTIME', f"missing dependency {name}") from None
        require(actual == expected, 'PDF_RUNTIME', f"{name} requires {expected}")
    control.check()


def is_reparse(info):
    return bool(getattr(info, 'st_file_attributes', 0) & 0x400) or stat.S_ISLNK(info.st_mode)


def identity(info):
    return info.st_dev, info.st_ino


def plain_components(path: Path, code: str):
    for component in reversed((path, *path.parents)):
        info = component.lstat()
        require(not is_reparse(info), code, 'link/reparse component')
    return path.lstat()


def read_selected(request, control):
    path = selected_path(request['input']['path'], 'input.path')
    before = plain_components(path, 'PDF_INPUT')
    require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1,
            'PDF_INPUT', 'input is not a plain single-link file')
    maximum = min(control.limits['input_bytes'], control.limits['resource_bytes'], MAX_RAW_BYTES)
    require(before.st_size <= maximum, 'PDF_LIMIT', 'raw PDF byte limit')
    data = bytearray()
    digest = hashlib.sha256()
    with path.open('rb') as source:
        opened = os.fstat(source.fileno())
        require(identity(opened) == identity(before) and opened.st_nlink == 1,
                'PDF_INPUT', 'input identity changed on open')
        while True:
            control.check()
            chunk = source.read(8192)
            if not chunk:
                break
            require(len(data) + len(chunk) <= maximum, 'PDF_LIMIT', 'raw PDF byte limit')
            data.extend(chunk)
            digest.update(chunk)
        after = plain_components(path, 'PDF_INPUT')
        current = os.fstat(source.fileno())
        require(identity(after) == identity(before) == identity(current)
                and after.st_nlink == current.st_nlink == 1
                and after.st_size == before.st_size == len(data)
                and after.st_mtime_ns == before.st_mtime_ns == current.st_mtime_ns,
                'PDF_INPUT', 'input changed during read')
    actual = {'sha256': digest.hexdigest(), 'byte_length': len(data)}
    require(actual == request['input']['fingerprint'], 'PDF_INPUT', 'input fingerprint mismatch')
    return bytes(data), actual


def bounded_json(value, maximum, control):
    data = bytearray()
    encoder = json.JSONEncoder(ensure_ascii=False, allow_nan=False, separators=(',', ':'))
    for piece in encoder.iterencode(value):
        control.check()
        chunk = piece.encode('utf-8', 'strict')
        require(len(data) + len(chunk) <= maximum, 'PDF_LIMIT', 'JSON byte limit')
        data.extend(chunk)
    return bytes(data)


class Resources:
    def __init__(self, root: Path, control):
        self.root = root
        self.control = control
        info = plain_components(root, 'PDF_WORKSPACE')
        require(stat.S_ISDIR(info.st_mode), 'PDF_WORKSPACE', 'workspace is not a plain directory')
        require(not any(root.iterdir()), 'PDF_WORKSPACE', 'workspace is not empty')
        self.root_identity = identity(info)
        self.records = []
        self.pages_created = False
        self.byte_count = 0

    def verify_root(self):
        self.control.check()
        info = plain_components(self.root, 'PDF_WORKSPACE')
        require(stat.S_ISDIR(info.st_mode) and identity(info) == self.root_identity,
                'PDF_WORKSPACE', 'workspace identity changed')

    def write(self, resource_id, relative_path, kind, data):
        self.verify_root()
        limits = self.control.limits
        require(len(self.records) < limits['resources'], 'PDF_LIMIT', 'resource count limit')
        require(len(data) <= limits['resource_bytes'], 'PDF_LIMIT', 'single resource byte limit')
        require(self.byte_count + len(data) <= limits['total_resource_bytes'],
                'PDF_LIMIT', 'total resource byte limit')
        require(relative_path in {'raw.pdf', 'pdf-manifest.json'}
                or re.fullmatch(r'pages/[0-9]{6}\.txt', relative_path) is not None,
                'PDF_RESOURCE', 'invalid fixed resource path')
        target = self.root / relative_path
        if target.parent != self.root:
            if not self.pages_created:
                target.parent.mkdir()
                self.pages_created = True
            info = plain_components(target.parent, 'PDF_WORKSPACE')
            require(stat.S_ISDIR(info.st_mode), 'PDF_WORKSPACE', 'invalid resource parent')
        with target.open('xb') as output:
            for offset in range(0, len(data), 8192):
                self.control.check()
                chunk = data[offset:offset + 8192]
                require(output.write(chunk) == len(chunk), 'PDF_RESOURCE', 'short resource write')
            output.flush()
            os.fsync(output.fileno())
            info = os.fstat(output.fileno())
            require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size == len(data),
                    'PDF_RESOURCE', 'resource changed during write')
        current = plain_components(target, 'PDF_RESOURCE')
        require(identity(current) == identity(info), 'PDF_RESOURCE', 'resource replaced after write')
        # The Core host independently reopens and hashes every declared file.
        record = {'resource_id': resource_id, 'relative_path': relative_path, 'kind': kind,
                  'fingerprint': {'sha256': hashlib.sha256(data).hexdigest(), 'byte_length': len(data)}}
        self.records.append(record)
        self.byte_count += len(data)
        return record


def finite_number(value, description):
    if type(value) not in {int, float} or not math.isfinite(value):
        raise PageFailure('PDF_PAGE_GEOMETRY', f"nonfinite/nonnumeric {description}")
    return float(value)


def raw_box(value, resolve, description):
    value = resolve(value)
    if not isinstance(value, (list, tuple)) or len(value) != 4:
        raise PageFailure('PDF_PAGE_GEOMETRY', f"invalid four-coordinate {description}")
    left, bottom, right, top = [finite_number(resolve(number), description) for number in value]
    return [min(left, right), min(bottom, top), max(left, right), max(bottom, top)]


def geometry_for(raw_page, resolve):
    # Raw attributes avoid pdfminer's silent geometry repairs.
    media = raw_box(raw_page.attrs.get('MediaBox'), resolve, 'MediaBox')
    crop = raw_box(raw_page.attrs.get('CropBox', media), resolve, 'CropBox')
    if not (media[0] < media[2] and media[1] < media[3]
            and media[0] <= crop[0] < crop[2] <= media[2]
            and media[1] <= crop[1] < crop[3] <= media[3]):
        raise PageFailure('PDF_PAGE_GEOMETRY', 'CropBox has no area or exceeds MediaBox')
    rotate = finite_number(resolve(raw_page.attrs.get('Rotate', 0)), 'Rotate')
    if rotate % 90 != 0:
        raise PageFailure('PDF_PAGE_GEOMETRY', 'Rotate is not a multiple of 90')
    rotate = int(rotate) % 360
    scale = finite_number(resolve(raw_page.attrs.get('UserUnit', 1)), 'UserUnit')
    if scale <= 0:
        raise PageFailure('PDF_PAGE_GEOMETRY', 'UserUnit must be positive')
    left, bottom, right, top = crop
    matrices = {
        0: [scale, 0, 0, -scale, -scale * left, scale * top],
        90: [0, scale, scale, 0, -scale * bottom, -scale * left],
        180: [-scale, 0, 0, scale, scale * right, -scale * bottom],
        270: [0, -scale, -scale, 0, scale * top, scale * right],
    }
    width, height = (right - left) * scale, (top - bottom) * scale
    if rotate in {90, 270}:
        width, height = height, width
    if width <= 0 or height <= 0 or not all(
            math.isfinite(number) for number in [width, height, *matrices[rotate]]):
        raise PageFailure('PDF_PAGE_GEOMETRY', 'nonpositive/nonfinite display geometry')
    return {'unit': 'pt', 'origin': 'crop_top_left', 'y_axis': 'down',
            'media_box': media, 'crop_box': crop, 'rotation_degrees': rotate,
            'user_unit': scale, 'width_pt': width, 'height_pt': height,
            'pdf_to_display': matrices[rotate]}


def checked_bbox(box, geometry, description):
    if not all(math.isfinite(number) for number in box):
        raise PageFailure('PDF_PAGE_BBOX', f"nonfinite {description} bbox")
    left, top, right, bottom = box
    if not (0 <= left <= right <= geometry['width_pt']
            and 0 <= top <= bottom <= geometry['height_pt']):
        raise PageFailure('PDF_PAGE_BBOX', f"{description} bbox exceeds CropBox")
    return box


def plumber_bbox(item, page, geometry):
    ml, mb, mr, mt = geometry['media_box']
    cl, cb, cr, ct = geometry['crop_box']
    offsets = {0: (cl - ml, mt - ct), 90: (cb - mb, cl - ml),
               180: (mr - cr, cb - mb), 270: (mt - ct, mr - cr)}
    crop_left, crop_top = offsets[geometry['rotation_degrees']]
    mb_left, mb_top = page.mediabox[:2]
    scale = geometry['user_unit']
    values = [finite_number(item.get(key), 'layout bbox') for key in ('x0', 'top', 'x1', 'bottom')]
    left, top, right, bottom = values
    return checked_bbox([(left - mb_left - crop_left) * scale,
                         (top - mb_top - crop_top) * scale,
                         (right - mb_left - crop_left) * scale,
                         (bottom - mb_top - crop_top) * scale], geometry, 'layout object')


def annotation_bbox(annotation, geometry, resolve):
    left, bottom, right, top = raw_box(annotation.get('Rect'), resolve, 'annotation Rect')
    a, b, c, d, e, f = geometry['pdf_to_display']
    points = [(a * x + c * y + e, b * x + d * y + f)
              for x, y in [(left, bottom), (left, top), (right, bottom), (right, top)]]
    return checked_bbox([min(p[0] for p in points), min(p[1] for p in points),
                         max(p[0] for p in points), max(p[1] for p in points)], geometry, 'annotation')


def group_lines(glyphs, control):
    indices = sorted(range(len(glyphs)), key=lambda index: (
        glyphs[index]['bbox'][1], glyphs[index]['bbox'][0], index))
    lines = []
    anchor = None
    for index in indices:
        control.check()
        top = glyphs[index]['bbox'][1]
        if anchor is None or top - anchor > 3:
            lines.append([])
            anchor = top
        lines[-1].append(index)
    for line in lines:
        control.check()
        line.sort(key=lambda index: (glyphs[index]['bbox'][0], index))
    return lines


def page_evidence(raw_page, pdf, page_index, resolve, Page, control):
    geometry = geometry_for(raw_page, resolve)
    page = Page(pdf, raw_page, page_number=page_index + 1, initial_doctop=0)
    try:
        control.check()
        chars = page.chars
        require(len(chars) <= MAX_PAGE_GLYPHS, 'PDF_LIMIT', 'page glyph inventory exceeds 50000')
        glyphs = []
        flags = {}
        for ordinal, char in enumerate(chars):
            control.check()
            text, font = char.get('text'), char.get('fontname')
            if not isinstance(text, str) or not text or not isinstance(font, str) or not font:
                raise PageFailure('PDF_PAGE_TEXT', 'missing decoded glyph text/font')
            text.encode('utf-8', 'strict')
            font.encode('utf-8', 'strict')
            size = finite_number(char.get('size'), 'glyph font size') * geometry['user_unit']
            if not math.isfinite(size) or size <= 0 or type(char.get('upright')) is not bool:
                raise PageFailure('PDF_PAGE_TEXT', 'invalid glyph size/orientation')
            glyphs.append({'ordinal': ordinal, 'text': text,
                           'bbox': plumber_bbox(char, page, geometry), 'font_name': font,
                           'font_size': size, 'upright': char['upright']})
            if len(text) > 1:
                flags['PDF_MULTICODEPOINT_GLYPH'] = 'Glyph decodes to multiple codepoints; text/font metrics require review.'
            if re.search(r'symbol|math|cmsy|cmmi|cmex|stix|msam|msbm|cid', font, re.IGNORECASE) \
                    or '(cid:' in text.lower() or '\ufffd' in text \
                    or any(ord(character) < 32 and not character.isspace() for character in text):
                flags['PDF_TEXT_ENCODING'] = 'Formula font/undecoded text requires review.'
            if not char['upright']:
                flags['PDF_TEXT_ORIENTATION'] = 'Nonhorizontal glyphs require review.'
        visuals = []
        for kind, objects in [('image', page.images), ('vector', page.rects),
                              ('vector', page.lines), ('vector', page.curves)]:
            for item in objects:
                control.check()
                visuals.append({'visual_id': f"visual-{page_index:06d}-{len(visuals):06d}",
                                'kind': kind, 'physical_page_index': page_index,
                                'bbox': plumber_bbox(item, page, geometry), 'evidence_resource_id': 'raw-pdf'})
        annotations = resolve(raw_page.attrs.get('Annots')) or []
        if not isinstance(annotations, list):
            raise PageFailure('PDF_PAGE_ANNOTATION', 'invalid annotation list')
        for reference in annotations:
            control.check()
            annotation = resolve(reference)
            if not isinstance(annotation, dict):
                raise PageFailure('PDF_PAGE_ANNOTATION', 'invalid annotation object')
            visuals.append({'visual_id': f"visual-{page_index:06d}-{len(visuals):06d}",
                            'kind': 'annotation', 'physical_page_index': page_index,
                            'bbox': annotation_bbox(annotation, geometry, resolve),
                            'evidence_resource_id': 'raw-pdf'})
        if visuals:
            flags['PDF_VISUAL_CONTENT'] = 'Raw image/vector/annotation evidence is retained; structure/association requires review.'
        if not glyphs or all(glyph['text'].isspace() for glyph in glyphs):
            if not glyphs and any(visual['kind'] == 'image' for visual in visuals):
                flags['PDF_NO_TEXT_LAYER'] = 'Raster page has no decoded text; review required; OCR unavailable.'
            else:
                flags['PDF_UNKNOWN_BLANK'] = 'No non-whitespace decoded text; valid blank status remains unconfirmed.'
        if geometry['rotation_degrees'] != 0:
            flags['PDF_PAGE_ROTATION'] = 'Rotated page coordinates are retained; reading order requires review.'
        if geometry['user_unit'] != 1:
            flags['PDF_PAGE_USER_UNIT'] = 'Nondefault UserUnit converted to pt; interpretation requires review.'
        text_parts, regions = [], []
        byte_offset = 0
        resource_id = f"page-text-{page_index:06d}"
        for line in group_lines(glyphs, control):
            control.check()
            parts = []
            previous = None
            for index in line:
                glyph = glyphs[index]
                if previous is not None:
                    gap = glyph['bbox'][0] - previous['bbox'][2]
                    if gap > 3 and not previous['text'][-1].isspace() and not glyph['text'][0].isspace():
                        parts.append(' ')
                    if gap > 36:
                        flags['PDF_LAYOUT_GAP'] = 'Large horizontal gap: possible columns/table; order requires review.'
                    if gap < -0.000001 and min(glyph['bbox'][3], previous['bbox'][3]) \
                            > max(glyph['bbox'][1], previous['bbox'][1]):
                        flags['PDF_GLYPH_OVERLAP'] = 'Overlapping glyph boxes require review.'
                parts.append(glyph['text'])
                previous = glyph
            sizes = [glyphs[index]['font_size'] for index in line]
            if max(sizes) - min(sizes) > 0.000001:
                flags['PDF_MIXED_FONT_SIZE'] = 'Mixed line font sizes: possible superscript/formula; review required.'
            text = ''.join(parts)
            length = len(text.encode('utf-8', 'strict'))
            bounds = [glyphs[index]['bbox'] for index in line]
            regions.append({'region_id': f"region-{page_index:06d}-{len(regions):06d}",
                            'locator_kind': 'pdf_page_region', 'physical_page_index': page_index,
                            'bbox': [min(box[0] for box in bounds), min(box[1] for box in bounds),
                                     max(box[2] for box in bounds), max(box[3] for box in bounds)],
                            'text': {'resource_id': resource_id, 'resource_sha256': '',
                                     'start_byte': byte_offset, 'end_byte': byte_offset + length},
                            'glyph_indices': line})
            byte_offset += length + 1
            text_parts.append(text + '\n')
        for first, second in zip(regions, regions[1:]):
            control.check()
            first_box, second_box = first['bbox'], second['bbox']
            horizontal_gap = max(second_box[0] - first_box[2], first_box[0] - second_box[2])
            top_delta = second_box[1] - first_box[1]
            line_height = max(first_box[3] - first_box[1], second_box[3] - second_box[1])
            if horizontal_gap > 36 and 0 <= top_delta <= max(72, 6 * line_height):
                flags['PDF_POSSIBLE_COLUMNS'] = 'Adjacent separated lines: possible columns/side annotations; order requires review.'
        text_bytes = ''.join(text_parts).encode('utf-8', 'strict')
        digest = hashlib.sha256(text_bytes).hexdigest()
        for region in regions:
            control.check()
            region['text']['resource_sha256'] = digest
        return ({'physical_page_index': page_index, 'display_page_number': page_index + 1,
                 'printed_page_label': None, 'status': 'needs_review' if flags else 'parsed',
                 'geometry': geometry, 'text': {'resource_id': resource_id, 'sha256': digest,
                                                'byte_length': len(text_bytes)},
                 'glyphs': glyphs, 'regions': regions, 'visuals': visuals}, text_bytes, flags)
    finally:
        page.close()


def failed_page(index):
    return {'physical_page_index': index, 'display_page_number': index + 1,
            'printed_page_label': None, 'status': 'failed', 'geometry': None, 'text': None,
            'glyphs': [], 'regions': [], 'visuals': []}


def parse_pdf(data, actual, request, resources, control):
    # Import only after exact pin verification.
    logging.disable(logging.CRITICAL)
    warnings.simplefilter('ignore')
    import pdfplumber
    from pdfminer.pdfpage import PDFPage
    from pdfminer.pdftypes import resolve1
    from pdfplumber.page import Page

    control.check()
    issues = []
    pages, units, raw_pages = [], [], []
    enumeration_complete = False
    enumeration_failure = None
    pdf = None
    stream = io.BytesIO(data)
    try:
        if not data.startswith(b'%PDF-'):
            enumeration_failure = ('PDF_OPEN', 'Selected bytes are not a supported PDF.')
        else:
            try:
                pdf = pdfplumber.open(stream)
                if pdf.doc.encryption is not None:
                    enumeration_failure = ('PDF_ENCRYPTED', 'Encrypted PDF unsupported; no password supplied.')
                elif not pdf.doc.is_extractable:
                    enumeration_failure = ('PDF_PERMISSION', 'PDF extraction permission unavailable.')
                else:
                    generator = PDFPage.create_pages(pdf.doc)
                    while True:
                        control.check()
                        try:
                            raw_page = next(generator)
                        except StopIteration:
                            enumeration_complete = True
                            break
                        require(len(raw_pages) < MAX_PAGES, 'PDF_LIMIT', 'PDF has more than 128 physical pages')
                        raw_pages.append(raw_page)
            except AdapterFailure:
                raise
            except Exception as error:
                enumeration_failure = ('PDF_ENUMERATION', 'Open/page enumeration failed: ' + type(error).__name__)

        unit_count = len(raw_pages) + (0 if enumeration_complete and raw_pages else 1)
        require(unit_count <= control.limits['units'], 'PDF_LIMIT', 'page/document unit count limit')

        def add_issue(code, message, unit_id, references):
            control.check()
            require(len(issues) < control.limits['issues'], 'PDF_LIMIT', 'issue count limit')
            issues.append({'code': code, 'severity': 'blocking', 'message': message,
                           'unit_ids': [unit_id], 'resource_ids': references})

        total_glyphs = 0
        for page_index, raw_page in enumerate(raw_pages):
            control.check()
            unit_id = f"page-{page_index:06d}"
            try:
                page, text_bytes, flags = page_evidence(raw_page, pdf, page_index, resolve1, Page, control)
            except AdapterFailure:
                raise
            except Exception as error:
                page, text_bytes, flags = failed_page(page_index), None, {}
                code = error.code if isinstance(error, PageFailure) else 'PDF_PAGE_PARSE'
                message = str(error) if isinstance(error, PageFailure) else 'Page extraction failed: ' + type(error).__name__
                add_issue(code, message, unit_id, ['raw-pdf', 'pdf-manifest'])
            total_glyphs += len(page['glyphs'])
            require(total_glyphs <= MAX_TOTAL_GLYPHS, 'PDF_LIMIT', 'total glyph inventory exceeds 200000')
            references = ['raw-pdf', 'pdf-manifest']
            text = None
            if text_bytes is not None:
                resource_id = page['text']['resource_id']
                resources.write(resource_id, f"pages/{page_index:06d}.txt", 'utf8_text', text_bytes)
                references.append(resource_id)
                if text_bytes:
                    text = {'resource_id': resource_id, 'start_byte': 0, 'end_byte': len(text_bytes)}
                for code, message in flags.items():
                    add_issue(code, message, unit_id, list(references))
            pages.append(page)
            units.append({'unit_id': unit_id, 'ordinal': page_index,
                          'kind': 'text' if text is not None else 'unknown', 'status': page['status'],
                          'text': text, 'resource_ids': references})
        if not enumeration_complete or not raw_pages:
            units.append({'unit_id': 'pdf-document', 'ordinal': len(raw_pages), 'kind': 'unknown',
                          'status': 'failed', 'text': None, 'resource_ids': ['raw-pdf', 'pdf-manifest']})
            code, message = enumeration_failure or ('PDF_NO_PAGES', 'No physical pages enumerated.')
            add_issue(code, message, 'pdf-document', ['raw-pdf', 'pdf-manifest'])
        status = 'failed' if any(unit['status'] == 'failed' for unit in units) else (
            'needs_review' if issues or any(page['status'] == 'needs_review' for page in pages) else 'parsed')
        manifest = {'schema_version': 1, 'format': 'pdf-page-regions-v1', 'input_fingerprint': actual,
                    'parser': dict(PARSER_IDENTITY), 'raw_resource_id': 'raw-pdf',
                    'page_count': len(raw_pages) if enumeration_complete else None,
                    'status': status, 'pages': pages, 'resources': list(resources.records), 'issues': issues}
        maximum = min(control.limits['resource_bytes'], control.limits['total_resource_bytes'] - resources.byte_count)
        resources.write('pdf-manifest', 'pdf-manifest.json', 'utf8_text', bounded_json(manifest, maximum, control))
        return {'protocol_version': 1, 'request_id': request['request_id'], 'input_fingerprint': actual,
                'workspace_capability_id': request['workspace']['capability_id'], 'producer': 'parser',
                'parser': {'name': 'pdfplumber', 'version': '0.11.9', 'capability_version': 1,
                           'capabilities': ['source_units_v1']},
                'unit_count': len(units), 'units': units, 'resources': resources.records, 'issues': issues}
    finally:
        # PDF.close() would reconstruct all pages, including failed geometry.
        if pdf is not None:
            pdf.flush_cache()
        stream.close()


def main():
    request = read_request()
    control = Control(request['limits'])
    check_runtime(control)
    resources = Resources(selected_path(request['workspace']['path'], 'workspace.path'), control)
    data, actual = read_selected(request, control)
    resources.write('raw-pdf', 'raw.pdf', 'binary', data)
    response = parse_pdf(data, actual, request, resources, control)
    output = bounded_json(response, control.limits['response_bytes'], control)
    resources.verify_root()
    control.check()
    sys.stdout.buffer.write(output)
    sys.stdout.buffer.flush()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        code = error.code if isinstance(error, AdapterFailure) else 'PDF_ADAPTER'
        message = str(error) if isinstance(error, AdapterFailure) else type(error).__name__
        diagnostic = (code + ': ' + message + '\n').encode('utf-8', 'replace')[:DIAGNOSTIC_LIMIT]
        sys.stderr.buffer.write(diagnostic)
        sys.stderr.buffer.flush()
        raise SystemExit(2) from None
