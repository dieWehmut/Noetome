# One selected scan CLI invocation; DPAPI stays in memory and the shared quota is never reset.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [Parameter(Mandatory=$true)][string]$CaseDirectory,
    [Parameter(Mandatory=$true)][string]$ApiConfig,
    [Parameter(Mandatory=$true)][string]$Profile,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [string]$CredentialFile,
    [string]$UsageLedgerFile
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not ('Noetome.ScanInvokeNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
namespace Noetome {
    public static class ScanInvokeNative {
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
    if ($External -and ($full.Equals($repository,[StringComparison]::OrdinalIgnoreCase) -or $full.StartsWith($repository+'\',[StringComparison]::OrdinalIgnoreCase))) { throw 'path-in-repository' }
    $parents = New-Object 'Collections.Generic.List[string]'
    $parent = [IO.Path]::GetDirectoryName($full)
    while ($parent) { $parents.Add($parent); $next=[IO.Path]::GetDirectoryName($parent); if ($next -eq $parent) { break }; $parent=$next }
    for ($i=$parents.Count-1; $i -ge 0; $i--) { $guards.Add([Noetome.ScanInvokeNative]::DirectoryGuard($parents[$i])) }
    return $full
}
function Open-Selected([string]$Value, [switch]$Write, [switch]$External) {
    $full=Selected-Path $Value -External:$External
    $access=if ($Write) { [IO.FileAccess]::ReadWrite } else { [IO.FileAccess]::Read }
    $stream=[IO.File]::Open($full,[IO.FileMode]::Open,$access,[IO.FileShare]::Read)
    try { if (-not [Noetome.ScanInvokeNative]::Ordinary($stream.SafeFileHandle,$false)) { throw 'path-link' }; $guards.Add($stream); return $stream }
    catch { $stream.Dispose(); throw }
}
function Read-Selected($Stream, [long]$Maximum) {
    if ($Stream.Length -lt 1 -or $Stream.Length -gt $Maximum) { throw 'file-limit' }
    $Stream.Position=0
    $reader=New-Object IO.StreamReader($Stream,(New-Object Text.UTF8Encoding($false,$true)),$false,1024,$true)
    try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
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
    $prefix=$UsageLedgerFile+'.scan-'+$record.invocation_id
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
    $receipt=[pscustomobject]@{schema_version=1;kind='scan_api_credential_invocation';invocation_id=$record.invocation_id;phase=$Phase;recorded_at=[DateTime]::UtcNow.ToString('o');usage_ledger_reference=$UsageLedgerFile;ledger_snapshot=$snapshot;selected_binary=[pscustomobject]@{path=$Binary;sha256=$record.binary_sha256;byte_length=$binaryGuard.Length};selected_config=[pscustomobject]@{path=$ApiConfig;sha256=$record.config_sha256;byte_length=$configStream.Length};case_directory=$CaseDirectory;profile=$Profile;output_root=$OutputRoot;reservation=[pscustomobject]@{policy=$record.reservation_policy;max_total_tokens=$record.reservation_tokens};launch=$launch;result=$result;settlement=$settlement}
    $null=Save-New ($prefix+'.'+$Phase+'.json') ((New-Object Text.UTF8Encoding($false)).GetBytes(($receipt | ConvertTo-Json -Depth 100)+"`n"))
}
function Integer($Value, [long]$Maximum) {
    return (($Value -is [int] -or $Value -is [long]) -and $Value -ge 0 -and $Value -le $Maximum)
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
    [long]$confirmed=0; [long]$pending=0
    $identities=New-Object 'Collections.Generic.HashSet[string]'
    foreach ($record in @($Ledger.records)) {
        if (-not $identities.Add([string]$record.invocation_id) -or -not (Integer $record.reservation_tokens 10000000)) { throw 'ledger-record' }
        if ($null -eq $record.confirmed_total_tokens) { $pending += $record.reservation_tokens }
        else { if (-not (Integer $record.confirmed_total_tokens 10000000)) { throw 'ledger-record' }; $confirmed += $record.confirmed_total_tokens }
    }
    if ($confirmed -ne $Ledger.accounted_tokens -or $pending -ne $Ledger.pending_reserved_tokens) { throw 'ledger-record-arithmetic' }
}
function Write-Ledger($Stream,$Ledger) {
    $Ledger.updated_at=[DateTime]::UtcNow.ToString('o')
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($Ledger | ConvertTo-Json -Depth 100)+"`n")
    if ($bytes.Length -gt 1048576) { throw 'ledger-limit' }
    $Stream.Position=0; $Stream.SetLength(0); $Stream.Write($bytes,0,$bytes.Length); $Stream.Flush($true)
}
function Hide-Credential([string]$Value,[string]$Secret) {
    if ([string]::IsNullOrEmpty($Secret)) { return $Value }
    $parts=foreach ($character in $Secret.ToCharArray()) {
        $literal=[regex]::Escape([string]$character)
        $json=(([string]$character | ConvertTo-Json -Compress).Trim('"'))
        '(?:'+$literal+'|'+[regex]::Escape($json)+'|\\u'+([int]$character).ToString('x4')+')'
    }
    return [regex]::Replace($Value,($parts -join ''),'[redacted]',[Text.RegularExpressions.RegexOptions]::IgnoreCase)
}

$secure=$null; $pointer=[IntPtr]::Zero; $plaintext=$null; $process=$null; $start=$null
$ledgerStream=$null; $ledger=$null; $record=$null; $diagnostic='operation-failed'; $exitCode=2
$started=$false
$reservationApplied=$false; $stdout=$null; $stderr=$null
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { $diagnostic='windows-required'; throw 'stop' }
    $diagnostic='configuration-invalid'
    $configStream=Open-Selected $ApiConfig
    $configText=Read-Selected $configStream 65536
    [Noetome.ScanInvokeNative]::UniqueJsonKeys($configText)
    $configuration=$configText | ConvertFrom-Json
    $environmentName=$configuration.credential_env
    $live=$configuration.transport_mode -ceq 'live'
    if ($live) {
        if ($environmentName -cne 'NOETOME_TANGMAO_API_KEY' -or $configuration.endpoint -cne 'https://ai.1343263.xyz/v1/chat/completions' -or $configuration.model -cne 'deepseek-flash') { throw 'stop' }
    } elseif ($configuration.transport_mode -cne 'local_http_test' -or $environmentName -cne 'NOETOME_TEST_SCAN_CREDENTIAL') { throw 'stop' }
    if ($configuration.provider -cne 'openai_chat_completions' -or $configuration.required_input -cne 'vision' -or -not (Integer $configuration.budget.max_requests 1) -or -not (Integer $configuration.budget.max_total_tokens 10000000) -or $null -ne $configuration.budget.max_cost_usd -or -not (Integer $configuration.limits.timeout_ms 60000) -or $configuration.limits.timeout_ms -eq 0) { throw 'stop' }
    $diagnostic='credential-missing'
    if ($CredentialFile) {
        $credentialStream=Open-Selected $CredentialFile -External
        $secure=(Read-Selected $credentialStream 65536) | ConvertTo-SecureString
        $pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
        $plaintext=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    } else { $plaintext=[Environment]::GetEnvironmentVariable($environmentName,'Process') }
    if ([string]::IsNullOrEmpty($plaintext) -or $plaintext.Length -gt 8192 -or $plaintext -cnotmatch '^[\x21-\x7e]+$') { throw 'stop' }
    $diagnostic='binary-invalid'; $binaryGuard=Open-Selected $Binary
    if ($live -and $configuration.budget.max_requests -eq 1) {
        $diagnostic='ledger-missing'
        if (-not $UsageLedgerFile) { throw 'stop' }
        $ledgerStream=Open-Selected $UsageLedgerFile -Write -External
        $originalLedger=Read-Selected $ledgerStream 1048576
        [Noetome.ScanInvokeNative]::UniqueJsonKeys($originalLedger)
        $ledger=$originalLedger | ConvertFrom-Json; Check-Ledger $ledger
        [long]$reservation=$configuration.budget.max_total_tokens
        if ($reservation -lt 1 -or $reservation -gt $ledger.remaining_tokens) { $diagnostic='budget-stop'; throw 'stop' }
        $id=[Guid]::NewGuid().ToString()
        $record=[pscustomobject]@{invocation_id=$id;task_id='B06V-T02';reserved_at=[DateTime]::UtcNow.ToString('o');status='reserved';endpoint=$configuration.endpoint;configured_model=$configuration.model;request_strategy='tangmao-vision-scan-v1';reservation_tokens=$reservation;reservation_policy='bounded_request_total_cap';confirmed_total_tokens=$null;network_attempts=$null;config_reference=$ApiConfig;config_sha256=(Digest-Stream $configStream);binary_reference=$Binary;binary_sha256=(Digest-Stream $binaryGuard);case_reference=$CaseDirectory;profile_reference=$Profile;output_root=$OutputRoot;actual_model=$null;response_sha256=$null;price_usd=$null;reason='One explicit scan CLI request; unknown usage retains reservation.'}
        $backupPath=$UsageLedgerFile+'.scan-before-'+$id+'.json'
        $null=Save-New $backupPath (Bytes-Stream $ledgerStream)
        Save-Phase 'before'
        $ledger.records=@($ledger.records)+@($record); $ledger.pending_reserved_tokens += $reservation; $ledger.remaining_tokens -= $reservation
        $reservationApplied=$true
        Write-Ledger $ledgerStream $ledger
        Save-Phase 'reserved'
    }
    $diagnostic='launch-failed'
    $start=New-Object Diagnostics.ProcessStartInfo
    $start.FileName=$Binary
    $start.Arguments=(@('knowledge','scan-formula',$CaseDirectory,$ApiConfig,$Profile,$OutputRoot) | ForEach-Object { Native-Argument $_ }) -join ' '
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $start.EnvironmentVariables[$environmentName]=$plaintext
    $process=New-Object Diagnostics.Process; $process.StartInfo=$start
    if (-not $process.Start()) { throw 'stop' }
    $started=$true
    if ($reservationApplied) { Save-Phase 'launch' }
    $stdoutTask=[Noetome.ScanInvokeNative]::ReadBounded($process.StandardOutput.BaseStream,16777216)
    $stderrTask=[Noetome.ScanInvokeNative]::ReadBounded($process.StandardError.BaseStream,1048576)
    $deadline=[DateTime]::UtcNow.AddMilliseconds($configuration.limits.timeout_ms+10000)
    while (-not $process.WaitForExit(50)) {
        if ($stdoutTask.IsFaulted -or $stderrTask.IsFaulted) { $diagnostic='child-output-limit'; throw 'stop' }
        if ([DateTime]::UtcNow -gt $deadline) { $diagnostic='child-timeout'; throw 'stop' }
    }
    $stdout=$stdoutTask.GetAwaiter().GetResult(); $stderr=$stderrTask.GetAwaiter().GetResult(); $exitCode=$process.ExitCode
    if ($null -ne $record) {
        $record.status='usage_unknown'
        try {
            [Noetome.ScanInvokeNative]::UniqueJsonKeys($stdout)
            $report=$stdout | ConvertFrom-Json
            $api=$report.api
            if ($api.PSObject.Properties.Name -contains 'actual_model' -and $api.actual_model -is [string]) { $record.actual_model=Hide-Credential $api.actual_model $plaintext }
            if ($api.PSObject.Properties.Name -contains 'response_receipt' -and $null -ne $api.response_receipt -and $api.response_receipt.sha256 -cmatch '^[0-9a-f]{64}$') { $record.response_sha256=$api.response_receipt.sha256 }
            if (Integer $api.usage.network_attempts 1) {
                $record.network_attempts=$api.usage.network_attempts
                $ledger.model_attempts += $api.usage.network_attempts
                if ($api.usage.network_attempts -eq 0) {
                    $record.confirmed_total_tokens=0; $record.status='settled_no_request'
                } elseif ($api.usage.model_usage_status -ceq 'validated') {
                    $usage=$api.usage.model_usage
                    if ((Integer $usage.input_tokens 10000000) -and (Integer $usage.output_tokens 10000000) -and (Integer $usage.total_tokens 10000000) -and $usage.total_tokens -eq ($usage.input_tokens+$usage.output_tokens)) {
                        $record.confirmed_total_tokens=$usage.total_tokens; $record.status='settled_valid_usage_scan'; $ledger.accounted_tokens += $usage.total_tokens
                    }
                }
            }
        } catch { }
        if ($null -ne $record.confirmed_total_tokens) {
            $ledger.pending_reserved_tokens -= $record.reservation_tokens
            $ledger.remaining_tokens=$ledger.approved_total_tokens-$ledger.accounted_tokens-$ledger.pending_reserved_tokens
        }
        Write-Ledger $ledgerStream $ledger
        Save-Phase 'settled'
    }
    [Console]::Out.Write((Hide-Credential $stdout $plaintext)); [Console]::Error.Write((Hide-Credential $stderr $plaintext))
} catch {
    if ($reservationApplied) {
        try { if ($null -eq $record.confirmed_total_tokens) { $record.status='usage_unknown' }; Write-Ledger $ledgerStream $ledger } catch { }
    }
    [Console]::Error.WriteLine('scan-credential-invoke: '+$diagnostic); $exitCode=2
} finally {
    if ($started -and -not $process.HasExited) {
        try { & ([IO.Path]::Combine($env:SystemRoot,'System32','taskkill.exe')) /PID $process.Id /T /F *> $null; $process.WaitForExit(5000) | Out-Null } catch { }
    }
    if ($null -ne $start) { $start.EnvironmentVariables.Remove($environmentName) }
    $plaintext=$null; $stdout=$null; $stderr=$null
    if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
    if ($null -ne $secure) { $secure.Dispose() }
    if ($null -ne $process) { $process.Dispose() }
    for ($i=$guards.Count-1; $i -ge 0; $i--) { $guards[$i].Dispose() }
}
exit $exitCode
