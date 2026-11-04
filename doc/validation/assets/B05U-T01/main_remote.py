"""Set/verify the authorized repository default; keep Git credentials in memory."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.error
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument('--verify-only', action='store_true')
parser.add_argument('--output', required=True)
args = parser.parse_args()
repository = 'dieWehmut/Noetome'
feature = 'codex/b05u-t01-sidebar-settings'
result = {'at_utc': datetime.now(timezone.utc).isoformat(), 'repository': repository,
          'mode': 'verify only' if args.verify_only else 'set and verify default main'}

def git(*arguments):
    return subprocess.check_output(['git', *arguments]).decode('utf-8').strip()

def refs():
    lines = git('ls-remote', '--symref', 'origin', 'HEAD', 'refs/heads/main', 'refs/heads/' + feature).splitlines()
    result = {}
    for line in lines:
        value, name = line.split('\t')
        result[name] = value
    return result

try:
    assert git('remote', 'get-url', 'origin') == 'https://github.com/' + repository + '.git'
    expected = git('rev-parse', 'main')
    feature_head = git('rev-parse', feature)
    assert git('branch', '--show-current') == 'main'
    subprocess.check_call(['git', 'merge-base', '--is-ancestor', feature_head, 'main'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    result['expected_main_head'] = expected
    result['feature_head'] = feature_head
    result['head_parents'] = git('rev-list', '--parents', '-n', '1', 'main').split()[1:]
    initial_refs = refs()
    assert initial_refs['refs/heads/main'] == expected
    assert initial_refs['refs/heads/' + feature] == feature_head
    result['remote_refs_before'] = initial_refs

    environment = os.environ.copy()
    environment['GIT_TERMINAL_PROMPT'] = '0'
    environment['GCM_INTERACTIVE'] = 'Never'
    filled = subprocess.run(['git', 'credential', 'fill'],
        input=('protocol=https\nhost=github.com\npath=' + repository + '.git\n\n').encode(),
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment, timeout=30)
    if filled.returncode != 0:
        raise RuntimeError('Existing Git credential helper did not return an API credential')
    credentials = dict(line.split('=', 1) for line in filled.stdout.decode().splitlines() if '=' in line)
    token = credentials.get('password')
    if not token:
        raise RuntimeError('Existing Git credential helper returned no password/token')
    def api(method='GET', data=None):
        request = urllib.request.Request('https://api.github.com/repos/' + repository,
            data=json.dumps(data).encode() if data is not None else None,
            headers={'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json',
                     'Content-Type': 'application/json', 'User-Agent': 'Noetome-development'}, method=method)
        with urllib.request.urlopen(request, timeout=20) as response:
            return response.status, json.load(response)
    status, before = api()
    assert before['full_name'] == repository
    result['api_default_before'] = before['default_branch']
    result['default_patch_sent'] = False
    if before['default_branch'] != 'main':
        if args.verify_only:
            raise RuntimeError('Remote default branch is not main')
        status, changed = api('PATCH', {'default_branch': 'main'})
        assert status == 200 and changed['default_branch'] == 'main'
        result['default_patch_sent'] = True
        result['patch_status'] = status
    status, after = api()
    assert status == 200 and after['default_branch'] == 'main'
    result['api_default_after'] = after['default_branch']
    for _ in range(3):
        observed = refs()
        if observed.get('HEAD') == expected and observed.get('refs/heads/main') == expected:
            break
        time.sleep(.3)
    assert observed['HEAD'] == expected and observed['refs/heads/main'] == expected
    assert observed['HEAD'] != 'ref: refs/heads/main'  # symref and commit are recorded separately below.
    symbolic = git('ls-remote', '--symref', 'origin', 'HEAD').splitlines()
    assert symbolic[0] == 'ref: refs/heads/main\tHEAD'
    result['remote_head_symbolic'] = 'refs/heads/main'
    result['remote_refs_after'] = observed
    subprocess.check_call(['git', 'remote', 'set-head', 'origin', '-a'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert git('symbolic-ref', 'refs/remotes/origin/HEAD') == 'refs/remotes/origin/main'
    result['local_origin_head'] = 'refs/remotes/origin/main'
    result['verified'] = True
except urllib.error.HTTPError as error:
    result['verified'] = False
    result['failure'] = {'http_status': error.code, 'reason': error.reason}
except Exception as error:
    result['verified'] = False
    result['failure'] = {'type': type(error).__name__, 'reason': str(error)}
Path(args.output).write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(result, ensure_ascii=True))
sys.exit(0 if result['verified'] else 1)
