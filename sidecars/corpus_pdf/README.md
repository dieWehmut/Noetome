# Corpus PDF worker v1

`worker.py` is a standalone resource for the locked Python 3.12.14,
pdfplumber 0.11.9 / pdfminer.six 20251230 runtime. It has no sibling Python
module dependency and does not import the bounded historical fixture adapter.

```text
python worker.py preflight --input SNAPSHOT.pdf --output-dir OUTPUT
python worker.py pages --input SNAPSHOT.pdf --output-dir OUTPUT --start-page 1 --end-page 8
```

The host supplies its immutable PDF snapshot, invokes argv without a shell,
owns the 180-second batch timeout/cancellation, and persists confirmed pages.
Ranges are inclusive and 1-based. An omitted end page selects eight pages from
the start; a short final batch ends at the actual last page. Explicit ranges
have no book/page-count cap. Restarting a range atomically replaces the same
page files; unrelated completed pages remain untouched.

Stdout is UTF-8 JSON Lines: `preflight_progress` (every 128 enumerated pages),
`preflight`, `page_written`, `page_failed`, `completed`, or `error`. Result paths
are relative to OUTPUT. Preflight writes `preflight.json`; pages write
`pages/000001.json`, etc. File/runtime failures exit 2. Isolated failed pages
still produce page records and a completed batch with failure counts, exit 0.
The host must inspect each page status and must not equate exit 0 with every
page being readable.

Preflight records byte length, physical page count, encryption/password state,
the first three page classifications without source text, available outline
titles/destinations, parser identity, issues, and `limits`. File bytes and page
counts have no imposed cap. A password-protected PDF that requires a password
is explicit `password_required`; password input is not implemented here.
An invalid page tree fails the document inventory rather than inventing a count.

Each page has `schema_version`, 1-based `page_number`, `width_pt`, `height_pt`,
`rotation`, `geometry`, `status`, `text`, `regions`, `issues`, and `parser`.
Regions contain stable local `ordinal`, `kind` (`text`, `image`, `vector`), text,
and bbox `[left, top, right, bottom]`. Physical page numbers include blank and
failed pages. Failed geometry has null dimensions/geometry. A page JSON larger
than 16 MiB becomes an explicit `PDF_PAGE_JSON_LIMIT` failed page; text is never
silently truncated. Writes use a same-directory temporary file, flush/fsync,
and atomic replacement; an interrupted write cannot replace a completed page
with partial JSON. The host may remove abandoned temporary files in its own
worker directory during recovery.

`geometry.media_box` and `crop_box` retain original PDF coordinates before
UserUnit scaling. Region coordinates and page dimensions are physical points
after CropBox translation, clockwise page rotation, and UserUnit scaling,
with origin at the displayed CropBox's top left and y increasing downwards.
`pdf_to_display` is `[a,b,c,d,e,f]` where `x'=a*x+c*y+e`, `y'=b*x+d*y+f`.
Fully out-of-crop objects are omitted; partially clipped text raises an issue.

`text_ready` means the text layer was extracted without the worker's known
quality flags; it is not a scientific/semantic validation. Empty visual-free
pages are `blank`. Image-only pages, or a large image with fewer than 80 text
characters (such as an isolated page number), are `needs_ocr`. Mixed graphics,
formula fonts, unresolved text encoding, rotated text, possible columns/tables,
and annotations are `needs_review`, with extracted text retained. These are
conservative heuristics, not guarantees of reading order or complete layout
recognition. OCR is supplied by the separate host-selected OCR worker.

The worker opens the selected file read-only and seeks through it; it never
copies it into a whole-file bytes buffer. It never builds pdfplumber's all-page
list and closes each page's extraction cache. Decoded object/font caching is
disabled. Memory still includes PDF xref/page-tree metadata and the current
page's decoded objects; a very complex single page can require substantial
memory before its output-size check. Host process isolation and page-level
retry/recovery remain necessary. No whole-book glyph arrays or raster images
are saved in the page JSON.
