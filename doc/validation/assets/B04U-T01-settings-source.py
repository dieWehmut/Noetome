"""Fingerprint only explicit Noetome setting files; never decrypt credentials."""
import hashlib
import json
import os
from pathlib import Path
import sys

base = Path(os.environ['LOCALAPPDATA'])
paths = [base/'Noetome/model-services/tangmao-deepseek.json',
         base/'Noetome/model-services/tangmao-deepseek.usage.json',
         base/'Noetome/credentials/tangmao-deepseek.dpapi',
         base/'com.noetome.desktop/desktop-preferences.json']
result = {str(p): {'exists': p.exists(), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None,
                   'bytes': p.stat().st_size if p.exists() else None,
                   'mtime_ns': p.stat().st_mtime_ns if p.exists() else None} for p in paths}
output = Path('doc/validation/assets/B04U-T01-settings-' + sys.argv[1] + '.json')
assert not output.exists(), output
if sys.argv[1] == 'after':
    original = json.loads(Path('doc/validation/assets/B04U-T01-settings-before.json').read_text(encoding='utf-8'))
    assert result == original, 'Explicit Noetome settings changed during UI installation/readonly QA'
output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print('Explicit Noetome settings fingerprinted without decryption; ' + ('unchanged' if sys.argv[1] == 'after' else 'before installation'))
