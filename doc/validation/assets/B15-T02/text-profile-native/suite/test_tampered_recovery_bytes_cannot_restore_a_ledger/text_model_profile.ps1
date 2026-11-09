# Narrow CurrentUser text credential status/storage and explicit missing-ledger restoration.
[CmdletBinding()]
param(
    [ValidateSet('Status','Store','RestoreLedger')][string]$Mode='Status',
    [Parameter(Mandatory=$true)][string]$CredentialFile,
    [Parameter(Mandatory=$true)][string]$UsageLedgerFile,
    [string]$CredentialSetter,
    [string]$RecoveryLedger,
    [string]$RecoveryProof,
    [switch]$Confirmed
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    [Console]::Error.WriteLine('TEXT_PROFILE_WINDOWS_REQUIRED'); exit 2
}
. (Join-Path $PSScriptRoot 'text_api_support.ps1')

function Ensure-Directory([string]$Path) {
    if ([IO.Directory]::Exists($Path)) {
        $guards.Add([Noetome.TextInvokeNative]::DirectoryGuard($Path)); return
    }
    $parent=[IO.Path]::GetDirectoryName($Path)
    if (-not $parent -or $parent -eq $Path) { throw 'profile-path-invalid' }
    Ensure-Directory $parent
    $null=[IO.Directory]::CreateDirectory($Path)
    $guards.Add([Noetome.TextInvokeNative]::DirectoryGuard($Path))
}
function Entry-State([string]$Path) {
    try {
        $attributes=[IO.File]::GetAttributes($Path)
        if (($attributes -band [IO.FileAttributes]::Directory) -ne 0 -or ($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return 'invalid' }
        return 'ready'
    } catch [IO.FileNotFoundException] { return 'missing' }
      catch [IO.DirectoryNotFoundException] { return 'missing' }
      catch { return 'invalid' }
}
function Status {
    $credentialState='missing'; $ledgerState='missing'; $ledger=$null; $secure=$null; $pointer=[IntPtr]::Zero
    $credentialState=Entry-State $CredentialFile
    try {
        if ($credentialState -eq 'ready') {
            $credentialState='invalid'
            $stream=Open-Selected $CredentialFile -External
            $secure=(Read-Selected $stream 65536) | ConvertTo-SecureString
            $pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
            if ($secure.Length -lt 1 -or $secure.Length -gt 8192) { throw 'invalid-credential' }
            for ($index=0; $index -lt $secure.Length; $index++) {
                $character=[Runtime.InteropServices.Marshal]::ReadInt16($pointer,$index*2)
                if ($character -lt 33 -or $character -gt 126) { throw 'invalid-credential' }
            }
            $credentialState='ready'
        }
    } catch { $credentialState='invalid' }
    finally {
        if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
        if ($null -ne $secure) { $secure.Dispose() }
    }
    $ledgerState=Entry-State $UsageLedgerFile
    try {
        if ($ledgerState -eq 'ready') {
            $ledgerState='invalid'
            $stream=Open-Selected $UsageLedgerFile -External
            $text=Read-Selected $stream 1048576
            [Noetome.TextInvokeNative]::UniqueJsonKeys($text)
            $ledger=Parse-JsonText $text; Check-Ledger $ledger
            $ledgerState='ready'
        }
    } catch { $ledger=$null; $ledgerState='invalid' }
    $ready=($credentialState -eq 'ready' -and $ledgerState -eq 'ready' -and $ledger.remaining_tokens -gt 0)
    $message=if ($ready) { 'One explicit bounded text request is ready; existing cumulative quota will be reserved.' } else { 'Credential or cumulative quota is unavailable; no model request was made.' }
    $payload=[ordered]@{ready=$ready;endpoint='https://ai.1343263.xyz/v1/chat/completions';model='deepseek-flash';credential_state=$credentialState;ledger_state=$ledgerState;accounted_tokens=$null;pending_reserved_tokens=$null;remaining_tokens=$null;model_attempts=$null;message=$message}
    if ($null -ne $ledger) {
        foreach ($name in @('accounted_tokens','pending_reserved_tokens','remaining_tokens','model_attempts')) { $payload[$name]=$ledger.$name }
    }
    return [pscustomobject]$payload
}
try {
    if ($Mode -eq 'RestoreLedger') {
        if (-not $Confirmed) { throw 'TEXT_LEDGER_CONFIRMATION_REQUIRED' }
        if ((Entry-State $UsageLedgerFile) -ne 'missing') { throw 'TEXT_LEDGER_EXISTS' }
        $source=Open-Selected $RecoveryLedger
        $bytes=Bytes-Stream $source
        if ($bytes.Length -ne 23989 -or (Digest-Bytes $bytes) -cne 'ea6b32432cb8bc1c325be48be01ce42f1dde933f2fc35f8c739c9d3fc07d8ffd') { throw 'TEXT_LEDGER_RECOVERY_SOURCE_INVALID' }
        $text=(New-Object Text.UTF8Encoding($false,$true)).GetString($bytes)
        [Noetome.TextInvokeNative]::UniqueJsonKeys($text)
        $ledger=Parse-JsonText $text; Check-Ledger $ledger
        $proofStream=Open-Selected $RecoveryProof
        $proofText=Read-Selected $proofStream 65536
        [Noetome.TextInvokeNative]::UniqueJsonKeys($proofText)
        $proof=Parse-JsonText $proofText
        if ($ledger.accounted_tokens -ne 33185 -or $ledger.pending_reserved_tokens -ne 0 -or $ledger.remaining_tokens -ne 9966815 -or $ledger.model_attempts -ne 10 -or @($ledger.records).Count -ne 10 -or $proof.sha256 -cne (Digest-Bytes $bytes) -or $proof.byte_length -ne $bytes.Length -or $proof.accounted_tokens -ne 33185 -or $proof.record_count -ne 10 -or $proof.model_attempts -ne 10 -or $proof.status -cne 'passed') { throw 'TEXT_LEDGER_RECOVERY_SOURCE_INVALID' }
        Ensure-Directory ([IO.Path]::GetDirectoryName($UsageLedgerFile))
        $destination=Selected-Path $UsageLedgerFile -External
        $null=Save-New $destination $bytes
    } elseif ($Mode -eq 'Store') {
        if ((Entry-State $CredentialFile) -ne 'missing') { throw 'TEXT_CREDENTIAL_EXISTS' }
        Ensure-Directory ([IO.Path]::GetDirectoryName($CredentialFile))
        $null=Selected-Path $CredentialFile -External
        $setter=Open-Selected $CredentialSetter
        $powershell=[IO.Path]::Combine($env:SystemRoot,'System32','WindowsPowerShell','v1.0','powershell.exe')
        & $powershell -NoProfile -STA -File $CredentialSetter -CredentialFile $CredentialFile -ServiceUrl 'https://ai.1343263.xyz/' -Model 'deepseek-flash' -Dialog *> $null
        if ($LASTEXITCODE -ne 0) { throw 'TEXT_CREDENTIAL_STORE_FAILED' }
    }
    $value=Status
    Emit-Utf8 (($value | ConvertTo-Json -Depth 10)+"`n")
} catch {
    # Stable diagnostics only; invalid ciphertext and existing history are retained.
    [Console]::Error.WriteLine('text-model-profile: operation-refused'); exit 2
} finally {
    for ($i=$guards.Count-1; $i -ge 0; $i--) { $guards[$i].Dispose() }
}
