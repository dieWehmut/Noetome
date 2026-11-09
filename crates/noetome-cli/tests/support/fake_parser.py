"""Synthetic test producer; standard library only, test directory only."""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

def ready(path, text):
    path=Path(path)
    pending=path.with_name(path.name+'.pending')
    pending.write_text(text)
    os.replace(pending,path)

mode = sys.argv[1]
if mode == 'probe_hold':
    from ctypes import wintypes
    # An independent test observer retains the exact live process objects.
    # Reopening a numeric PID after termination can observe an unrelated reuse.
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
    kernel.WaitForSingleObject.restype = wintypes.DWORD
    kernel.GetProcessTimes.argtypes = (wintypes.HANDLE,) + (ctypes.POINTER(wintypes.FILETIME),) * 4
    kernel.GetProcessTimes.restype = wintypes.BOOL
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    kernel.CloseHandle.restype = wintypes.BOOL
    pids = list(map(int, sys.argv[2:]))
    handles = []
    def sample():
        times = []
        exit_times = []
        states = []
        for handle in handles:
            created, exited, system, user = (wintypes.FILETIME() for _ in range(4))
            if not kernel.GetProcessTimes(handle, ctypes.byref(created), ctypes.byref(exited), ctypes.byref(system), ctypes.byref(user)):
                raise ctypes.WinError(ctypes.get_last_error())
            times.append((created.dwHighDateTime << 32) | created.dwLowDateTime)
            exit_times.append((exited.dwHighDateTime << 32) | exited.dwLowDateTime)
            states.append(kernel.WaitForSingleObject(handle, 0))
        print(json.dumps({'pids': pids, 'creation_times': times, 'exit_times': exit_times, 'wait_codes': states}), flush=True)
    try:
        for pid in pids:
            handle = kernel.OpenProcess(0x1000 | 0x100000, False, pid)
            if not handle:
                raise ctypes.WinError(ctypes.get_last_error())
            handles.append(handle)
        sample()
        for line in sys.stdin:
            if line.strip() == 'sample': sample()
            elif line.strip() == 'quit': break
            else: raise RuntimeError('invalid held-probe command')
    finally:
        for handle in handles: kernel.CloseHandle(handle)
    sys.exit(0)
if mode == 'probe':
    kernel = ctypes.windll.kernel32
    kernel.OpenProcess.restype = ctypes.c_void_p
    alive = []
    for pid in map(int, sys.argv[2:]):
        handle = kernel.OpenProcess(0x1000 | 0x100000, False, pid)
        if handle:
            if kernel.WaitForSingleObject(ctypes.c_void_p(handle), 0) == 0x102:
                alive.append(pid)
            kernel.CloseHandle(ctypes.c_void_p(handle))
    print(json.dumps(alive))
    sys.exit(0)
if mode == 'grandchild':
    ready(sys.argv[2],str(os.getpid()))
    time.sleep(30)
    sys.exit(0)
