"""Test-only documents load the actual app with bounded storage faults.

Fixtures are outside the application/public directory and never packaged.
They alter only this document's Storage prototype, not persisted values.
"""
from pathlib import Path
import urllib.request

root = Path.cwd()
with urllib.request.urlopen('http://127.0.0.1:1420/', timeout=5) as response:
    entry = response.read().decode('utf-8')
scripts = {
    'theme-write': "const original=Storage.prototype.setItem; Storage.prototype.setItem=function(key,value){if(key==='noetome.appearance.theme')throw new DOMException('Controlled write failure','QuotaExceededError');return original.call(this,key,value)};",
    'theme-read': "const original=Storage.prototype.getItem; Storage.prototype.getItem=function(key){if(key==='noetome.appearance.theme')throw new DOMException('Controlled read failure','SecurityError');return original.call(this,key)};",
    'theme-invalid': "const original=Storage.prototype.getItem; Storage.prototype.getItem=function(key){if(key==='noetome.appearance.theme')return 'invalid-theme';return original.call(this,key)};",
}
for name, script in scripts.items():
    target = root / '.artifacts/b05u' / f'{name}-test.html'
    content = entry.replace('<title>Noetome</title>', f'<title>Noetome / controlled {name}</title>')
    content = content.replace('<div id="root"></div>', f'<script>{script}</script>\n    <div id="root"></div>')
    target.write_text(content, encoding='utf-8', newline='\n')
    print(target.relative_to(root).as_posix())
