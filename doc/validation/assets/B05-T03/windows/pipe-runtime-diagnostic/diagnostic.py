import ctypes,json,os,subprocess,threading,time
from ctypes import wintypes
BASE=r'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
VENV=r'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
class Entry(ctypes.Structure):
    _fields_=[('dwSize',wintypes.DWORD),('cntUsage',wintypes.DWORD),('th32ProcessID',wintypes.DWORD),('th32DefaultHeapID',ctypes.c_size_t),('th32ModuleID',wintypes.DWORD),('cntThreads',wintypes.DWORD),('th32ParentProcessID',wintypes.DWORD),('pcPriClassBase',wintypes.LONG),('dwFlags',wintypes.DWORD),('szExeFile',wintypes.WCHAR*260)]
kernel=ctypes.WinDLL('kernel32',use_last_error=True)
kernel.CreateToolhelp32Snapshot.argtypes=[wintypes.DWORD,wintypes.DWORD];kernel.CreateToolhelp32Snapshot.restype=wintypes.HANDLE
kernel.Process32FirstW.argtypes=[wintypes.HANDLE,ctypes.POINTER(Entry)];kernel.Process32NextW.argtypes=kernel.Process32FirstW.argtypes
kernel.CloseHandle.argtypes=[wintypes.HANDLE]
kernel.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD];kernel.OpenProcess.restype=wintypes.HANDLE
kernel.TerminateProcess.argtypes=[wintypes.HANDLE,wintypes.UINT]
def snapshot(pids):
    handle=kernel.CreateToolhelp32Snapshot(2,0); entry=Entry();entry.dwSize=ctypes.sizeof(entry);rows=[]
    try:
        ok=kernel.Process32FirstW(handle,ctypes.byref(entry))
        while ok:
            if entry.th32ProcessID in pids:rows.append(dict(pid=entry.th32ProcessID,parent_pid=entry.th32ParentProcessID,name=entry.szExeFile))
            ok=kernel.Process32NextW(handle,ctypes.byref(entry))
    finally:kernel.CloseHandle(handle)
    return rows
rows=[]
for label,exe in [('venv_redirector',VENV),('direct_base',BASE)]:
    child=subprocess.Popen([exe,'-c','import os,time; os.close(0); print(os.getpid(),flush=True); time.sleep(5)'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    actual_pid=int(child.stdout.readline()); owned={child.pid,actual_pid}; process_rows=snapshot(owned)
    assert any(p['pid']==actual_pid and (actual_pid==child.pid or p['parent_pid']==child.pid) for p in process_rows)
    result={};started=time.monotonic()
    def writer():
        try:child.stdin.write(b'x'*65536);child.stdin.flush();result.update(completed=True,error=None)
        except OSError as exc:result.update(completed=True,error_type=type(exc).__name__,winerror=getattr(exc,'winerror',None),errno=exc.errno)
        result['latency_ms']=(time.monotonic()-started)*1000
    t=threading.Thread(target=writer);t.start();t.join(.5)
    before_cleanup=dict(result) if result else {'completed':False,'observed_for_ms':500}
    # Only processes whose exact created PID/parent relationship was just checked.
    for pid in owned:
        handle=kernel.OpenProcess(1,False,pid)
        if handle:
            try:kernel.TerminateProcess(handle,99)
            finally:kernel.CloseHandle(handle)
    child.wait(timeout=2);t.join(2);assert not t.is_alive()
    child.stdin.close();child.stdout.close();child.stderr.close()
    rows.append(dict(label=label,executable=exe,launcher_pid=child.pid,actual_python_pid=actual_pid,processes=process_rows,writer_before_cleanup=before_cleanup,writer_after_cleanup=result,created_pids_no_longer_present=snapshot(owned)==[]))
print(json.dumps({'kind':'stdlib_runtime_pipe_diagnostic','parent_pid':os.getpid(),'results':rows},indent=2))