if mode == 'child':
    subprocess.Popen([sys.executable, __file__, 'grandchild', sys.argv[2]], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ready(sys.argv[3],str(os.getpid()))
    time.sleep(30)
    sys.exit(0)
if mode == 'tree_close_stdin':
    base=Path(sys.argv[2])
    subprocess.Popen([sys.executable,__file__,'child',str(base)+'.grandchild',str(base)+'.child'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    while not (Path(str(base)+'.grandchild').exists() and Path(str(base)+'.child').exists()): time.sleep(.005)
    ready(base,json.dumps([os.getpid(),int(Path(str(base)+'.child').read_text()),int(Path(str(base)+'.grandchild').read_text())]))
    while not Path(str(base)+'.go').exists(): time.sleep(.005)
    os.close(0)
    time.sleep(5)
    sys.exit(0)
if mode == 'no_stdin':
    time.sleep(30)
    sys.exit(0)
request = json.load(sys.stdin)
fp = request.get('input', {}).get('fingerprint', {'sha256': '0'*64, 'byte_length': 0})
response = {
    'protocol_version': 1, 'request_id': request.get('request_id', 'test'),
    'input_fingerprint': fp, 'workspace_capability_id': request.get('workspace', {}).get('capability_id', 'test'),
    'producer': 'fake', 'parser': {'name': 'synthetic-test', 'version': '1', 'capability_version': 1, 'capabilities': ['source_units_v1']},
    'unit_count': 0, 'units': [], 'resources': [], 'issues': []
}
if mode.startswith('tree_'):
    base = Path(sys.argv[2])
    subprocess.Popen([sys.executable, __file__, 'child', str(base)+'.grandchild', str(base)+'.child'], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    until = time.monotonic()+5
    while not (Path(str(base)+'.grandchild').exists() and Path(str(base)+'.child').exists()):
        if time.monotonic() > until: raise RuntimeError('tree handshake failed')
        time.sleep(.005)
    ready(base,json.dumps([os.getpid(), int(Path(str(base)+'.child').read_text()), int(Path(str(base)+'.grandchild').read_text())]))
    while not Path(str(base)+'.go').exists(): time.sleep(.005)
    if mode == 'tree_early':
        print(json.dumps(response), flush=True)
        sys.exit(0)
    if mode == 'tree_output':
        sys.stdout.write('x'*1000000)
        sys.stdout.flush()
    time.sleep(30)
    sys.exit(0)
if mode == 'empty': sys.exit(0)
if mode == 'invalid_utf8': sys.stdout.buffer.write(b'\xff'); sys.exit(0)
if mode == 'invalid_json': print('{oops'); sys.exit(0)
if mode == 'crash': os._exit(13)
if mode == 'parser_zero': response['producer']='parser'  # deliberately forged adversarial response, never a valid fake
if mode == 'version': response['protocol_version']=2
if mode == 'capability_version': response['parser']['capability_version']=2
if mode == 'unknown':
    response['surprise']=True
    sys.stderr.buffer.write(b'synthetic protocol failure diagnostic\n')
if mode == 'nested_unknown': response['parser']['surprise']=True
if mode == 'enum': response['producer']='fixture'
if mode == 'capability': response['parser']['capabilities']=['unknown']
if mode == 'request_id': response['request_id']='wrong'
if mode == 'sha': response['input_fingerprint']={'sha256':'1'*64, 'byte_length':fp['byte_length']}
if mode == 'length': response['input_fingerprint']={'sha256':fp['sha256'], 'byte_length':fp['byte_length']+1}
if mode == 'workspace_id': response['workspace_capability_id']='wrong'
if mode == 'parser_empty': response['parser']['version']=''
if mode == 'duplicate_unit':
    response['units']=[{'unit_id':'u','ordinal':i,'kind':'formatting','status':'parsed','text':None,'resource_ids':[]} for i in range(2)]
    response['unit_count']=2
if mode == 'missing_unit': response['unit_count']=1
if mode == 'ordinal':
    response['units']=[{'unit_id':'u','ordinal':1,'kind':'formatting','status':'parsed','text':None,'resource_ids':[]}]
    response['unit_count']=1
if mode == 'unknown_ref': response['issues']=[{'code':'TEST','severity':'blocking','message':'synthetic','unit_ids':['missing'],'resource_ids':[]}]
if mode == 'input_write':
    try: Path(request['input']['path']).write_bytes(b'changed')
    except OSError: pass
if mode.startswith('resource_') or mode == 'valid_resource':
    root=Path(request['workspace']['path'])
    path='text.txt'
    data='synthetic 文本'.encode()
    root.joinpath(path).write_bytes(data)
    response['resources']=[{'resource_id':'r','relative_path':path,'kind':'utf8_text','fingerprint':{'sha256':hashlib.sha256(data).hexdigest(),'byte_length':len(data)}}]
    response['units']=[{'unit_id':'u','ordinal':0,'kind':'text','status':'parsed','text':{'resource_id':'r','start_byte':0,'end_byte':len(data)},'resource_ids':['r']}]
    response['unit_count']=1
    if mode=='resource_deep':
        path='/'.join(['deep']*40)+'/text.txt'
        root.joinpath(path).parent.mkdir(parents=True)
        root.joinpath('text.txt').rename(root.joinpath(path))
        response['resources'][0]['relative_path']=path
    if mode=='resource_missing_text': del response['units'][0]['text']
    if mode=='resource_failed_no_issue': response['units'][0]['status']='failed'
    if mode=='resource_review_no_issue': response['units'][0]['status']='needs_review'
    if mode=='resource_unknown_parsed': response['units'][0]['kind']='unknown'
    if mode=='resource_review':
        response['units'][0]['status']='needs_review'
        response['issues']=[{'code':'SYNTHETIC_REVIEW','severity':'blocking','message':'synthetic test requires review','unit_ids':['u'],'resource_ids':['r']}]
    if mode=='resource_extra_issue':
        response['issues']=[{'code':'A','severity':'blocking','message':'synthetic','unit_ids':['u'],'resource_ids':[]},{'code':'B','severity':'warning','message':'synthetic','unit_ids':[],'resource_ids':[]}]
    if mode=='resource_extra_units':
        response['units'].append(dict(response['units'][0],unit_id='u2',ordinal=1))
        response['unit_count']=2
    if mode=='resource_extra_resources':
        root.joinpath('second.txt').write_bytes(data)
        response['resources'].append(dict(response['resources'][0],resource_id='r2',relative_path='second.txt'))
        response['units'][0]['resource_ids'].append('r2')
    if mode=='resource_sha': response['resources'][0]['fingerprint']['sha256']='0'*64
    if mode=='resource_length': response['resources'][0]['fingerprint']['byte_length']=1
    if mode=='resource_unknown': root.joinpath('extra.txt').write_bytes(b'extra')
    if mode=='resource_emptydir': root.joinpath('extra').mkdir()
    if mode=='resource_abs': response['resources'][0]['relative_path']=str(root/'text.txt')
    if mode=='resource_parent': response['resources'][0]['relative_path']='../outside.txt'
    if mode=='resource_reserved': response['resources'][0]['relative_path']='NUL.txt'
    if mode=='resource_trailing': response['resources'][0]['relative_path']='text.txt.'
    if mode=='resource_case': response['resources'].append(dict(response['resources'][0], resource_id='r2', relative_path='TEXT.txt'))
    if mode=='resource_prefix': response['resources'].append(dict(response['resources'][0], resource_id='r2', relative_path='text.txt/child'))
    if mode=='resource_utf8range': response['units'][0]['text']['end_byte']=11
    if mode=='resource_reference': response['units'][0]['text']['resource_id']='missing'
    if mode=='resource_unreferenced': response['units'][0]['resource_ids']=[]; response['units'][0]['text']=None; response['units'][0]['kind']='formatting'
    if mode=='resource_hardlink':
        root.joinpath('text.txt').unlink()
        os.link(sys.argv[2], root/'text.txt')
    if mode=='resource_symlink':
        root.joinpath('text.txt').unlink()
        os.symlink(sys.argv[2], root/'text.txt')
    if mode=='resource_junction':
        subprocess.run(['cmd','/c','mklink','/J',os.path.normpath(str(root/'linked')),os.path.normpath(sys.argv[2])], stdout=subprocess.DEVNULL, check=True)
    if mode=='resource_invalid_utf8':
        root.joinpath(path).write_bytes(b'\xff')
        response['resources'][0]['fingerprint']={'sha256':hashlib.sha256(b'\xff').hexdigest(),'byte_length':1}
if mode=='many_units_large_resource':
    data=b'x'*(16*1024*1024)
    Path(request['workspace']['path'],'text.txt').write_bytes(data)
    response['resources']=[{'resource_id':'r','relative_path':'text.txt','kind':'utf8_text','fingerprint':{'sha256':hashlib.sha256(data).hexdigest(),'byte_length':len(data)}}]
    response['units']=[{'unit_id':f'u{i}','ordinal':i,'kind':'text','status':'parsed','text':{'resource_id':'r','start_byte':0,'end_byte':1},'resource_ids':['r']} for i in range(10000)]
    response['unit_count']=10000
if mode=='stdout_limit': sys.stdout.write(' '*1000000)
if mode=='stderr_limit': sys.stderr.write('x'*1000000); sys.stderr.flush()
if mode=='diagnostics': sys.stderr.buffer.write(b'synthetic diagnostic\n')
if mode=='concurrent': sys.stderr.write('x'*60000); sys.stderr.flush()
wire=json.dumps(response)
if mode=='truncated': wire=wire[:-1]
if mode=='extra': wire+='\n{}'
if mode=='trailing': wire+='\n \t\r'
sys.stdout.write(wire)
sys.stdout.flush()
sys.exit(7 if mode=='nonzero' else 0)
