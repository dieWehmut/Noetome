[CmdletBinding()]
param(
    [Alias('SourcePath')]
    [string]$InstallerPath,
    [string]$OutputDir,
    [switch]$PassThru
)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Preparing the Windows installer requires Windows.' }
$taskRepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $InstallerPath) {
    $taskInstallers = @(Get-ChildItem -LiteralPath (Join-Path $taskRepoRoot 'target\release\bundle\nsis') -Filter '*-setup.exe' -File)
    if ($taskInstallers.Count -ne 1) { throw 'Expected exactly one built NSIS installer. Pass -InstallerPath explicitly.' }
    $InstallerPath = $taskInstallers[0].FullName
}
$taskSource = Get-Item -LiteralPath $InstallerPath -Force
if ($taskSource.PSIsContainer) { throw 'InstallerPath must identify a file.' }
$InstallerPath = $taskSource.FullName
if (-not $OutputDir) {
    if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is unavailable. Pass -OutputDir explicitly.' }
    $OutputDir = Join-Path $env:LOCALAPPDATA 'NoetomeBuilds\windows'
}
$OutputDir = [IO.Path]::GetFullPath($OutputDir)

# Query mandatory labels only. No ACL, token or security policy is changed.
# An executable's Low label can lower a new process despite a Medium parent.
if (-not ('NoetomeDesktopInstaller.Integrity' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace NoetomeDesktopInstaller {
    public sealed class IntegrityLabel {
        public string SDDL;
        public uint Rid;
    }
    public static class Integrity {
        [StructLayout(LayoutKind.Sequential)]
        private struct AclSize { public uint Count, Used, Free; }
        [DllImport("advapi32.dll", CharSet = CharSet.Unicode)]
        private static extern uint GetNamedSecurityInfoW(string path, int type, uint info,
            IntPtr owner, IntPtr group, IntPtr dacl, out IntPtr sacl, out IntPtr descriptor);
        [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool ConvertSecurityDescriptorToStringSecurityDescriptorW(
            IntPtr descriptor, uint revision, uint info, out IntPtr text, out uint length);
        [DllImport("advapi32.dll", SetLastError = true)]
        private static extern bool GetAclInformation(IntPtr acl, out AclSize size, uint length, int info);
        [DllImport("advapi32.dll", SetLastError = true)]
        private static extern bool GetAce(IntPtr acl, uint index, out IntPtr ace);
        [DllImport("advapi32.dll")]
        private static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
        [DllImport("advapi32.dll")]
        private static extern IntPtr GetSidSubAuthority(IntPtr sid, uint index);
        [DllImport("kernel32.dll")]
        private static extern IntPtr LocalFree(IntPtr memory);

        public static IntegrityLabel Read(string path) {
            const uint LabelInformation = 0x10;
            IntPtr sacl, descriptor;
            uint error = GetNamedSecurityInfoW(path, 1, LabelInformation,
                IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, out sacl, out descriptor);
            if (error != 0) throw new Win32Exception((int)error, "Cannot read integrity label: " + path);
            IntPtr text = IntPtr.Zero;
            try {
                uint length;
                if (!ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, 1,
                    LabelInformation, out text, out length)) throw new Win32Exception(Marshal.GetLastWin32Error());
                var result = new IntegrityLabel { SDDL = Marshal.PtrToStringUni(text), Rid = 8192 };
                bool foundLabel = false;
                if (sacl != IntPtr.Zero) {
                    AclSize size;
                    if (!GetAclInformation(sacl, out size, (uint)Marshal.SizeOf(typeof(AclSize)), 2))
                        throw new Win32Exception(Marshal.GetLastWin32Error());
                    for (uint index = 0; index < size.Count; index++) {
                        IntPtr ace;
                        if (!GetAce(sacl, index, out ace)) throw new Win32Exception(Marshal.GetLastWin32Error());
                        if (Marshal.ReadByte(ace) != 0x11) continue; // SYSTEM_MANDATORY_LABEL_ACE
                        IntPtr sid = IntPtr.Add(ace, 8);
                        byte count = Marshal.ReadByte(GetSidSubAuthorityCount(sid));
                        if (count == 0) throw new InvalidOperationException("Invalid mandatory integrity SID.");
                        uint rid = unchecked((uint)Marshal.ReadInt32(GetSidSubAuthority(sid, (uint)count - 1)));
                        if (!foundLabel || rid < result.Rid) result.Rid = rid;
                        foundLabel = true;
                    }
                }
                return result;
            } finally {
                if (text != IntPtr.Zero) LocalFree(text);
                LocalFree(descriptor);
            }
        }
    }
}
'@
}

function Assert-NormalIntegrity([string]$Path) {
    $taskLabel = [NoetomeDesktopInstaller.Integrity]::Read($Path)
    if ($taskLabel.Rid -lt 8192) {
        throw "Installer output has Low or Untrusted integrity: $Path ($($taskLabel.SDDL)). Choose a normal user directory with -OutputDir. Permissions were not changed."
    }
    return $taskLabel
}

if (Test-Path -LiteralPath $OutputDir -PathType Leaf) { throw 'OutputDir is an existing file. Choose another directory.' }
$null = [IO.Directory]::CreateDirectory($OutputDir)
$null = Assert-NormalIntegrity $OutputDir
$taskSourceHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$taskDestination = Join-Path $OutputDir $taskSource.Name
if (Test-Path -LiteralPath $taskDestination) {
    $taskExisting = Get-Item -LiteralPath $taskDestination -Force
    if ($taskExisting.PSIsContainer -or (Get-FileHash -LiteralPath $taskDestination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskSourceHash) {
        # Keep the unknown file/directory. A content-addressed sibling is a new output.
        $taskVersionDir = Join-Path $OutputDir $taskSourceHash.Substring(0, 12)
        if (Test-Path -LiteralPath $taskVersionDir -PathType Leaf) { throw 'The content-addressed output directory is occupied by a file. Choose another -OutputDir.' }
        $null = [IO.Directory]::CreateDirectory($taskVersionDir)
        $null = Assert-NormalIntegrity $taskVersionDir
        $taskDestination = Join-Path $taskVersionDir $taskSource.Name
    }
}

$taskReused = Test-Path -LiteralPath $taskDestination
$taskCreated = $false
try {
    if ($taskReused) {
        if ((Get-Item -LiteralPath $taskDestination -Force).PSIsContainer -or
            (Get-FileHash -LiteralPath $taskDestination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskSourceHash) {
            throw 'The content-addressed output path contains different existing content. Choose another -OutputDir.'
        }
    } else {
        # Copy content through CreateNew rather than copying source security metadata.
        $taskInput = $null
        $taskOutput = $null
        try {
            $taskInput = [IO.File]::Open($InstallerPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
            $taskOutput = [IO.File]::Open($taskDestination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
            $taskCreated = $true
            $taskInput.CopyTo($taskOutput)
            $taskOutput.Flush($true)
        } finally {
            if ($taskOutput) { $taskOutput.Dispose() }
            if ($taskInput) { $taskInput.Dispose() }
        }
    }
    $taskDestinationHash = (Get-FileHash -LiteralPath $taskDestination -Algorithm SHA256).Hash.ToLowerInvariant()
    $taskCurrentSourceHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($taskDestinationHash -ne $taskSourceHash -or $taskCurrentSourceHash -ne $taskSourceHash) {
        throw 'Installer content changed while preparing the distribution copy. Rebuild or retry with a stable source.'
    }
    $taskFinalLabel = Assert-NormalIntegrity $taskDestination
} catch {
    if ($taskCreated) {
        # Only our exclusively created file, resolved within the selected output root.
        $taskAbsoluteDestination = [IO.Path]::GetFullPath($taskDestination)
        $taskOutputPrefix = $OutputDir.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
        if ($taskAbsoluteDestination.StartsWith($taskOutputPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $taskAbsoluteDestination -ErrorAction SilentlyContinue
        }
    }
    throw
}

$taskResult = [pscustomobject]@{
    Path = $taskDestination
    SHA256 = $taskDestinationHash
    IntegrityLabel = $taskFinalLabel.SDDL
    IntegrityRid = $taskFinalLabel.Rid
    SourcePath = $InstallerPath
    SourceSHA256 = $taskSourceHash
    Reused = [bool]$taskReused
}
if ($PassThru) { $taskResult } else {
    Write-Output "Installer: $($taskResult.Path)"
    Write-Output "SHA256: $($taskResult.SHA256)"
    Write-Output 'Integrity: Medium or higher (no permissions changed)'
}
