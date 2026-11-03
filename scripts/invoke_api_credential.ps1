# Read one selected DPAPI reference and launch one existing bounded CLI command.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(Mandatory = $true)][string]$Project,
    [Parameter(Mandatory = $true)][string]$ImportId,
    [Parameter(Mandatory = $true)][string]$Config,
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [Parameter(Mandatory = $true)][string]$CredentialFile,
    [string]$CancelFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-CredentialPath {
    param([string]$Path)
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path.Substring(2).Contains(':')) { throw 'path-invalid' }
    foreach ($part in ($Path.Substring(3) -split '[\\/]')) {
        if ($part -eq '.' -or $part -eq '..' -or $part -match '[. ]$' -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'path-invalid' }
    }
    $full = [IO.Path]::GetFullPath($Path)
    $repository = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\', '/')
    if ($full.Equals($repository, [StringComparison]::OrdinalIgnoreCase) -or
        $full.StartsWith($repository + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'path-in-repository' }
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            if (([IO.File]::GetAttributes($cursor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'path-link' }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    if (-not [IO.File]::Exists($full)) { throw 'credential-not-file' }
    return $full
}

function ConvertTo-NativeArgument {
    param([string]$Value)
    # Windows CRT quoting: double backslashes only before quotes and closing quote.
    return '"' + [regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
}

function Open-CredentialReadGuard {
    param([string]$Path)
    if (-not ('Noetome.CredentialReadGuard' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
namespace Noetome {
    public static class CredentialReadGuard {
        [StructLayout(LayoutKind.Sequential)]
        private struct FileInformation {
            public uint Attributes;
            public uint CreationLow, CreationHigh, AccessLow, AccessHigh;
            public uint WriteLow, WriteHigh, VolumeSerial, SizeHigh, SizeLow;
            public uint Links, IndexHigh, IndexLow;
        }
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetFileInformationByHandle(
            SafeFileHandle file, out FileInformation information);
        public static bool IsSingleOrdinaryFile(SafeFileHandle file) {
            FileInformation information;
            if (!GetFileInformationByHandle(file, out information))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            return information.Links == 1 &&
                (information.Attributes & (0x10u | 0x400u)) == 0;
        }
    }
}
'@
    }
    # Inspect only this selected handle. Deny writers/deletion during inspection/read.
    $guard = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if (-not [Noetome.CredentialReadGuard]::IsSingleOrdinaryFile($guard.SafeFileHandle)) { throw 'path-link' }
        return $guard
    }
    catch {
        $guard.Dispose()
        throw
    }
}

$secure = $null
$pointer = [IntPtr]::Zero
$plaintext = $null
$process = $null
$start = $null
$environmentName = $null
$credentialGuard = $null
$credentialReader = $null
$diagnostic = 'operation-failed'
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { $diagnostic = 'windows-required'; throw 'stop' }
    $diagnostic = 'path-rejected'
    $credentialPath = Assert-CredentialPath $CredentialFile
    $credentialGuard = Open-CredentialReadGuard $credentialPath
    $diagnostic = 'configuration-invalid'
    if ((Get-Item -LiteralPath $Config).Length -gt 65536) { throw 'stop' }
    $configuration = [IO.File]::ReadAllText($Config) | ConvertFrom-Json
    $environmentName = $configuration.credential_env
    if ($environmentName -isnot [string] -or $environmentName -cnotmatch '^[A-Z][A-Z0-9_]{1,63}$') { throw 'stop' }
    $diagnostic = 'credential-invalid'
    if ($credentialGuard.Length -gt 65536) { throw 'stop' }
    $credentialReader = New-Object IO.StreamReader($credentialGuard, [Text.Encoding]::UTF8, $true, 1024, $true)
    $secure = $credentialReader.ReadToEnd() | ConvertTo-SecureString
    if ($secure.Length -lt 1 -or $secure.Length -gt 8192) { throw 'stop' }
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    for ($index = 0; $index -lt $secure.Length; $index++) {
        $character = [Runtime.InteropServices.Marshal]::ReadInt16($pointer, $index * 2)
        if ($character -lt 33 -or $character -gt 126) { throw 'stop' }
    }
    $plaintext = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    $diagnostic = 'launch-failed'
    $arguments = @('extract', 'api', $Project, $ImportId, $Config, $OutputRoot)
    if ($CancelFile) { $arguments += @('--cancel-file', $CancelFile) }
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Binary
    $start.Arguments = ($arguments | ForEach-Object { ConvertTo-NativeArgument $_ }) -join ' '
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = New-Object Text.UTF8Encoding($false)
    $start.StandardErrorEncoding = New-Object Text.UTF8Encoding($false)
    # Only the child environment changes. The parent stays untouched, including on failure.
    $start.EnvironmentVariables[$environmentName] = $plaintext
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    if (-not $process.Start()) { throw 'stop' }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    [Console]::Out.Write($stdout.Replace($plaintext, '[redacted]'))
    [Console]::Error.Write($stderr.Replace($plaintext, '[redacted]'))
    $exitCode = $process.ExitCode
    exit $exitCode
}
catch {
    [Console]::Error.WriteLine('credential-invoke: ' + $diagnostic)
    exit 2
}
finally {
    if ($null -ne $start -and $null -ne $environmentName) { $start.EnvironmentVariables.Remove($environmentName) }
    $plaintext = $null
    $stdout = $null
    $stderr = $null
    if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
    if ($null -ne $secure) { $secure.Dispose() }
    if ($null -ne $process) { $process.Dispose() }
    if ($null -ne $credentialReader) { $credentialReader.Dispose() }
    if ($null -ne $credentialGuard) { $credentialGuard.Dispose() }
}
