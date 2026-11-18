# Launch one persisted production run. Rust owns requests, deadlines and budget transactions.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [Parameter(Mandatory=$true)][string]$Project,
    [Parameter(Mandatory=$true)][string]$RunId,
    [Parameter(Mandatory=$true)][string]$CredentialFile
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'text_api_support.ps1')

# Assign at creation with a native Job-list attribute, create suspended, then resume.
# The secret goes only into this child's Unicode environment; neither arguments nor files contain it.
if (-not ('Noetome.CompileProcess' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
namespace Noetome {
    public sealed class CompileProcess : IDisposable {
        [StructLayout(LayoutKind.Sequential)] private struct Security {
            public uint Length; public IntPtr Descriptor; public int Inherit;
        }
        [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] private struct Startup {
            public uint Size; public string Reserved, Desktop, Title;
            public uint X,Y,XSize,YSize,XCount,YCount,Fill,Flags;
            public ushort Show,ReservedSize; public IntPtr ReservedBytes,Input,Output,Error;
        }
        [StructLayout(LayoutKind.Sequential)] private struct Info { public IntPtr Process,Thread; public uint Pid,Tid; }
        [StructLayout(LayoutKind.Sequential)] private struct ExtendedStartup { public Startup Startup; public IntPtr Attributes; }
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool CreatePipe(out IntPtr read, out IntPtr write, ref Security security, uint size);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern bool CreateProcess(string application, StringBuilder command, IntPtr processSecurity, IntPtr threadSecurity, bool inherit, uint flags, IntPtr environment, string directory, ref ExtendedStartup startup, out Info info);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool InitializeProcThreadAttributeList(IntPtr list, uint count, uint flags, ref IntPtr size);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, IntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returned);
        [DllImport("kernel32.dll")] private static extern void DeleteProcThreadAttributeList(IntPtr list);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern uint ResumeThread(IntPtr thread);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool TerminateProcess(IntPtr process, uint code);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool GetExitCodeProcess(IntPtr process, out uint code);
        [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
        [DllImport("kernel32.dll")] private static extern IntPtr GetStdHandle(int kind);
        private Info info;
        private Process process;
        public Stream Stdout { get; private set; }
        public Stream Stderr { get; private set; }
        public Process Process { get { return process; } }
        public int ExitCode { get {
            uint code; if (!GetExitCodeProcess(info.Process,out code)) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (code==259) throw new IOException("child-still-active"); return unchecked((int)code);
        } }
        private static void Pipe(out IntPtr read, out IntPtr write) {
            var security=new Security { Length=(uint)Marshal.SizeOf(typeof(Security)), Inherit=1 };
            if (!CreatePipe(out read,out write,ref security,0)) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (!SetHandleInformation(read,1,0)) { int error=Marshal.GetLastWin32Error(); CloseHandle(read); CloseHandle(write); read=write=IntPtr.Zero; throw new Win32Exception(error); }
        }
        public CompileProcess(string binary, string arguments, string project, string name, string secret, IntPtr jobHandle) {
            IntPtr outRead=IntPtr.Zero,outWrite=IntPtr.Zero,errRead=IntPtr.Zero,errWrite=IntPtr.Zero,environment=IntPtr.Zero;
            IntPtr attributes=IntPtr.Zero,jobs=IntPtr.Zero; bool initialized=false; int environmentCharacters=0;
            try {
                Pipe(out outRead,out outWrite); Pipe(out errRead,out errWrite);
                var variables=new SortedDictionary<string,string>(StringComparer.OrdinalIgnoreCase);
                foreach (DictionaryEntry pair in Environment.GetEnvironmentVariables()) variables[(string)pair.Key]=(string)pair.Value;
                variables[name]=secret;
                var block=new StringBuilder(); foreach (var pair in variables) block.Append(pair.Key).Append('=').Append(pair.Value).Append('\0'); block.Append('\0');
                environmentCharacters=block.Length; environment=Marshal.StringToHGlobalUni(block.ToString());
                IntPtr size=IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref size);
                if (size==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
                attributes=Marshal.AllocHGlobal(size);
                if (!InitializeProcThreadAttributeList(attributes,1,0,ref size)) throw new Win32Exception(Marshal.GetLastWin32Error());
                initialized=true; jobs=Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobs,jobHandle);
                // PROC_THREAD_ATTRIBUTE_JOB_LIST, supported by the Windows 11 target.
                if (!UpdateProcThreadAttribute(attributes,0,new IntPtr(0x2000d),jobs,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero)) throw new Win32Exception(Marshal.GetLastWin32Error());
                var startup=new ExtendedStartup { Startup=new Startup { Size=(uint)Marshal.SizeOf(typeof(ExtendedStartup)),Flags=0x100u,Input=GetStdHandle(-10),Output=outWrite,Error=errWrite },Attributes=attributes };
                // EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW.
                if (!CreateProcess(binary,new StringBuilder(arguments),IntPtr.Zero,IntPtr.Zero,true,0x08080404u,environment,project,ref startup,out info)) throw new Win32Exception(Marshal.GetLastWin32Error());
                process=Process.GetProcessById((int)info.Pid);
                Stdout=new FileStream(new SafeFileHandle(outRead,true),FileAccess.Read,8192,false); outRead=IntPtr.Zero;
                Stderr=new FileStream(new SafeFileHandle(errRead,true),FileAccess.Read,8192,false); errRead=IntPtr.Zero;
            } catch { Dispose(); throw; }
            finally {
                if (initialized) DeleteProcThreadAttributeList(attributes);
                if (attributes!=IntPtr.Zero) Marshal.FreeHGlobal(attributes); if (jobs!=IntPtr.Zero) Marshal.FreeHGlobal(jobs);
                if (outRead!=IntPtr.Zero) CloseHandle(outRead); if (errRead!=IntPtr.Zero) CloseHandle(errRead);
                if (outWrite!=IntPtr.Zero) CloseHandle(outWrite); if (errWrite!=IntPtr.Zero) CloseHandle(errWrite);
                if (environment!=IntPtr.Zero) { // Erase the copied Unicode credential environment before releasing it.
                    for (int i=0;i<environmentCharacters;i++) Marshal.WriteInt16(environment,i*2,0);
                    Marshal.FreeHGlobal(environment);
                }
            }
        }
        public void Resume() {
            if (ResumeThread(info.Thread)==UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error());
            CloseHandle(info.Thread); info.Thread=IntPtr.Zero;
        }
        public void Dispose() {
            if (info.Process!=IntPtr.Zero) {
                if (process==null || !process.HasExited) TerminateProcess(info.Process,2);
                CloseHandle(info.Process); info.Process=IntPtr.Zero;
            }
            if (info.Thread!=IntPtr.Zero) { CloseHandle(info.Thread); info.Thread=IntPtr.Zero; }
            if (Stdout!=null) { Stdout.Dispose(); Stdout=null; } if (Stderr!=null) { Stderr.Dispose(); Stderr=null; }
            if (process!=null) { process.Dispose(); process=null; }
        }
    }
}
'@
}
$secure=$null; $pointer=[IntPtr]::Zero; $plaintext=$null; $worker=$null; $job=$null
$stdout=$null; $stderr=$null; $diagnostic='operation-failed'; $exitCode=2
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { $diagnostic='windows-required'; throw 'stop' }
    $diagnostic='scope-invalid'
    if ($RunId -cnotmatch '^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$') { throw 'stop' }
    $binaryGuard=Open-Selected $Binary
    $selectedProject=Selected-Path $Project
    $guards.Add([Noetome.TextInvokeNative]::DirectoryGuard($selectedProject))
    $diagnostic='credential-unavailable'
    if (-not (Test-Path -LiteralPath $CredentialFile)) { $diagnostic='credential-missing'; throw 'stop' }
    $credentialGuard=Open-Selected $CredentialFile -External
    $secure=(Read-Selected $credentialGuard 65536) | ConvertTo-SecureString
    if ($secure.Length -lt 1 -or $secure.Length -gt 8192) { throw 'stop' }
    $pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    for ($index=0; $index -lt $secure.Length; $index++) {
        $character=[Runtime.InteropServices.Marshal]::ReadInt16($pointer,$index*2)
        if ($character -lt 33 -or $character -gt 126) { throw 'stop' }
    }
    $plaintext=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    $arguments=((@($Binary,'compile','run',$selectedProject,$RunId)) | ForEach-Object { Native-Argument $_ }) -join ' '
    $diagnostic='child-launch-failed'
    $job=New-Object Noetome.TextInvokeJob
    $diagnostic='child-containment-failed'
    $worker=New-Object Noetome.CompileProcess($Binary,$arguments,$selectedProject,'NOETOME_TANGMAO_API_KEY',$plaintext,$job.Handle)
    $worker.Resume()
    $stdoutTask=[Noetome.TextInvokeNative]::ReadBounded($worker.Stdout,16777216)
    $stderrTask=[Noetome.TextInvokeNative]::ReadBounded($worker.Stderr,1048576)
    $cleanupWatch=$null
    while ($true) {
        if ($stdoutTask.IsFaulted -or $stderrTask.IsFaulted) { $diagnostic='child-output-limit'; throw 'stop' }
        if ($worker.Process.WaitForExit(50)) {
            if ($null -eq $cleanupWatch) {
                $exitCode=$worker.ExitCode
                # A root may leave a descendant holding its pipes. Stop the owned
                # Job immediately on root exit, then give only cleanup a deadline.
                $diagnostic='child-cleanup-failed'; $job.Terminate()
                if (-not $job.WaitEmpty(5000)) { throw 'stop' }
                $cleanupWatch=[Diagnostics.Stopwatch]::StartNew()
            }
            if ($stdoutTask.IsCompleted -and $stderrTask.IsCompleted) { break }
            if ($cleanupWatch.ElapsedMilliseconds -gt 10000) { throw 'stop' }
        }
    }
    $stdout=$stdoutTask.GetAwaiter().GetResult(); $stderr=$stderrTask.GetAwaiter().GetResult()
    # Public stdout/stderr use the same bounded conservative credential redaction as legacy callers.
    Emit-Utf8 (Hide-Credential $stdout $plaintext)
    Emit-Utf8 (Hide-Credential $stderr $plaintext) -ErrorStream
} catch {
    [Console]::Error.WriteLine('compile-credential-invoke: '+$diagnostic); $exitCode=2
} finally {
    if ($null -ne $job) {
        try { $job.Terminate(); if (-not $job.WaitEmpty(5000)) { [Console]::Error.WriteLine('compile-credential-invoke: child-termination-unconfirmed'); $exitCode=2 } }
        catch { [Console]::Error.WriteLine('compile-credential-invoke: child-termination-unconfirmed'); $exitCode=2 }
        $job.Dispose()
    }
    if ($null -ne $worker) { $worker.Dispose() }
    $plaintext=$null; $stdout=$null; $stderr=$null
    if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
    if ($null -ne $secure) { $secure.Dispose() }
    for ($i=$guards.Count-1; $i -ge 0; $i--) { $guards[$i].Dispose() }
}
exit $exitCode
