"""Create the reviewed ASCII PDF family without using an extraction library.

Writes only missing outputs beside this script. Expected text and positions come
from literal author instructions below; no adapter output is read or accepted.
"""

from datetime import datetime, timezone
import hashlib
from io import BytesIO
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont
from reportlab.lib.pdfencrypt import StandardEncryption
from reportlab.pdfgen.canvas import Canvas


ROOT = Path(__file__).resolve().parent
DEFAULT_BOX = [0, 0, 612, 792]
FONT_ADVANCE = 7.2


def digest(data):
    return hashlib.sha256(data).hexdigest()


def exclusive(path, data):
    with path.open("xb") as stream:
        stream.write(data)


def number(value):
    return format(value, ".8g")


def transform_point(x, y, crop, rotation, unit):
    left, bottom, right, top = crop
    point = {
        0: (x - left, top - y),
        90: (y - bottom, x - left),
        180: (right - x, y - bottom),
        270: (top - y, right - x),
    }[rotation]
    return [round(v * unit, 8) for v in point]


def line(text, x=None, y=None, matrix=None):
    assert text.isascii()
    return {"text": text, "matrix": matrix or [1, 0, 0, 1, x, y]}


def page(lines=(), *, media=None, crop=None, rotation=0, unit=1,
         status="parsed", exact=True, drawing="", image=None, bad_filter=False):
    return {"lines": list(lines), "media": media or DEFAULT_BOX,
            "crop": crop or media or DEFAULT_BOX, "rotation": rotation,
            "unit": unit, "status": status, "exact": exact,
            "drawing": drawing, "image": image, "bad_filter": bad_filter}


class PdfAuthor:
    def __init__(self):
        self.objects = [b"", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>"]
        self.page_ids = []

    def add(self, data):
        self.objects.append(data)
        return len(self.objects)

    def stream(self, data, attrs=""):
        return self.add((f"<< /Length {len(data)} {attrs} >>\nstream\n").encode("ascii") + data + b"\nendstream")

    def add_page(self, spec):
        commands = []
        for item in spec["lines"]:
            text = item["text"].replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
            matrix = " ".join(number(v) for v in item["matrix"])
            commands.append(f"BT /F1 12 Tf 0 Tc 0 Tw 100 Tz {matrix} Tm ({text}) Tj ET\n")
        commands.append(spec["drawing"])
        resources = "/Font << /F1 3 0 R >>" if spec["lines"] else ""
        if spec["image"]:
            img = spec["image"]
            image_id = self.stream(img["pixels"], f'/Type /XObject /Subtype /Image /Width {img["width"]} /Height {img["height"]} /ColorSpace /DeviceRGB /BitsPerComponent 8')
            resources += f" /XObject << /Im1 {image_id} 0 R >>"
            x0, y0, x1, y1 = img["bbox"]
            commands.append(f"q {x1-x0} 0 0 {y1-y0} {x0} {y0} cm /Im1 Do Q\n")
        data = "".join(commands).encode("ascii")
        if spec["bad_filter"]:
            data = b"BT /F1 12 Tf 72 720 Td (DAMAGED CONTENT) Tj ET\n"
        content_id = self.stream(data, "/Filter /B03FixtureUnsupported" if spec["bad_filter"] else "")
        media = " ".join(number(v) for v in spec["media"])
        crop = " ".join(number(v) for v in spec["crop"])
        page_id = self.add((f"<< /Type /Page /Parent 2 0 R /MediaBox [{media}] /CropBox [{crop}] /Rotate {spec['rotation']} /UserUnit {spec['unit']} /Resources << {resources} >> /Contents {content_id} 0 R >>").encode("ascii"))
        self.page_ids.append(page_id)

    def finish(self):
        self.objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>"
        kids = " ".join(f"{v} 0 R" for v in self.page_ids)
        self.objects[1] = f"<< /Type /Pages /Count {len(self.page_ids)} /Kids [{kids}] >>".encode("ascii")
        result = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = [0]
        for i, item in enumerate(self.objects, start=1):
            offsets.append(len(result))
            result.extend(f"{i} 0 obj\n".encode("ascii") + item + b"\nendobj\n")
        xref = len(result)
        result.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode("ascii"))
        for offset in offsets[1:]:
            result.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
        result.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode("ascii"))
        return bytes(result)


