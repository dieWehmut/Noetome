"""Launch only the installed test app with process-local loopback diagnostics."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.request

parser=argparse.ArgumentParser()
parser.add_argument('--node',required=True)
parser.add_argument('--modules',required=True)
args=parser.parse_args()
root=Path.cwd()
out=root/'.artifacts/b05u'
installed=Path(os.environ['LOCALAPPDATA'])/'Programs/Noetome/noetome-desktop.exe'
assert installed.is_file()
installed_hash=hashlib.sha256(installed.read_bytes()).hexdigest()
install=json.loads((root/'.artifacts/desktop-install.json').read_text(encoding='utf-8-sig'))
assert install['installer_exit_code']==0 and install['matches_current_build']
assert install['installer_unchanged'] and install['build_unchanged']
assert install['executable_sha256']==installed_hash
assert Path(install['installed_executable'])==installed
result={'installed_executable':str(installed),'installed_sha256':installed_hash,'diagnostic_scope':'process-only loopback; product configuration unchanged','runs':[]}
processes=[]

def launch(stage):
    with socket.socket() as probe:
        probe.bind(('127.0.0.1',0));port=probe.getsockname()[1]
    environment=os.environ.copy()
    environment['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS']=f'--remote-debugging-address=127.0.0.1 --remote-debugging-port={port}'
    process=subprocess.Popen([str(installed)],env=environment,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    processes.append(process)
    endpoint=f'http://127.0.0.1:{port}'
    deadline=time.monotonic()+15
    while time.monotonic()<deadline:
        if process.poll() is not None: raise RuntimeError(f'Installed app exited during startup: {process.returncode}')
        try:
            with urllib.request.urlopen(endpoint+'/json/list',timeout=1) as response:targets=json.load(response)
            if any(item.get('url')=='http://tauri.localhost/' for item in targets):break
        except (OSError,ValueError):pass
        time.sleep(.2)
    else:raise RuntimeError('Owned installed WebView diagnostic endpoint unavailable')
    run={'stage':stage,'pid':process.pid,'endpoint':endpoint}
    result['runs'].append(run)
    return process,endpoint,run

try:
    first,endpoint,run=launch('first')
    command=[args.node,str(out/'native_check.mjs'),'--modules',args.modules,'--endpoint',endpoint,'--stage','first','--output',str(out/'native-first.json'),'--dark-image',str(out/'native-dark.png')]
    completed=subprocess.run(command,capture_output=True,timeout=50)
    (out/'native-first-client-stderr.txt').write_bytes(completed.stderr)
    assert completed.returncode==0,(completed.returncode,completed.stderr.decode('utf-8','replace')[-2000:])
    first_report=json.loads((out/'native-first.json').read_text(encoding='utf-8'))
    run['close_exit_code']=first.wait(timeout=10)
    assert run['close_exit_code']==0
    run['real_close_ended_process']=True
    second,endpoint,run=launch('restart')
    command=[args.node,str(out/'native_check.mjs'),'--modules',args.modules,'--endpoint',endpoint,'--stage','restart','--output',str(out/'native-restart.json'),'--original-close',first_report['originalClose'],'--original-theme',first_report['originalTheme'],'--light-image',str(out/'native-light.png')]
    completed=subprocess.run(command,capture_output=True,timeout=50)
    (out/'native-restart-client-stderr.txt').write_bytes(completed.stderr)
    assert completed.returncode==0,(completed.returncode,completed.stderr.decode('utf-8','replace')[-2000:])
    # The test owns this process. End diagnostics after original settings restore.
    second.terminate();run['diagnostic_cleanup_exit_code']=second.wait(timeout=10)
    environment=os.environ.copy();environment.pop('WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS',None)
    ordinary=subprocess.Popen([str(installed)],env=environment,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    time.sleep(1)
    assert ordinary.poll() is None
    result['ordinary_launch']={'pid':ordinary.pid,'process_running':True,'diagnostics_enabled':False}
    result['original_close']=first_report['originalClose'];result['original_theme']=first_report['originalTheme']
    result['passed']=True
except Exception as error:
    result['passed']=False;result['failure']=str(error)
finally:
    for process in processes:
        if process.poll() is None:process.terminate();process.wait(timeout=10)
    (out/'native-supervisor.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps(result))
sys.exit(0 if result['passed'] else 1)
