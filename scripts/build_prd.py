"""Build an offline HTML reading edition from the Noetome Markdown source.

Uses only the Python standard library. The Markdown source remains
canonical; review the generated HTML in a browser before delivery.
"""

from __future__ import annotations

import argparse
import html
import re
from pathlib import Path


INLINE = re.compile(
    r"(\*\*.+?\*\*|`[^`]+`|\[[^\]]+\]\([^\s)]+\)|(?<!\*)\*[^*]+\*(?!\*))"
)


def fence_open(line):
    match = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
    return match.group(1) if match else None


def read_fenced_block(lines, index):
    marker = fence_open(lines[index])
    assert marker is not None
    closing = re.compile(r"^ {0,3}" + re.escape(marker[0]) + "{" + str(len(marker)) + r",}\s*$")
    code = []
    next_index = index + 1
    while next_index < len(lines):
        if closing.fullmatch(lines[next_index]):
            return code, next_index + 1
        code.append(lines[next_index])
        next_index += 1
    raise ValueError(f"Unclosed code fence at source line {index + 1}")


def main_headings(lines):
    """Exclude example headings inside code fences from the navigation."""
    entries = []
    index = 0
    while index < len(lines):
        if fence_open(lines[index]):
            _, index = read_fenced_block(lines, index)
            continue
        match = re.match(r"^##\s+(.+)$", lines[index])
        if match:
            entries.append((index, match.group(1)))
        index += 1
    return entries


def split_cells(line):
    content = line.strip().strip("|")
    result, current, in_code, escaped = [], [], False, False
    for char in content:
        if escaped:
            current.extend(("\\", char))
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == "`":
            in_code = not in_code
            current.append(char)
        elif char == "|" and not in_code:
            result.append("".join(current).strip())
            current = []
        else:
            current.append(char)
    if escaped:
        current.append("\\")
    result.append("".join(current).strip())
    return result


def html_inline(text):
    parts = []
    text = text.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n")
    for token in INLINE.split(text):
        if not token:
            continue
        if token.startswith("[") and "](" in token and token.endswith(")"):
            label, url = token[1:-1].split("](", 1)
            local_link = (not re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", url)
                          and not url.startswith(("/", "\\")) and "\\" not in url)
            if not (re.match(r"^(?:https?://|#)", url) or local_link):
                parts.append(html.escape(label))
            else:
                parts.append('<a href="' + html.escape(url, quote=True) + '">' + html.escape(label) + '</a>')
        elif token.startswith("**") and token.endswith("**"):
            parts.append("<strong>" + html.escape(token[2:-2]) + "</strong>")
        elif token.startswith("`") and token.endswith("`"):
            parts.append("<code>" + html.escape(token[1:-1]) + "</code>")
        elif token.startswith("*") and token.endswith("*"):
            parts.append("<em>" + html.escape(token[1:-1]) + "</em>")
        else:
            parts.append(html.escape(token.replace("\\|", "|")))
    return "".join(parts).replace("\n", "<br>")


HTML_CSS = """
:root { color-scheme: light; --ink:#17212b; --muted:#526373; --line:#d9dfe4; }
* { box-sizing:border-box; }
html { scroll-behavior:smooth; scroll-padding-top:30px; }
body { margin:0; color:var(--ink); background:#f4f6f7;
  font:16px/1.85 'Microsoft YaHei','Noto Sans SC',system-ui,sans-serif; }
a { color:#25577a; text-decoration:none; }
a:hover { text-decoration:underline; }
.layout { max-width:1450px; margin:auto; display:grid; grid-template-columns:255px minmax(0,1fr); gap:28px; padding:32px 28px 64px; }
.navigation { position:sticky; top:28px; max-height:calc(100vh - 56px); overflow-y:auto;
  align-self:start; font-size:13px; line-height:1.65; padding:4px 12px 16px 0; }
.navigation strong { display:block; color:#000; font-size:17px; margin-bottom:12px; }
.navigation a { display:block; padding:5px 0; color:#485969; }
.navigation a:hover { color:#111; }
.print-button { border:1px solid #cad3da; border-radius:4px; padding:8px 14px; margin:0 0 20px;
  background:white; color:#263d50; font:inherit; cursor:pointer; }
main { min-width:0; background:white; border:1px solid #e0e5e9; padding:48px 54px; }
h1,h2,h3,h4,h5,h6 { color:#000; line-height:1.45; font-weight:700; }
h1 { font-size:31px; margin:0 0 26px; letter-spacing:.2px; }
h2 { font-size:24px; margin:46px 0 18px; }
h3 { font-size:19px; margin:30px 0 13px; }
h4,h5,h6 { font-size:16px; margin:24px 0 10px; }
p { margin:0 0 14px; overflow-wrap:anywhere; }
.list-item { position:relative; padding-left:1.5em; margin-bottom:7px; }
.list-item .marker { position:absolute; left:0; }
.table-scroll { overflow-x:auto; margin:19px 0 22px; }
table { width:100%; border-collapse:collapse; font-size:14px; line-height:1.65; }
th,td { border:1px solid var(--line); padding:10px 12px; vertical-align:middle; overflow-wrap:anywhere; }
th { color:#000; background:#e6edf2; font-weight:600; text-align:left; }
td { background:#fff; }
code { font:0.91em/1.5 Consolas,'Microsoft YaHei',monospace; overflow-wrap:anywhere; }
pre { white-space:pre-wrap; overflow-wrap:anywhere; font-size:13px; line-height:1.65; margin:18px 0 22px; }
pre code { font-size:inherit; }
blockquote { margin:16px 0; }
.source-line { color:var(--muted); font-size:14px; }
@media (max-width:1100px) {
  .layout { grid-template-columns:210px minmax(0,1fr); gap:18px; padding:22px 18px 48px; }
  main { padding:34px; }
}
@media (max-width:800px) {
  .layout { display:block; padding:16px; }
  .navigation { position:static; max-height:250px; margin-bottom:22px; }
  main { padding:28px 22px; }
  h1 { font-size:25px; } h2 { font-size:21px; }
  table { min-width:640px; }
}
@media print {
  @page { size:A4; margin:18mm 17mm; }
  body { background:#fff; font-size:10.5pt; line-height:1.5; }
  .layout { display:block; padding:0; }
  .navigation,.print-button { display:none; }
  main { border:0; padding:0; }
  h1 { font-size:22pt; } h2 { font-size:16pt; margin-top:24pt; }
  h3 { font-size:13pt; } h4,h5,h6 { font-size:11pt; }
  h1,h2,h3,h4,h5,h6 { break-after:avoid; }
  p { orphans:3; widows:3; }
  .table-scroll { overflow:visible; }
  table { min-width:0; font-size:9pt; }
  th,td { padding:5pt 6pt; }
  tr { break-inside:avoid; }
  thead { display:table-header-group; }
  pre { font-size:8.5pt; }
  a { color:inherit; }
}
"""


