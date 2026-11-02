# Development helper for one explicitly configured small-text API request.
# Invoke with PowerShell 7 -NoProfile -STA -File. Dot-sourcing only defines functions.
# No credential parameter exists at the script entry point. This is not a product GUI.
[CmdletBinding()]
param(
    [string] $CliPath,
    [string] $ExpectedCliSha256,
    [string] $ProjectPath,
    [string] $VersionId,
    [string] $ConfigPath,
    [string] $AuthorizationPath,
    [string] $OutputPath,
    [string] $PythonPath,
    [string] $StatusPath
)

function Initialize-NoetomeKeyTypes {
    if ('NoetomeKeyLocal.OwnedStatus' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

namespace NoetomeKeyLocal {
    public sealed class OneShotGate {
        private int attempted;
        public bool TryEnter() { return Interlocked.CompareExchange(ref attempted, 1, 0) == 0; }
        public bool Attempted { get { return Volatile.Read(ref attempted) != 0; } }
    }
    internal static class Identity {
        [StructLayout(LayoutKind.Sequential)]
        internal struct Info {
            public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh;
            public uint WriteLow, WriteHigh, Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
        }
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetFileInformationByHandle(SafeFileHandle h, out Info info);
        internal static Info Read(FileStream file) {
            Info info;
            if (!GetFileInformationByHandle(file.SafeFileHandle, out info))
                throw new InvalidOperationException("unsafe_path");
            return info;
        }
        internal static bool Same(Info a, Info b) {
            return a.Volume == b.Volume && a.IndexHigh == b.IndexHigh && a.IndexLow == b.IndexLow;
        }
    }
    public static class SafeFiles {
        public static void CheckAncestors(string path) {
            string current = Path.GetFullPath(path);
            while (current != null) {
                if (File.Exists(current) || Directory.Exists(current)) {
                    if ((File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0)
                        throw new InvalidOperationException("unsafe_path");
                }
                current = Path.GetDirectoryName(current);
            }
        }
        public static FileStream OpenVerifiedCli(string path, string expectedSha) {
            CheckAncestors(path);
            FileStream stream = null;
            try {
                stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
                if (Identity.Read(stream).Links != 1) throw new InvalidOperationException("unsafe_path");
                // Check DOS/PE byte markers and the expected hash, not Authenticode trust.
                using (var reader = new BinaryReader(stream, Encoding.UTF8, true)) {
                    if (stream.Length < 64 || reader.ReadUInt16() != 0x5A4D)
                        throw new InvalidOperationException("cli_not_pe");
                    stream.Position = 0x3c;
                    uint offset = reader.ReadUInt32();
                    if (offset < 64 || offset > stream.Length - 24)
                        throw new InvalidOperationException("cli_not_pe");
                    stream.Position = offset;
                    if (reader.ReadUInt32() != 0x00004550)
                        throw new InvalidOperationException("cli_not_pe");
                }
                stream.Position = 0;
                using (var sha = SHA256.Create()) {
                    string actual = Convert.ToHexString(sha.ComputeHash(stream));
                    if (!String.Equals(actual, expectedSha, StringComparison.OrdinalIgnoreCase))
                        throw new InvalidOperationException("cli_hash_mismatch");
                }
                stream.Position = 0;
                return stream; // Held until the only child exits; replacement is denied meanwhile.
            } catch { if (stream != null) stream.Dispose(); throw; }
        }
    }
    public sealed class OwnedStatus : IDisposable {
        private readonly string path;
        private Identity.Info owned;
        public OwnedStatus(string path) {
            this.path = path;
            using (var f = new FileStream(path, FileMode.CreateNew, FileAccess.ReadWrite, FileShare.Read)) {
                byte[] data = Encode("waiting_for_key", null, null, null);
                f.Write(data, 0, data.Length);
                f.Flush(true);
                owned = Identity.Read(f);
            }
        }
        private static byte[] Encode(string phase, int? pid, int? exitCode, string error) {
            // PowerShell maps its $null to String.Empty when calling a string parameter.
            if (error == String.Empty) error = null;
            if (phase != "waiting_for_key" && phase != "running" && phase != "finished"
                && phase != "cancelled" && phase != "failed")
                throw new InvalidOperationException("invalid_status");
            if (error != null && error != "preflight_failed" && error != "child_start_failed"
                && error != "cli_failed" && error != "child_pipe_failed" && error != "status_publish_failed")
                throw new InvalidOperationException("invalid_status");
            string json = "{\"schema_version\":1,\"phase\":\"" + phase + "\",\"pid\":"
                + (pid.HasValue ? pid.Value.ToString(System.Globalization.CultureInfo.InvariantCulture) : "null")
                + ",\"exit_code\":" + (exitCode.HasValue ? exitCode.Value.ToString(System.Globalization.CultureInfo.InvariantCulture) : "null")
                + ",\"error_code\":" + (error == null ? "null" : "\"" + error + "\"") + "}\n";
            return new UTF8Encoding(false).GetBytes(json);
        }
        public void AssertOwned() {
            try {
                SafeFiles.CheckAncestors(path);
                using (var f = new FileStream(path, FileMode.Open, FileAccess.Read,
                    FileShare.ReadWrite | FileShare.Delete)) {
                    var info = Identity.Read(f);
                    if (info.Links != 1 || !Identity.Same(owned, info))
                        throw new InvalidOperationException("status_ownership_lost");
                }
            } catch (IOException e) when (IsSharingConflict(e)) { throw; }
            catch { throw new InvalidOperationException("status_ownership_lost"); }
        }
        private static bool IsSharingConflict(IOException e) {
            int code = e.HResult & 0xffff;
            return code == 32 || code == 33;
        }
        private static void AssertTemporaryOwned(string temporary, Identity.Info expected) {
            try {
                SafeFiles.CheckAncestors(temporary);
                using (var f = new FileStream(temporary, FileMode.Open, FileAccess.Read,
                    FileShare.ReadWrite | FileShare.Delete)) {
                    var actual = Identity.Read(f);
                    if (actual.Links != 1 || !Identity.Same(expected, actual))
                        throw new InvalidOperationException("status_ownership_lost");
                }
            } catch { throw new InvalidOperationException("status_ownership_lost"); }
        }
        // A sharing reader is transient contention, never permission to overwrite a new owner.
        public bool TryWrite(string phase, int? pid, int? exitCode, string error) {
            byte[] data = Encode(phase, pid, exitCode, error);
            try { AssertOwned(); }
            catch (IOException e) when (IsSharingConflict(e)) { return false; }
            string temporary = Path.Combine(Path.GetDirectoryName(path),
                ".noetome-key-status-" + Guid.NewGuid().ToString("N") + ".tmp");
            bool created = false;
            bool replaceAttempted = false;
            Identity.Info replacement = default;
            try {
                using (var f = new FileStream(temporary, FileMode.CreateNew, FileAccess.ReadWrite, FileShare.Read)) {
                    created = true;
                    f.Write(data, 0, data.Length);
                    f.Flush(true);
                    // Recheck after writing the temporary file and only replace our own state.
                    AssertOwned();
                    replacement = Identity.Read(f);
                }
                replaceAttempted = true;
                File.Replace(temporary, path, null);
                // Keep the prior identity if Replace fails. Only a committed replace owns this ID.
                owned = replacement;
                created = false;
                return true;
            } catch (IOException e) when (replaceAttempted && (e.HResult & 0xffff) == 1175) {
                // ReplaceFile 1175 retains both names. Retry only after both native owners match.
                // Other partial replacement errors (1176/1177) remain failures.
                try { AssertTemporaryOwned(temporary, replacement); }
                catch { created = false; throw; } // Preserve an unknown replacement, never delete it.
                AssertOwned();
                return false;
            } catch (IOException e) when (IsSharingConflict(e)) {
                return false;
            } finally { if (created) File.Delete(temporary); }
        }
        public void Write(string phase, int? pid, int? exitCode, string error) {
            if (!TryWrite(phase, pid, exitCode, error)) throw new IOException("status_busy");
        }
        public void Dispose() { /* Preserve the fixed-schema result for the caller. */ }
    }
    public static class Pipes {
        public static async Task DiscardAsync(StreamReader reader) {
            char[] buffer = new char[2048];
            try {
                while (await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false) != 0)
                    Array.Clear(buffer, 0, buffer.Length);
            } finally { Array.Clear(buffer, 0, buffer.Length); }
        }
    }
}
'@ -ErrorAction Stop | Out-Null
}

function Get-NoetomeFixedException {
    param($Record, [string] $Fallback)
    $errorObject = $Record.Exception
    while ($null -ne $errorObject.InnerException) { $errorObject = $errorObject.InnerException }
    if ($errorObject.Message -in @('unsafe_path', 'cli_not_pe', 'cli_hash_mismatch', 'status_ownership_lost')) {
        return $errorObject.Message
    }
    return $Fallback
}

function Get-NoetomeAbsolutePath {
    param([string] $Value)
    if ([string]::IsNullOrEmpty($Value) -or $Value.IndexOfAny([char[]]"`0`r`n") -ge 0 -or
        -not [IO.Path]::IsPathFullyQualified($Value) -or $Value.StartsWith('\\')) { throw 'unsafe_path' }
    # No device/UNC paths or alternate data streams. The drive separator is allowed.
    if ($Value.Length -lt 3 -or $Value.Substring(2).Contains(':')) { throw 'unsafe_path' }
    $full = [IO.Path]::GetFullPath($Value)
    try { [NoetomeKeyLocal.SafeFiles]::CheckAncestors($full) }
    catch { throw 'unsafe_path' }
    return $full
}

function Write-NoetomeKeyStatus {
    param($Session, [string] $Phase, $PidValue = $null, $ExitCode = $null, $ErrorCode = $null)
    # Before a child exists, bounded contention checks can safely delay starting it.
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while (-not (TryWrite-NoetomeKeyStatus -Session $Session -Phase $Phase -PidValue $PidValue -ExitCode $ExitCode -ErrorCode $ErrorCode)) {
        if ($clock.ElapsedMilliseconds -ge 2000) { throw 'status_publish_failed' }
        Start-Sleep -Milliseconds 10
    }
}

function TryWrite-NoetomeKeyStatus {
    param($Session, [string] $Phase, $PidValue = $null, $ExitCode = $null, $ErrorCode = $null)
    try { return $Session.Status.TryWrite($Phase, $PidValue, $ExitCode, $ErrorCode) }
    catch { throw 'status_ownership_lost' }
}

function New-NoetomeKeySession {
    [CmdletBinding()]
    param([string] $CliPath, [string] $ExpectedCliSha256, [string] $ProjectPath,
        [string] $VersionId, [string] $ConfigPath, [string] $AuthorizationPath,
        [string] $OutputPath, [string] $PythonPath, [string] $StatusPath)
    if ($PSVersionTable.PSVersion.Major -lt 7 -or -not $IsWindows -or
        [Threading.Thread]::CurrentThread.GetApartmentState() -ne 'STA') { throw 'requires_pwsh7_sta' }
    Initialize-NoetomeKeyTypes
    $paths = @{}
    foreach ($name in @('CliPath','ProjectPath','ConfigPath','AuthorizationPath','OutputPath','PythonPath','StatusPath')) {
        $paths[$name] = Get-NoetomeAbsolutePath (Get-Variable -Name $name -ValueOnly)
    }
    $parent = [IO.Path]::GetDirectoryName($paths.StatusPath)
    if (-not [IO.Directory]::Exists($parent) -or
        $parent -ne [IO.Path]::GetDirectoryName($paths.OutputPath) -or
        $paths.OutputPath -eq $paths.StatusPath -or
        @([IO.Directory]::EnumerateFileSystemEntries($parent)).Count -ne 0) { throw 'result_root_not_fresh' }
    if (-not [IO.Directory]::Exists($paths.ProjectPath) -or
        -not [IO.File]::Exists($paths.ConfigPath) -or -not [IO.File]::Exists($paths.AuthorizationPath) -or
        -not [IO.File]::Exists($paths.PythonPath) -or [string]::IsNullOrWhiteSpace($VersionId) -or
        $VersionId.IndexOfAny([char[]]"`0`r`n") -ge 0 -or $ExpectedCliSha256 -notmatch '^[a-fA-F0-9]{64}$') {
        throw 'invalid_configuration'
    }
    $status = $null
    $cliLock = $null
    try {
        $status = [NoetomeKeyLocal.OwnedStatus]::new($paths.StatusPath)
        $cliLock = [NoetomeKeyLocal.SafeFiles]::OpenVerifiedCli($paths.CliPath, $ExpectedCliSha256)
        return [pscustomobject]@{
            Paths = $paths; VersionId = $VersionId; Status = $status; CliLock = $cliLock
            Gate = [NoetomeKeyLocal.OneShotGate]::new(); Process = $null; Drains = @()
            Phase = 'waiting_for_key'; ExitCode = $null; PidValue = $null; StatusFailed = $false
            RunningStatusPending = $false; StatusPublishClock = $null; ErrorCode = $null
        }
    } catch {
        $fixed = Get-NoetomeFixedException $_ 'preflight_failed'
        if ($null -ne $cliLock) { $cliLock.Dispose() }
        if ($null -ne $status) {
            try { $status.Write('failed', $null, $null, 'preflight_failed') } catch { }
            $status.Dispose()
        }
        throw $fixed
    }
}

function Start-NoetomeKeyRun {
    param($Session, [AllowEmptyString()] [AllowNull()] [string] $Credential)
    # Validate the original value; spaces are significant. Never trim, log or fingerprint it.
    try {
        $bytes = [Text.UTF8Encoding]::new($false, $true).GetByteCount($Credential)
        if ($bytes -lt 1 -or $bytes -gt 4096 -or $Credential.IndexOfAny([char[]]"`0`r`n") -ge 0) { throw 'invalid_key' }
    } catch { throw 'invalid_key' }
    if (-not $Session.Gate.TryEnter()) { throw 'already_attempted' }
    if ($Session.Phase -ne 'waiting_for_key') { throw 'already_attempted' }
    try {
        if ([IO.Path]::Exists($Session.Paths.OutputPath)) { throw 'output_exists' }
        Write-NoetomeKeyStatus -Session $Session -Phase 'running'
    } catch {
        $fixed = $_.Exception.Message
        $Session.Phase = 'failed'
        try { Write-NoetomeKeyStatus -Session $Session -Phase 'failed' -ErrorCode 'preflight_failed' }
        catch { $Session.StatusFailed = $true }
        if ($fixed -eq 'output_exists') { throw 'output_exists' }
        throw 'status_ownership_lost'
    }
    $Session.Phase = 'running'
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Session.Paths.CliPath
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.WorkingDirectory = [IO.Path]::GetDirectoryName($Session.Paths.CliPath)
    foreach ($argument in @('api', 'extract', $Session.Paths.ProjectPath, $Session.VersionId,
        $Session.Paths.ConfigPath, $Session.Paths.AuthorizationPath, $Session.Paths.OutputPath,
        '--python', $Session.Paths.PythonPath)) { $info.ArgumentList.Add($argument) }
    $info.Environment.Clear()
    $info.Environment['SystemRoot'] = [Environment]::GetEnvironmentVariable('SystemRoot', 'Process')
    $info.Environment['NOETOME_AI134_KEY'] = $Credential
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    try {
        if (-not $process.Start()) { throw 'child_start_failed' }
        $Session.Process = $process
        $Session.PidValue = $process.Id
        $Session.Drains = @([NoetomeKeyLocal.Pipes]::DiscardAsync($process.StandardOutput),
            [NoetomeKeyLocal.Pipes]::DiscardAsync($process.StandardError))
        try { $Session.RunningStatusPending = -not (TryWrite-NoetomeKeyStatus -Session $Session -Phase 'running' -PidValue $process.Id) }
        catch { $Session.StatusFailed = $true; $Session.ErrorCode = 'status_publish_failed' }
    } catch {
        if ($null -eq $Session.Process) {
            $process.Dispose()
            $Session.Phase = 'failed'
            Write-NoetomeKeyStatus -Session $Session -Phase 'failed' -ErrorCode 'child_start_failed'
            throw 'child_start_failed'
        }
        # A started child owns the one attempt. Keep polling it even if status writing fails.
        $Session.StatusFailed = $true
    } finally {
        [void] $info.Environment.Remove('NOETOME_AI134_KEY')
        $Credential = $null
        # .NET/WinForms strings can have copies: this is best effort, not full RAM erasure.
    }
}

function Update-NoetomeKeyRun {
    param($Session)
    if ($Session.Phase -ne 'running') { return $true }
    if ($null -eq $Session.Process) { return $false }
    if (-not $Session.Process.HasExited) {
        if ($Session.RunningStatusPending -and -not $Session.StatusFailed) {
            try { $Session.RunningStatusPending = -not (TryWrite-NoetomeKeyStatus -Session $Session -Phase 'running' -PidValue $Session.PidValue) }
            catch { $Session.StatusFailed = $true; $Session.ErrorCode = 'status_publish_failed' }
        }
        return $false
    }
    foreach ($drain in $Session.Drains) { if (-not $drain.IsCompleted) { return $false } }
    $Session.ExitCode = $Session.Process.ExitCode
    if ($null -eq $Session.StatusPublishClock) { $Session.StatusPublishClock = [Diagnostics.Stopwatch]::StartNew() }
    $code = $null
    if ($Session.ExitCode -ne 0) { $code = 'cli_failed' }
    foreach ($drain in $Session.Drains) { if ($drain.IsFaulted) { $code = 'child_pipe_failed' } }
    try {
        if (TryWrite-NoetomeKeyStatus -Session $Session -Phase 'finished' -PidValue $Session.PidValue -ExitCode $Session.ExitCode -ErrorCode $code) {
            $Session.Phase = 'finished'
            $Session.ErrorCode = $code
            $Session.StatusFailed = $false
            $Session.RunningStatusPending = $false
            return $true
        }
        # The timer keeps the same exited child and drained pipes; only local state is retried.
        if ($Session.StatusPublishClock.ElapsedMilliseconds -lt 2000) { return $false }
    } catch { }
    $Session.Phase = 'failed'
    $Session.StatusFailed = $true
    $Session.ErrorCode = 'status_publish_failed'
    try { [void] (TryWrite-NoetomeKeyStatus -Session $Session -Phase 'failed' -PidValue $Session.PidValue -ExitCode $Session.ExitCode -ErrorCode 'status_publish_failed') }
    catch { }
    return $true
}

function Stop-NoetomeKeyBeforeStart {
    param($Session)
    if ($Session.Phase -ne 'waiting_for_key' -or -not $Session.Gate.TryEnter()) { throw 'already_attempted' }
    Write-NoetomeKeyStatus -Session $Session -Phase 'cancelled'
    $Session.Phase = 'cancelled'
}

function Test-NoetomeKeyCanClose {
    param($Session)
    return $Session.Phase -ne 'running'
}

function Close-NoetomeKeySession {
    param($Session)
    if (-not (Test-NoetomeKeyCanClose -Session $Session)) { throw 'child_still_running' }
    if ($Session.StatusFailed -and $Session.Phase -eq 'failed') {
        # If a reader was released after the deadline, preserve the fixed failure when closing.
        try { [void] (TryWrite-NoetomeKeyStatus -Session $Session -Phase 'failed' -PidValue $Session.PidValue -ExitCode $Session.ExitCode -ErrorCode 'status_publish_failed') }
        catch { }
    }
    if ($null -ne $Session.Process) { $Session.Process.Dispose() }
    $Session.CliLock.Dispose()
    $Session.Status.Dispose()
}

function Initialize-NoetomeOwnFormWindow {
    if ('NoetomeKeyLocal.OwnFormWindow' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace NoetomeKeyLocal {
    public static class OwnFormWindow {
        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
        [DllImport("user32.dll")]
        private static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("user32.dll")]
        private static extern bool IsWindowVisible(IntPtr window);
        public static void ShowNormal(IntPtr window) {
            uint owner;
            if (window == IntPtr.Zero || GetWindowThreadProcessId(window, out owner) == 0
                || owner != (uint)Environment.ProcessId)
                throw new InvalidOperationException("window_owner_invalid");
            // A hidden PowerShell host supplies SW_HIDE to the first native show.
            // After WinForms' Shown lifecycle event, explicitly show this process's form.
            ShowWindow(window, 1); // SW_SHOWNORMAL; the return value means previous visibility.
            if (!IsWindowVisible(window)) throw new InvalidOperationException("window_show_failed");
        }
    }
}
'@ -ErrorAction Stop | Out-Null
}

function New-NoetomeKeyForm {
    param($Session)
    Initialize-NoetomeOwnFormWindow
    Add-Type -AssemblyName System.Windows.Forms -ErrorAction Stop | Out-Null
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop | Out-Null
    $form = [Windows.Forms.Form]::new()
    $form.Text = 'Noetome · 本次 API 密钥'
    $form.ClientSize = [Drawing.Size]::new(620, 380)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false
    $description = [Windows.Forms.Label]::new()
    $description.SetBounds(22, 18, 576, 158)
    $description.Text = @"
本次仅调用一次： https://ai.1343263.xyz/v1/responses
模型：deepseek-flash
资料：已登记并授权的 R7 短文；账户预算 10M tokens。
本次输出最多 1024 tokens，由 CLI 核对来源、配置和单次预算。

请在下方输入 API key。输入只用于本次子进程，不保存到文件或用户环境。
结束后如需另一次调用，必须重新准备独立输出并输入 key。
"@
    $label = [Windows.Forms.Label]::new()
    $label.Text = 'API key（隐藏输入）'
    $label.SetBounds(22, 182, 576, 22)
    $keyBox = [Windows.Forms.TextBox]::new()
    $keyBox.SetBounds(22, 210, 576, 30)
    $keyBox.UseSystemPasswordChar = $true
    # Reject an oversized full value in the start helper rather than truncating a paste.
    $keyBox.MaxLength = 0
    $run = [Windows.Forms.Button]::new()
    $run.Text = '仅运行这一次'
    $run.SetBounds(320, 256, 148, 34)
    $cancel = [Windows.Forms.Button]::new()
    $cancel.Text = '取消'
    $cancel.SetBounds(482, 256, 116, 34)
    $message = [Windows.Forms.Label]::new()
    $message.SetBounds(22, 306, 576, 58)
    $message.Text = '等待输入。本窗口不显示或保存子进程的正文输出。'
    $form.Controls.AddRange(@($description, $label, $keyBox, $run, $cancel, $message))
    $form.AcceptButton = $run
    $form.CancelButton = $cancel
    $form.Add_Shown({ [NoetomeKeyLocal.OwnFormWindow]::ShowNormal($form.Handle) }.GetNewClosure())
    $timer = [Windows.Forms.Timer]::new()
    $timer.Interval = 100
    $ui = [pscustomobject]@{ Form = $form; KeyBox = $keyBox; RunButton = $run; CancelButton = $cancel; Message = $message; Timer = $timer }
    $run.Add_Click({
        if ($Session.Gate.Attempted) { return }
        $run.Enabled = $false
        $cancel.Enabled = $false
        $keyBox.Enabled = $false
        try {
            # The textbox is cleared as soon as the value is handed to the start helper.
            $localKey = $keyBox.Text
            $keyBox.Clear()
            Start-NoetomeKeyRun -Session $Session -Credential $localKey
            $message.Text = '唯一一次调用正在运行。请等待 CLI 完成，窗口暂时不能关闭。'
            $timer.Start()
        } catch {
            if (-not $Session.Gate.Attempted) {
                $run.Enabled = $true; $cancel.Enabled = $true; $keyBox.Enabled = $true
                $message.Text = '输入须为 1–4096 UTF8 字节，且不包含换行或空字符。请重新输入。'
                $keyBox.Focus()
            } else {
                $message.Text = '本次启动失败；不会重试。可关闭窗口并核对固定状态文件。'
                $cancel.Enabled = $true; $cancel.Text = '关闭'
            }
        } finally { $localKey = $null; $keyBox.Clear() }
    }.GetNewClosure())
    $timer.Add_Tick({
        if (Update-NoetomeKeyRun -Session $Session) {
            $timer.Stop()
            $message.Text = "本次调用已结束，CLI 退出码：$($Session.ExitCode)。请核对 CLI 报告。"
            if ($Session.StatusFailed) { $message.Text += ' 状态保存失败（status_publish_failed），需人工核对，勿重复调用。' }
            $cancel.Text = '关闭'; $cancel.Enabled = $true
        }
    }.GetNewClosure())
    $cancel.Add_Click({ $keyBox.Clear(); $form.Close() }.GetNewClosure())
    $form.Add_FormClosing({
        param($sender, $eventArgs)
        if (-not (Test-NoetomeKeyCanClose -Session $Session)) {
            $eventArgs.Cancel = $true
            $message.Text = '唯一一次调用仍在运行。请等待结束后关闭，避免不明结果下重复调用。'
            return
        }
        $keyBox.Clear()
        if ($Session.Phase -eq 'waiting_for_key') {
            try { Stop-NoetomeKeyBeforeStart -Session $Session } catch { $Session.StatusFailed = $true }
        }
    }.GetNewClosure())
    return $ui
}

if ($MyInvocation.InvocationName -eq '.') { return }
$ErrorActionPreference = 'Stop'
$session = $null
$ui = $null
try {
    $session = New-NoetomeKeySession @PSBoundParameters
    $ui = New-NoetomeKeyForm -Session $session
    [void] $ui.Form.ShowDialog()
    if ($session.StatusFailed) { exit 1 }
    if ($session.Phase -eq 'cancelled') { exit 2 }
    if ($session.Phase -ne 'finished' -or $session.ExitCode -ne 0 -or $null -ne $session.ErrorCode) { exit 1 }
    exit 0
} catch {
    # Never print exception objects: child errors and future input plumbing can be sensitive.
    if ($null -ne $session) {
        if ($session.Phase -eq 'running') {
            # If a form fails after start, still observe the sole child to completion.
            while (-not (Update-NoetomeKeyRun -Session $session)) { Start-Sleep -Milliseconds 100 }
        } elseif ($session.Phase -eq 'waiting_for_key') {
            $session.Phase = 'failed'
            try { Write-NoetomeKeyStatus -Session $session -Phase 'failed' -ErrorCode 'preflight_failed' }
            catch { $session.StatusFailed = $true }
        }
    }
    Write-Output 'key_dialog_failed'
    exit 1
} finally {
    if ($null -ne $ui) { $ui.KeyBox.Clear(); $ui.Timer.Dispose(); $ui.Form.Dispose() }
    if ($null -ne $session -and (Test-NoetomeKeyCanClose -Session $session)) {
        Close-NoetomeKeySession -Session $session
    }
}