def authored_anchor(item, spec):
    a, b, c, d, e, f = item["matrix"]
    length = FONT_ADVANCE * len(item["text"])
    points = []
    for x in (-1, length + 1):
        for y in (-4, 12):
            raw_x, raw_y = a*x + c*y + e, b*x + d*y + f
            points.append(transform_point(raw_x, raw_y, spec["crop"], spec["rotation"], spec["unit"]))
    xs, ys = zip(*points)
    box = [min(xs), min(ys), max(xs), max(ys)]
    baseline = transform_point(e, f, spec["crop"], spec["rotation"], spec["unit"])
    # Endpoint windows apply to displayed horizontal text only; rotated vertical
    # material keeps the independently mapped full envelope for visual review.
    horizontal = spec["rotation"] == 0 and (a,b,c,d) == (1,0,0,1) or (
        spec["rotation"] == 90 and (a,b,c,d) == (0,1,-1,0)) or (
        spec["rotation"] == 180 and (a,b,c,d) == (-1,0,0,-1)) or (
        spec["rotation"] == 270 and (a,b,c,d) == (0,-1,1,0))
    windows = None
    if horizontal:
        x, y = baseline
        u = spec["unit"]
        windows = {"left": [x-u, x+u], "top": [y-12*u, y-6*u],
                   "right": [x+length*u-u, x+length*u+u], "bottom": [y, y+4*u]}
    return {"excerpt_utf8": item["text"], "author_text_matrix": item["matrix"],
            "font": "Courier", "font_size_raw": 12, "char_advance_raw": 7.2,
            "display_baseline_pt": baseline, "display_envelope_pt": box,
            "endpoint_windows_pt": windows}


def page_expected(spec, index):
    media, crop, rotation, unit = spec["media"], spec["crop"], spec["rotation"], spec["unit"]
    width, height = crop[2]-crop[0], crop[3]-crop[1]
    if rotation in (90,270):
        width,height = height,width
    result = {"physical_page_index": index, "displayed_physical_page": index+1,
              "printed_page_label": None, "expected_status": spec["status"],
              "raw_media_box": media, "raw_crop_box": crop, "rotation": rotation,
              "user_unit": unit, "display_size_pt": [width*unit,height*unit],
              "expected_glyph_count": None if spec["bad_filter"] else sum(len(v["text"]) for v in spec["lines"]),
              "authored_glyph_stream_utf8": None if spec["bad_filter"] else "".join(v["text"] for v in spec["lines"]),
              "anchors": [authored_anchor(v,spec) for v in spec["lines"]], "text": None,
              "visual_objects_expected": bool(spec["drawing"] or spec["image"])}
    if spec["exact"] and not spec["bad_filter"]:
        value = "\n".join(v["text"] for v in spec["lines"])
        if spec["lines"]:
            value += "\n"
        data = value.encode("utf-8")
        ranges=[]
        start=0
        for ordinal,item in enumerate(spec["lines"]):
            raw=item["text"].encode("utf-8")
            ranges.append({"ordinal":ordinal,"start_byte":start,"end_byte":start+len(raw),"excerpt_utf8":item["text"],"excerpt_sha256":digest(raw)})
            start += len(raw)+1
        result["text"] = {"utf8":value,"byte_length":len(data),"sha256":digest(data),"line_ranges":ranges}
    if spec["image"]:
        image=spec["image"]
        result["image"]={"pixel_width":image["width"],"pixel_height":image["height"],
                         "raw_pixel_sha256":digest(image["pixels"]),"raw_placement_box":image["bbox"]}
    return result


