# Shared guarded files, bounded pipes and quota checks for the narrow text desktop helper.
if (-not ('Noetome.TextInvokeNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
namespace Noetome {
    // This narrow job contains the selected CLI after Process.Start. It is not
    // a file/network sandbox and does not remove the pre-assignment spawn window.
    public sealed class TextInvokeJob : IDisposable {
        [StructLayout(LayoutKind.Sequential)] private struct BasicLimit {
            public long ProcessTime, JobTime; public uint Flags;
            public UIntPtr MinimumWorkingSet, MaximumWorkingSet; public uint ActiveLimit;
            public UIntPtr Affinity; public uint Priority, Scheduling;
        }
        [StructLayout(LayoutKind.Sequential)] private struct IoCounters {
            public ulong Reads, Writes, Other, ReadBytes, WriteBytes, OtherBytes;
        }
        [StructLayout(LayoutKind.Sequential)] private struct ExtendedLimit {
            public BasicLimit Basic; public IoCounters Io;
            public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
        }
        [StructLayout(LayoutKind.Sequential)] private struct Accounting {
            public long User, Kernel, PeriodUser, PeriodKernel;
            public uint Faults, TotalProcesses, ActiveProcesses, TerminatedProcesses;
        }
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool SetInformationJobObject(IntPtr job, int kind, ref ExtendedLimit value, uint length);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool QueryInformationJobObject(IntPtr job, int kind, out Accounting value, uint length, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool TerminateJobObject(IntPtr job, uint code);
        [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
        private IntPtr handle;
        public TextInvokeJob() {
            handle=CreateJobObject(IntPtr.Zero,null);
            if (handle==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            var limits=new ExtendedLimit(); limits.Basic.Flags=0x2000u;
            if (!SetInformationJobObject(handle,9,ref limits,(uint)Marshal.SizeOf(typeof(ExtendedLimit)))) {
                int error=Marshal.GetLastWin32Error(); Dispose(); throw new Win32Exception(error);
            }
        }
        public void Assign(Process process) {
            if (!AssignProcessToJobObject(handle,process.Handle)) throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        public uint ActiveProcesses() {
            Accounting info;
            if (!QueryInformationJobObject(handle,1,out info,(uint)Marshal.SizeOf(typeof(Accounting)),IntPtr.Zero)) throw new Win32Exception(Marshal.GetLastWin32Error());
            return info.ActiveProcesses;
        }
        public void Terminate() {
            if (!TerminateJobObject(handle,2)) throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        public bool WaitEmpty(int milliseconds) {
            var watch=Stopwatch.StartNew();
            do {
                if (ActiveProcesses()==0) return true;
                System.Threading.Thread.Sleep(20);
            } while (watch.ElapsedMilliseconds<milliseconds);
            return ActiveProcesses()==0;
        }
        public void Dispose() { if (handle!=IntPtr.Zero) { CloseHandle(handle); handle=IntPtr.Zero; } }
    }
    public static class TextInvokeNative {
        public static string JsonString(string value) {
            var result=new StringBuilder(); result.Append('"');
            foreach (char c in value) {
                if (c=='"' || c=='\\') { result.Append('\\'); result.Append(c); }
                else if (c=='\b') result.Append("\\b");
                else if (c=='\f') result.Append("\\f");
                else if (c=='\n') result.Append("\\n");
                else if (c=='\r') result.Append("\\r");
                else if (c=='\t') result.Append("\\t");
                else if (c<32) result.Append("\\u"+((int)c).ToString("x4"));
                else result.Append(c);
            }
            return result.Append('"').ToString();
        }
        private sealed class JsonScope {
            public bool Object, Key;
            public readonly HashSet<string> Names=new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        }
        public static void UniqueJsonKeys(string json) {
            var stack=new Stack<JsonScope>();
            for (int i=0;i<json.Length;i++) {
                char c=json[i];
                if (c=='"') {
                    var value=new StringBuilder(); bool closed=false;
                    while (++i<json.Length) {
                        c=json[i]; if (c=='"') { closed=true; break; }
                        if (c=='\\') {
                            if (++i>=json.Length) throw new IOException("invalid-json");
                            c=json[i];
                            if (c=='u') {
                                if (i+4>=json.Length) throw new IOException("invalid-json");
                                value.Append((char)Convert.ToInt32(json.Substring(i+1,4),16)); i+=4; continue;
                            }
                            if (c=='b') c='\b'; else if (c=='f') c='\f'; else if (c=='n') c='\n';
                            else if (c=='r') c='\r'; else if (c=='t') c='\t';
                            else if (c!='"' && c!='\\' && c!='/') throw new IOException("invalid-json");
                        }
                        value.Append(c);
                    }
                    if (!closed) throw new IOException("invalid-json");
                    if (stack.Count>0 && stack.Peek().Object && stack.Peek().Key) {
                        if (!stack.Peek().Names.Add(value.ToString())) throw new IOException("duplicate-json-field");
                        stack.Peek().Key=false;
                    }
                } else if (c=='{' || c=='[') stack.Push(new JsonScope {Object=c=='{',Key=c=='{'});
                else if (c=='}' || c==']') { if (stack.Count==0) throw new IOException("invalid-json"); stack.Pop(); }
                else if (c==',' && stack.Count>0 && stack.Peek().Object) stack.Peek().Key=true;
            }
            if (stack.Count!=0) throw new IOException("invalid-json");
        }
        private static int Hex(char value) {
            if (value>='0' && value<='9') return value-'0';
            if (value>='a' && value<='f') return value-'a'+10;
            if (value>='A' && value<='F') return value-'A'+10;
            return -1;
        }
        private static bool Escape(string value, int offset, out char decoded, out int length) {
            decoded='\0'; length=0;
            if (offset+1>=value.Length || value[offset]!='\\') return false;
            switch (value[offset+1]) {
                case '"': decoded='"'; break;
                case '\\': decoded='\\'; break;
                case '/': decoded='/'; break;
                case 'b': decoded='\b'; break;
                case 'f': decoded='\f'; break;
                case 'n': decoded='\n'; break;
                case 'r': decoded='\r'; break;
                case 't': decoded='\t'; break;
                case 'u':
                    if (offset+5>=value.Length) return false;
                    int code=0;
                    for (int index=offset+2;index<offset+6;index++) {
                        int digit=Hex(value[index]); if (digit<0) return false;
                        code=code*16+digit;
                    }
                    decoded=(char)code; length=6; return true;
                default: return false;
            }
            length=2; return true;
        }
        private static string DecodeEscapes(string value, out bool changed) {
            changed=false; var result=new StringBuilder(value.Length);
            for (int offset=0;offset<value.Length;offset++) {
                char decoded; int length;
                if (Escape(value,offset,out decoded,out length)) {
                    result.Append(decoded); offset+=length-1; changed=true;
                } else result.Append(value[offset]);
            }
            return changed ? result.ToString() : value;
        }
        public static bool CredentialEchoOrUncertain(string value, string secret) {
            if (String.IsNullOrEmpty(secret)) return false;
            string normalized=secret;
            for (int round=0;round<32;round++) {
                bool changed; normalized=DecodeEscapes(normalized,out changed);
                if (!changed) break;
            }
            // Literal key escapes can decode before a mixed-depth suffix appears.
            // Compare their normalized form too; tiny escaped keys are ambiguous.
            if (normalized.Length==0 || (normalized.Length<4 && normalized!=secret)) return value.Length!=0;
            for (int offset=0;offset<normalized.Length;offset++) {
                char ignored; int length;
                if (Escape(normalized,offset,out ignored,out length)) return value.Length!=0;
            }
            string decoded=value;
            // Decode raw/mixed JSON escapes as well as nested serialized strings.
            // Check each intermediate value before its literal key escapes change.
            for (int round=0;round<32;round++) {
                if (decoded.IndexOf(secret,StringComparison.OrdinalIgnoreCase)>=0 || decoded.IndexOf(normalized,StringComparison.OrdinalIgnoreCase)>=0) return true;
                bool changed; decoded=DecodeEscapes(decoded,out changed);
                if (!changed) return false;
            }
            if (decoded.IndexOf(secret,StringComparison.OrdinalIgnoreCase)>=0 || decoded.IndexOf(normalized,StringComparison.OrdinalIgnoreCase)>=0) return true;
            // A remaining decodable layer exceeds the bound: omit the whole value.
            for (int offset=0;offset<decoded.Length;offset++) {
                char ignored; int length;
                if (Escape(decoded,offset,out ignored,out length)) return true;
            }
            return false;
        }
        [StructLayout(LayoutKind.Sequential)] private struct Info {
            public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh;
            public uint WriteLow, WriteHigh, VolumeSerial, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
        }
        [DllImport("kernel32.dll", SetLastError=true)] private static extern bool GetFileInformationByHandle(SafeFileHandle handle, out Info info);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
        public static bool Ordinary(SafeFileHandle handle, bool directory) {
            Info info; if (!GetFileInformationByHandle(handle, out info)) throw new Win32Exception(Marshal.GetLastWin32Error());
            return (info.Attributes & 0x400u)==0 && ((info.Attributes & 0x10u)!=0)==directory && (directory || info.Links==1);
        }
        public static SafeFileHandle DirectoryGuard(string name) {
            var handle=CreateFile(name,0x80u,3u,IntPtr.Zero,3u,0x02200000u,IntPtr.Zero);
            if (handle.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (!Ordinary(handle,true)) { handle.Dispose(); throw new IOException("directory-link"); }
            return handle;
        }
        public static async Task<string> ReadBounded(Stream stream, int maximum) {
            using (var memory=new MemoryStream()) {
                var buffer=new byte[8192]; int count;
                while ((count=await stream.ReadAsync(buffer,0,buffer.Length).ConfigureAwait(false))!=0) {
                    if (memory.Length+count>maximum) throw new IOException("output-limit");
                    memory.Write(buffer,0,count);
                }
                return new UTF8Encoding(false,true).GetString(memory.ToArray());
            }
        }
    }
}
'@
}
$guards = New-Object 'Collections.Generic.List[IDisposable]'
function Selected-Path([string]$Value, [switch]$External) {
    if ($Value -notmatch '^[A-Za-z]:[\\/]' -or $Value.Substring(2).Contains(':')) { throw 'path-invalid' }
    foreach ($part in ($Value.Substring(3) -split '[\\/]')) {
        if ($part -in @('.', '..') -or $part -match '[. ]$' -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'path-invalid' }
    }
    $full = [IO.Path]::GetFullPath($Value)
    $repository = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\','/')
    if ($External -and [IO.File]::Exists([IO.Path]::Combine($repository,'Cargo.toml')) -and ($full.Equals($repository,[StringComparison]::OrdinalIgnoreCase) -or $full.StartsWith($repository+'\',[StringComparison]::OrdinalIgnoreCase))) { throw 'path-in-repository' }
    $parents = New-Object 'Collections.Generic.List[string]'
    $parent = [IO.Path]::GetDirectoryName($full)
    while ($parent) { $parents.Add($parent); $next=[IO.Path]::GetDirectoryName($parent); if ($next -eq $parent) { break }; $parent=$next }
    for ($i=$parents.Count-1; $i -ge 0; $i--) { $guards.Add([Noetome.TextInvokeNative]::DirectoryGuard($parents[$i])) }
    return $full
}
function Open-Selected([string]$Value, [switch]$Write, [switch]$External) {
    $full=Selected-Path $Value -External:$External
    $access=if ($Write) { [IO.FileAccess]::ReadWrite } else { [IO.FileAccess]::Read }
    $stream=[IO.File]::Open($full,[IO.FileMode]::Open,$access,[IO.FileShare]::Read)
    try { if (-not [Noetome.TextInvokeNative]::Ordinary($stream.SafeFileHandle,$false)) { throw 'path-link' }; $guards.Add($stream); return $stream }
    catch { $stream.Dispose(); throw }
}
function Read-Selected($Stream, [long]$Maximum) {
    if ($Stream.Length -lt 1 -or $Stream.Length -gt $Maximum) { throw 'file-limit' }
    $Stream.Position=0
    $reader=New-Object IO.StreamReader($Stream,(New-Object Text.UTF8Encoding($false,$true)),$false,1024,$true)
    try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
}
function Parse-JsonText([string]$Value) {
    $command=Get-Command -Name ConvertFrom-Json -CommandType Cmdlet
    if ($command.Parameters.ContainsKey('DateKind')) { return ($Value | ConvertFrom-Json -DateKind String) }
    # Windows PowerShell 5.1 preserves JSON strings. Older Core versions may
    # convert dates without an option to preserve them; never rewrite history.
    if ($PSVersionTable.PSVersion.Major -ge 6) { throw 'json-string-preservation-unavailable' }
    return ($Value | ConvertFrom-Json)
}
function Native-Argument([string]$Value) {
    return '"'+[regex]::Replace([regex]::Replace($Value,'(\\*)"','$1$1\"'),'(\\+)$','$1$1')+'"'
}
function Digest-Stream($Stream) {
    $algorithm=[Security.Cryptography.SHA256]::Create()
    try { $Stream.Position=0; return (-join ($algorithm.ComputeHash($Stream) | ForEach-Object { $_.ToString('x2') })) }
    finally { $algorithm.Dispose() }
}
function Bytes-Stream($Stream) {
    if ($Stream.Length -lt 1 -or $Stream.Length -gt 1048576) { throw 'file-limit' }
    $Stream.Position=0; $bytes=New-Object byte[] ([int]$Stream.Length); $offset=0
    while ($offset -lt $bytes.Length) {
        $count=$Stream.Read($bytes,$offset,$bytes.Length-$offset)
        if ($count -eq 0) { throw 'file-short-read' }; $offset += $count
    }
    return ,$bytes
}
function Digest-Bytes([byte[]]$Bytes) {
    $algorithm=[Security.Cryptography.SHA256]::Create()
    try { return (-join ($algorithm.ComputeHash($Bytes) | ForEach-Object { $_.ToString('x2') })) }
    finally { $algorithm.Dispose() }
}
function Save-New([string]$Path,[byte[]]$Bytes) {
    $stream=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $stream.Write($Bytes,0,$Bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
    return [pscustomobject]@{path=$Path;sha256=(Digest-Bytes $Bytes);byte_length=$Bytes.Length}
}
function Save-Phase([string]$Phase) {
    $prefix=$UsageLedgerFile+'.text-'+$record.invocation_id
    $snapshot=Save-New ($prefix+'.'+$Phase+'-ledger.json') (Bytes-Stream $ledgerStream)
    $launch=$null; $result=$null; $settlement=$null
    if ($started) { $launch=[pscustomobject]@{pid=$process.Id;started_at=$process.StartTime.ToUniversalTime().ToString('o')} }
    if ($null -ne $stdout -and $null -ne $stderr) {
        $originalOut=(New-Object Text.UTF8Encoding($false)).GetBytes($stdout)
        $originalErr=(New-Object Text.UTF8Encoding($false)).GetBytes($stderr)
        $safeOut=Hide-Credential $stdout $plaintext; $safeErr=Hide-Credential $stderr $plaintext
        $outCopy=Save-New ($prefix+'.stdout.txt') ((New-Object Text.UTF8Encoding($false)).GetBytes($safeOut))
        $errCopy=Save-New ($prefix+'.stderr.txt') ((New-Object Text.UTF8Encoding($false)).GetBytes($safeErr))
        $outOmission=if ($safeOut -ceq $stdout) { $null } else { 'credential_echo_redacted' }
        $errOmission=if ($safeErr -ceq $stderr) { $null } else { 'credential_echo_redacted' }
        $result=[pscustomobject]@{cli_exit_code=$exitCode;stdout=[pscustomobject]@{original_sha256=(Digest-Bytes $originalOut);original_byte_length=$originalOut.Length;omission_reason=$outOmission;saved_copy=$outCopy};stderr=[pscustomobject]@{original_sha256=(Digest-Bytes $originalErr);original_byte_length=$originalErr.Length;omission_reason=$errOmission;saved_copy=$errCopy}}
        $settlement=[pscustomobject]@{record_status=$record.status;confirmed_total_tokens=$record.confirmed_total_tokens;network_attempts=$record.network_attempts}
    }
    $receipt=[pscustomobject]@{schema_version=1;kind='text_api_credential_invocation';invocation_id=$record.invocation_id;phase=$Phase;recorded_at=[DateTime]::UtcNow.ToString('o');usage_ledger_reference=$UsageLedgerFile;ledger_snapshot=$snapshot;selected_binary=[pscustomobject]@{path=$Binary;sha256=$record.binary_sha256;byte_length=$binaryGuard.Length};selected_config=[pscustomobject]@{path=$Config;sha256=$record.config_sha256;byte_length=$configStream.Length};project=$Project;import_id=$ImportId;output_root=$OutputRoot;reservation=[pscustomobject]@{policy=$record.reservation_policy;max_total_tokens=$record.reservation_tokens};launch=$launch;result=$result;settlement=$settlement}
    if ($Phase -eq 'failed') {
        $receipt | Add-Member -NotePropertyName failure -NotePropertyValue ([pscustomobject]@{code=$diagnostic;stdout_complete=($null -ne $stdoutTask -and $stdoutTask.Status -eq [Threading.Tasks.TaskStatus]::RanToCompletion);stderr_complete=($null -ne $stderrTask -and $stderrTask.Status -eq [Threading.Tasks.TaskStatus]::RanToCompletion);termination=$termination})
    }
    $null=Save-New ($prefix+'.'+$Phase+'.json') ((New-Object Text.UTF8Encoding($false)).GetBytes(($receipt | ConvertTo-Json -Depth 100)+"`n"))
}
function Integer($Value, [long]$Maximum) {
    return (($Value -is [int] -or $Value -is [long]) -and $Value -ge 0 -and $Value -le $Maximum)
}
function Canonical-Data($Value, [int]$Depth=0) {
    if ($Depth -gt 64) { throw 'ledger-depth' }
    if ($null -eq $Value) { return 'null' }
    if ($Value -is [string]) { return [Noetome.TextInvokeNative]::JsonString($Value) }
    if ($Value -is [bool]) { return $Value.ToString().ToLowerInvariant() }
    if ($Value -is [int] -or $Value -is [long]) { return $Value.ToString([Globalization.CultureInfo]::InvariantCulture) }
    if ($Value -is [Array]) { return '['+(@($Value | ForEach-Object { Canonical-Data $_ ($Depth+1) }) -join ',')+']' }
    if ($Value -isnot [pscustomobject]) { throw 'ledger-value-type' }
    [string[]]$names=@($Value.PSObject.Properties.Name)
    [Array]::Sort($names,[StringComparer]::Ordinal)
    $fields=@($names | ForEach-Object { [Noetome.TextInvokeNative]::JsonString($_)+':'+(Canonical-Data $Value.$_ ($Depth+1)) })
    return '{'+($fields -join ',')+'}'
}
function Check-Ledger($Ledger) {
    $fields=@('schema_version','approved_total_tokens','accounted_tokens','pending_reserved_tokens','remaining_tokens','model_attempts','records','updated_at')
    if (@($Ledger.PSObject.Properties.Name).Count -ne $fields.Count) { throw 'ledger-fields' }
    foreach ($name in $fields) { if ($name -notin $Ledger.PSObject.Properties.Name) { throw 'ledger-fields' } }
    if (-not (Integer $Ledger.schema_version 1) -or $Ledger.schema_version -ne 1 -or -not (Integer $Ledger.approved_total_tokens 10000000) -or $Ledger.approved_total_tokens -ne 10000000) { throw 'ledger-policy' }
    foreach ($name in @('accounted_tokens','pending_reserved_tokens','remaining_tokens','model_attempts')) {
        if (-not (Integer $Ledger.$name 10000000)) { throw 'ledger-number' }
    }
    if ($Ledger.remaining_tokens -ne ($Ledger.approved_total_tokens-$Ledger.accounted_tokens-$Ledger.pending_reserved_tokens)) { throw 'ledger-arithmetic' }
    [long]$confirmed=0; [long]$pending=0; [long]$attempts=0
    $openingIds=@('279ccd52-f21d-4e5b-92ed-fdf71c5b1741','992fe32d-a5f6-4b61-95c5-a1cae3b3fb8f','bc077795-1bbc-458f-9491-18a8c67aea5b','6ecd7089-4202-4e0e-bc21-7c2115aca5fc','c087468a-672a-4f74-8c5b-ad395ef86ead','8638efce-1021-445e-81db-7638e48cf140','3c0e4214-d98e-438f-950a-36e88e63e22b','7e4aacb4-12e1-46c0-a787-fef1d7a69d75','fd3b0182-5efc-49ab-b646-8efe31c03794','f96f375d-f1cc-425e-a459-6d06dd05ea90')
    $openingTokens=@(936,9683,4912,4783,3479,6304,654,776,1070,588)
    $openingHashes=@('d147013368ca60cbd524d02ef8273feaac6ed3c47a2001f284afd7dd8674982d','a38c5074d5604699a506518a316c09ba0a18bc930d3b914ac254368f28172714','d27eb999c036811c4822ffa1933b7642d80d686c7d193e86ba2647d47e32d245','5462a9765a60ba7a5b88bfba54cff00851c50f9fc1a1f0b149a9bb94d8955558','3b80e37aa958c25c1416bdd90d93bb029a9f47c61b606d7a09d16b69eb3f59bb','22a365714d8bf77648b0951b3eb1b7c9fed1c86ee63df9fa101c7d5953c95047','b316201e8f50c9bb14bc763f96cf69c655a518ea7687d6331aa3640973c1acc0','2f67dde84336d78268a2ffd28f6115f1f011adff1debb1d25b329e6fd34f118c','8651bcf210ff84d2829f6875dec46f4509d52e81a90e4d0ef609cca73faba38d','44cc6918f6aad45999dcbadd25415dd5a1aa0d422409afab24fbc4e8fbea2ad4')
    if ($Ledger.records -isnot [Array] -or @($Ledger.records).Count -lt 10) { throw 'ledger-opening-missing' }
    for ($index=0; $index -lt 10; $index++) {
        if ($Ledger.records[$index].invocation_id -cne $openingIds[$index] -or $Ledger.records[$index].confirmed_total_tokens -ne $openingTokens[$index] -or $Ledger.records[$index].network_attempts -ne 1) { throw 'ledger-opening-changed' }
        $canonical=(New-Object Text.UTF8Encoding($false)).GetBytes((Canonical-Data $Ledger.records[$index]))
        if ((Digest-Bytes $canonical) -cne $openingHashes[$index]) { throw 'ledger-opening-changed' }
    }
    $identities=New-Object 'Collections.Generic.HashSet[string]'
    foreach ($record in @($Ledger.records)) {
        if ($record.invocation_id -isnot [string] -or $record.invocation_id -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$' -or -not $identities.Add($record.invocation_id) -or -not (Integer $record.reservation_tokens 10000000) -or $record.reservation_tokens -lt 1) { throw 'ledger-record' }
        if ($null -ne $record.network_attempts) {
            if (-not (Integer $record.network_attempts 1)) { throw 'ledger-attempts' }
            $attempts += $record.network_attempts
        }
        if ($null -eq $record.confirmed_total_tokens) {
            if ($record.status -cnotin @('reserved','usage_unknown') -or $record.network_attempts -eq 0) { throw 'ledger-unknown-record' }
            $pending += $record.reservation_tokens
        } else {
            if (-not (Integer $record.confirmed_total_tokens 10000000)) { throw 'ledger-record' }
            if ($record.status -ceq 'settled_no_request') {
                if ($record.network_attempts -ne 0 -or $record.confirmed_total_tokens -ne 0) { throw 'ledger-no-request-record' }
            } elseif ($record.status -cnotin @('settled_valid_usage_response_failed','settled_valid_usage_candidates_validated','settled_valid_usage_scan','settled_valid_usage_text') -or $record.network_attempts -ne 1) { throw 'ledger-settled-record' }
            $confirmed += $record.confirmed_total_tokens
        }
    }
    if ($confirmed -ne $Ledger.accounted_tokens -or $pending -ne $Ledger.pending_reserved_tokens -or $attempts -ne $Ledger.model_attempts) { throw 'ledger-record-arithmetic' }
}
function Write-Ledger($Stream,$Ledger) {
    $Ledger.updated_at=[DateTime]::UtcNow.ToString('o')
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($Ledger | ConvertTo-Json -Depth 100)+"`n")
    if ($bytes.Length -gt 1048576) { throw 'ledger-limit' }
    $Stream.Position=0; $Stream.SetLength(0); $Stream.Write($bytes,0,$bytes.Length); $Stream.Flush($true)
}
function Hide-Credential([string]$Value,[string]$Secret) {
    if ([Noetome.TextInvokeNative]::CredentialEchoOrUncertain($Value,$Secret)) { return '[redacted]' }
    return $Value
}
function Emit-Utf8([string]$Value,[switch]$ErrorStream) {
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes($Value)
    $stream=if ($ErrorStream) { [Console]::OpenStandardError() } else { [Console]::OpenStandardOutput() }
    $stream.Write($bytes,0,$bytes.Length); $stream.Flush()
}
function Stop-SelectedChild {
    $before=$null; $after=$null; $empty=$false; $rootExited=$false
    $strategy=if ($contained) { 'windows_job' } else { 'taskkill_uncontained_root' }
    try {
        if ($contained) { $before=$job.ActiveProcesses(); $job.Terminate(); $empty=$job.WaitEmpty(5000); $after=$job.ActiveProcesses() }
        elseif (-not $process.HasExited) { & ([IO.Path]::Combine($env:SystemRoot,'System32','taskkill.exe')) /PID $process.Id /T /F *> $null }
        $rootExited=$process.WaitForExit(5000)
    } catch { }
    return [pscustomobject]@{strategy=$strategy;root_pid=$process.Id;containment_assignment='after_process_start';active_processes_before=$before;active_processes_after=$after;job_empty_confirmed=$empty;root_exit_confirmed=$rootExited}
}