def build_html(source: Path, output: Path, add_contents=True):
    """Create a self-contained, offline reading edition without DOCX rendering."""
    lines = source.read_text(encoding="utf-8-sig").splitlines()
    entries = main_headings(lines)
    body = []
    i = 0
    while i < len(lines):
        line = lines[i].rstrip()
        if not line.strip():
            i += 1
            continue
        if line.lstrip().startswith("<!--"):
            while "-->" not in line and i + 1 < len(lines):
                i += 1
                line = lines[i]
            i += 1
            continue
        if re.match(r"^\s*([-*_])(?:\s*\1){2,}\s*$", line):
            i += 1
            continue
        match = re.match(r"^(#{1,6})\s+(.+)$", line)
        if match:
            level = len(match.group(1))
            body.append(f'<h{level} id="section_{i}">{html_inline(match.group(2))}</h{level}>')
            i += 1
            continue
        if fence_open(line):
            code_lines, i = read_fenced_block(lines, i)
            body.append("<pre><code>" + html.escape("\n".join(code_lines)) + "</code></pre>")
            continue
        if line.lstrip().startswith("|"):
            rows = []
            while i < len(lines) and lines[i].lstrip().startswith("|"):
                row = split_cells(lines[i])
                if not all(re.fullmatch(r":?-{3,}:?", cell) for cell in row):
                    rows.append(row)
                i += 1
            if rows:
                body.append('<div class="table-scroll"><table><thead><tr>')
                body.extend("<th scope=\"col\">" + html_inline(value) + "</th>" for value in rows[0])
                body.append("</tr></thead><tbody>")
                for row in rows[1:]:
                    body.append("<tr>" + "".join("<td>" + html_inline(value) + "</td>" for value in row) + "</tr>")
                body.append("</tbody></table></div>")
            continue
        listing = re.match(r"^(\s*)([-*+]\s+|\d+[.)、]\s+)(.*)$", line)
        if listing:
            depth = min(len(listing.group(1)) // 2, 4)
            marker = listing.group(2).strip()
            marker = "•" if marker in ("-", "*", "+") else marker
            body.append(f'<p class="list-item" style="margin-left:{depth * 1.2}em"><span class="marker">'
                        + html.escape(marker) + "</span>" + html_inline(listing.group(3)) + "</p>")
            i += 1
            continue
        if line.startswith(">"):
            body.append("<blockquote><p>" + html_inline(re.sub(r"^>\s?", "", line)) + "</p></blockquote>")
        else:
            body.append("<p>" + html_inline(line) + "</p>")
        i += 1
    navigation = '<nav class="navigation" aria-label="文档目录"><strong>Noetome 需求文档</strong>'
    navigation += '<button type="button" class="print-button" onclick="window.print()">打印文档</button>'
    if add_contents:
        navigation += "".join(f'<a href="#section_{index}">{html_inline(title)}</a>' for index, title in entries)
    navigation += "</nav>"
    result = ('<!doctype html><html lang="zh-CN"><head><meta charset="utf-8">'
              '<meta name="viewport" content="width=device-width,initial-scale=1">'
              '<title>Noetome 详细需求文档</title><style>' + HTML_CSS + '</style></head><body>'
              '<div class="layout">' + navigation + '<main>' + "\n".join(body) + '</main></div></body></html>')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result, encoding="utf-8", newline="\n")
    print(f"Created {output} ({output.stat().st_size:,} bytes)")
    print(f"Source: {len(lines)} lines; {len(entries)} main sections")

if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=root / "doc" / "requirements" / "PRD.md")
    parser.add_argument("--output", type=Path, default=root / "doc" / "requirements" / "PRD.html")
    parser.add_argument("--format", choices=("html",), default="html")
    parser.add_argument("--no-toc", action="store_true")
    args = parser.parse_args()
    build_html(args.source, args.output, add_contents=not args.no_toc)