def main():
    cases=[]
    def add(case_id,name,pages,label="ready",category=None,special=None):
        cases.append({"case_id":case_id,"name":name,"pages":pages,"label":label,"category":category,"special":special})
    ys=[720,684,648,612]
    add("N01","single-column.pdf",[page([line(v,72,y) for v,y in zip([
        "SINGLE ALPHA","One column has four lines.","Heading 99 is not a page label.","SINGLE OMEGA"],ys)])])
    multi=[
        ["MULTI ALPHA START","The first leaf begins here.","Keep this before the second leaf.","MULTI ALPHA END"],
        ["MULTI BETA START","The middle leaf is required.","Its heading says Chapter 17.","MULTI BETA END"],
        ["MULTI GAMMA START","The last leaf is also required.","Keep this after the middle leaf.","MULTI GAMMA END"]]
    add("N02","multi-page-single-column.pdf",[page([line(v,72+24*i,y) for v,y in zip(values,ys)]) for i,values in enumerate(multi)])
    image=Image.new("RGB",(720,240),"white")
    draw=ImageDraw.Draw(image)
    font=ImageFont.load_default(size=38)
    draw.text((28,38),"SCAN ONLY ALPHA",font=font,fill="black")
    draw.text((28,128),"THIS IS RASTER TEXT",font=font,fill="black")
    scan={"pixels":image.tobytes(),"width":720,"height":240,"bbox":[72,540,432,660]}
    add("E01","scan-only.pdf",[page(image=scan,status="needs_review",exact=False)],"needs_review","no_text_layer")
    add("E02","blank.pdf",[page(status="needs_review")],"needs_review","empty_page")
    add("E03","damaged.pdf",[],"failed","document_invalid",special="damaged")
    add("E04","encrypted.pdf",[page([line("ENCRYPTED ALPHA",72,720)])],"failed","encrypted",special="encrypted")
    table=[line("TABLE SAMPLE",72,720),line("HEADER LEFT",84,644),line("HEADER RIGHT",228,644),line("ALPHA",84,604),line("10",228,604),line("BETA",84,564),line("20",228,564),line("Cells need a structure review.",72,510)]
    grid="1 w\n"+"".join(f"{x} 552 m {x} 672 l S\n" for x in (72,216,360))+"".join(f"72 {y} m 360 {y} l S\n" for y in (552,592,632,672))
    add("C01","table.pdf",[page(table,drawing=grid,status="needs_review",exact=False)],"needs_review","visual_structure")
    formula=[line("FORMULA SAMPLE",72,720),line("a + b",96,648),line("c",118,624),line("The fraction needs visual grouping.",72,576)]
    add("C02","formula.pdf",[page(formula,drawing="1 w 90 642 m 156 642 l S\n",status="needs_review",exact=False)],"needs_review","visual_structure")
    rgb=bytes((255,0,0)*32+(0,0,255)*32)*32
    picture={"pixels":rgb,"width":64,"height":32,"bbox":[72,576,216,648]}
    add("C03","image.pdf",[page([line("IMAGE SAMPLE",72,720),line("The picture has two squares.",72,540)],image=picture,status="needs_review")],"needs_review","visual_structure")
    columns=[line("TWO COLUMN SAMPLE",72,720),line("RIGHT SECOND",330,660),line("LEFT FIRST",72,684),line("RIGHT FIRST",330,684),line("LEFT THIRD",72,636),line("LEFT SECOND",72,660),line("RIGHT THIRD",330,636)]
    add("C04","two-columns.pdf",[page(columns,status="needs_review",exact=False)],"needs_review","reading_order")
    crop=[72,48,540,720]
    first=["ROTATE ZERO ALPHA","ROTATE NINETY BETA","ROTATE HALF GAMMA","ROTATE TWOSEVENTY DELTA"]
    ending=["ROTATE ZERO END","ROTATE NINETY END","ROTATE HALF END","ROTATE TWOSEVENTY END"]
    matrices=[([1,0,0,1,144,624],[1,0,0,1,144,588]),([0,1,-1,0,168,120],[0,1,-1,0,204,120]),([-1,0,0,-1,468,144],[-1,0,0,-1,468,180]),([0,-1,1,0,444,648],[0,-1,1,0,408,648])]
    add("P01","rotation-upright.pdf",[page([line(first[i],matrix=matrices[i][0]),line(ending[i],matrix=matrices[i][1])],crop=crop,rotation=r) for i,r in enumerate((0,90,180,270))])
    media=[-20,30,592,822]
    crop=[40,90,520,750]
    add("P02","crop-origin.pdf",[
        page([line("CROP ZERO ALPHA",88,690),line("CROP ZERO BETA",112,654)],media=media,crop=crop),
        page([line("CROP NINETY ALPHA",matrix=[0,1,-1,0,100,138]),line("CROP NINETY BETA",matrix=[0,1,-1,0,136,162])],media=media,crop=crop,rotation=90)])
    add("P03","rotation-vertical.pdf",[page([line("VERTICAL DISPLAY ALPHA",88,690),line("VERTICAL DISPLAY BETA",112,654)],media=media,crop=crop,rotation=90,status="needs_review",exact=False)],"needs_review","text_direction")
    add("P04","userunit-two.pdf",[page([line("USER UNIT TWO",72,720)],unit=2)])
    add("F01","mixed-page-failure.pdf",[
        page([line("MIXED ALPHA START",72,720),line("Before the damaged content.",72,684)]),
        page(status="failed",exact=False,bad_filter=True),
        page([line("MIXED GAMMA END",120,720),line("After the damaged content.",120,684)])],"needs_review","page_decode")
    assert len(cases)==15
    outputs=[ROOT/case["name"] for case in cases]+[ROOT/"expected.json",ROOT/"freeze.json",ROOT/"scan-source.png"]
    if any(path.exists() for path in outputs):
        raise FileExistsError("The immutable fixture outputs already exist; author refuses to overwrite.")
    scan_buffer=BytesIO()
    image.save(scan_buffer,format="PNG")
    exclusive(ROOT/"scan-source.png",scan_buffer.getvalue())
    manifest={"schema_version":1,"fixture_family":"pdf-text-v1","source_license":"self_created_public_test_material",
              "text_format_version":"page-lf-final-v1","coordinate_contract":"displayed-crop-local-top-left-pt-v1",
              "authoring_script_sha256":digest(Path(__file__).read_bytes()),"cases":[]}
    for case in cases:
        if case["special"]=="damaged":
            raw=b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
        elif case["special"]=="encrypted":
            buffer=BytesIO()
            canvas=Canvas(buffer,pagesize=(612,792),invariant=1,pageCompression=0,encrypt=StandardEncryption("fixture-open",ownerPassword="fixture-owner",strength=128))
            canvas.setFont("Courier",12)
            canvas.drawString(72,720,"ENCRYPTED ALPHA")
            canvas.showPage()
            canvas.save()
            raw=buffer.getvalue()
        else:
            author=PdfAuthor()
            for spec in case["pages"]:
                author.add_page(spec)
            raw=author.finish()
        exclusive(ROOT/case["name"],raw)
        unconfirmed=case["special"] in ("damaged","encrypted")
        label=case["label"]
        manifest["cases"].append({"case_id":case["case_id"],"raw":{"relative_path":case["name"],"byte_length":len(raw),"sha256":digest(raw)},
                                  "designed_page_count":None if case["special"]=="damaged" else len(case["pages"]),
                                  "expected_confirmed_page_count":None if unconfirmed else len(case["pages"]),
                                  "expected_label":label,"expected_exit_code":0 if label=="ready" else 3,
                                  "expected_response_status":"failed" if unconfirmed else "complete" if label=="ready" else "partial",
                                  "expected_issue_category":case["category"],
                                  "pages":[] if unconfirmed else [page_expected(spec,i) for i,spec in enumerate(case["pages"])],
                                  "independent_designed_pages":[page_expected(spec,i) for i,spec in enumerate(case["pages"])] if unconfirmed else None})
    encoded=(json.dumps(manifest,ensure_ascii=False,indent=2)+"\n").encode("utf-8")
    exclusive(ROOT/"expected.json",encoded)
    freeze={"frozen_at_utc":datetime.now(timezone.utc).isoformat(),"before_first_adapter_extraction":True,
            "expected_sha256":digest(encoded),"authoring_script_sha256":manifest["authoring_script_sha256"],
            "pdf_count":len(cases),"raw_files":[case["raw"] for case in manifest["cases"]]}
    exclusive(ROOT/"freeze.json",(json.dumps(freeze,indent=2)+"\n").encode())
    print(json.dumps(freeze,indent=2))


if __name__=="__main__":
    main()
