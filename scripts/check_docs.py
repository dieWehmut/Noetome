"""Validate local document links, Python syntax, PRD generation and SQL blueprint."""
from __future__ import annotations

import ast
import contextlib
import io
from pathlib import Path
import re
import sqlite3
import sys
import tempfile
from urllib.parse import unquote, urlsplit

import build_prd


def check(root):
    files = [root / 'README.md', root / 'AGENTS.md']
    files += sorted((root / 'doc').rglob('*.md'))
    files += sorted((root / 'dev-log').rglob('*.md'))
    count = 0
    for path in files:
        content = path.read_text(encoding='utf-8-sig')
        content = re.sub(r'(?ms)^\s*(```|~~~).*?^\s*\1\s*$', '', content)
        for target in re.findall(r'\[[^\]\n]*\]\(([^)\n]+)\)', content):
            target = target.strip('<>')
            parsed = urlsplit(target)
            if parsed.scheme or not parsed.path:
                continue
            resolved = (path.parent / unquote(parsed.path)).resolve()
            if not resolved.is_relative_to(root.resolve()) or not resolved.exists():
                raise ValueError(f'{path.relative_to(root)}: 无效本地链接 {target}')
            count += 1
    for folder in ('scripts', 'tests'):
        for path in (root / folder).rglob('*.py'):
            ast.parse(path.read_text(encoding='utf-8-sig'), filename=str(path))
    source = root / 'doc/requirements/PRD.md'
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'PRD.html'
        with contextlib.redirect_stdout(io.StringIO()):
            build_prd.build_html(source, output)
        expected = output.read_text(encoding='utf-8-sig')
    actual = (root / 'doc/requirements/PRD.html').read_text(encoding='utf-8-sig')
    if expected != actual:
        raise ValueError('PRD.html过期，请运行build_prd.py')
    connection = sqlite3.connect(':memory:')
    try:
        connection.execute('PRAGMA foreign_keys=ON')
        connection.executescript((root / 'doc/technical/schema.sql').read_text(encoding='utf-8-sig'))
        if connection.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
            raise ValueError('schema.sql integrity_check失败')
        if connection.execute('PRAGMA foreign_key_check').fetchall():
            raise ValueError('schema.sql foreign_key_check失败')
        tables = connection.execute("SELECT count(*) FROM sqlite_master WHERE type='table'").fetchone()[0]
    finally:
        connection.close()
    return f'规范检查通过：{len(files)}份Markdown、{count}个本地链接、Python语法、PRD同步、SQLite蓝图{tables}表；不是产品运行验收'


if __name__ == '__main__':
    try:
        print(check(Path(__file__).resolve().parents[1]))
    except (ValueError, OSError, sqlite3.Error, SyntaxError) as exc:
        print(f'规范错误：{exc}', file=sys.stderr)
        raise SystemExit(1)
