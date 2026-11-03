from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
paths = ['README.md', 'doc/README.md', 'doc/standards/DESIGN.md', 'doc/standards/TESTING.md', 'doc/execution/NEXT_STEPS.md', 'doc/execution/B04-T03_REVIEW_PLAN.md', 'doc/validation/B04-T03.md', 'doc/validation/B04-T03-ui-single-row.md']
checks = []
for name in paths:
    document = REPO / name
    for target in re.findall(r'!?\[[^\]]*\]\(([^\n)]+)\)', document.read_text(encoding='utf-8')):
        target = target.strip().split(' "', 1)[0].strip('<>')
        if re.match(r'^[a-z][a-z0-9+.-]*:', target, re.I) or target.startswith('#'): continue
        selected = (document.parent / target.split('#', 1)[0]).resolve()
        assert selected.exists(), (name, target)
        checks.append({'document': name, 'target': target, 'exists': True})
tested = json.loads((SDK / 'root-full-rust-tests-command.json').read_bytes())['code_after']
for name, fingerprint in tested.items():
    data = (REPO / name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == fingerprint['sha256'] and len(data) == fingerprint['byte_length'], name
result = {'captured_at': datetime.now(timezone.utc).isoformat(), 'documents': len(paths), 'local_link_checks': checks, 'local_links_passed': len(checks), 'tested_product_source_files_unchanged': len(tested), 'passed': True}
with (SDK / 'root-integration-docs.json').open('x', encoding='utf-8', newline='\n') as out:
    json.dump(result, out, ensure_ascii=False, indent=2); out.write('\n')
print(json.dumps({'documents': len(paths), 'local_links_passed': len(checks), 'tested_source_files_unchanged': len(tested), 'passed': True}))
