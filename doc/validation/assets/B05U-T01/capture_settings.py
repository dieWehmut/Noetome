"""Read hashes of the selected Noetome files; no decryption or settings writes."""
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
output = Path('.artifacts/b05u/settings-' + sys.argv[1] + '.json')
assert not output.exists(), output
if sys.argv[1] == 'after':
    original = json.loads(Path('.artifacts/b05u/settings-before.json').read_text(encoding='utf-8'))
    assert all(result[str(p)] == original[str(p)] for p in paths[:3]), 'Model/usage/encrypted credential files changed'
    if original[str(paths[3])]['exists']:
        assert result[str(paths[3])]['sha256'] == original[str(paths[3])]['sha256'], 'Original close preference bytes were not restored'
output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
print('Selected Noetome files fingerprinted without decryption: ' + sys.argv[1])
