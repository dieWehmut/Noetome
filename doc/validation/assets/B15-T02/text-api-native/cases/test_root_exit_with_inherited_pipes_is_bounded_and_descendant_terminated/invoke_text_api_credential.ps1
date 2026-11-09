# One selected text API CLI invocation; CurrentUser DPAPI and the existing cumulative quota stay external.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [Parameter(Mandatory=$true)][string]$Project,
    [Parameter(Mandatory=$true)][string]$ImportId,
    [Parameter(Mandatory=$true)][string]$Config,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][string]$CredentialFile,
    [Parameter(Mandatory=$true)][string]$UsageLedgerFile,
    [string]$CancelFile
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'text_api_support.ps1')
$secure=$null; $pointer=[IntPtr]::Zero; $plaintext=$null; $process=$null; $start=$null
$ledgerStream=$null; $ledger=$null; $record=$null; $diagnostic='operation-failed'; $exitCode=2
$started=$false
$reservationApplied=$false; $stdout=$null; $stderr=$null
$job=$null; $contained=$false; $termination=$null; $stdoutTask=$null; $stderrTask=$null
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { $diagnostic='windows-required'; throw 'stop' }
    $diagnostic='configuration-invalid'
    $configStream=Open-Selected $Config
    $configText=Read-Selected $configStream 65536
    [Noetome.TextInvokeNative]::UniqueJsonKeys($configText)
    $configuration=Parse-JsonText $configText
    $environmentName=$configuration.credential_env
    $live=$configuration.transport_mode -ceq 'live'
    if ($live) {
        if ($environmentName -cne 'NOETOME_TANGMAO_API_KEY' -or $configuration.endpoint -cne 'https://ai.1343263.xyz/v1/chat/completions' -or $configuration.model -cne 'deepseek-flash') { throw 'stop' }
    } elseif ($configuration.transport_mode -cne 'local_http_test' -or $environmentName -cne 'NOETOME_TEST_TEXT_CREDENTIAL') { throw 'stop' }
    if ($configuration.provider -cne 'openai_chat_completions' -or $configuration.required_input -cne 'text' -or -not (Integer $configuration.budget.max_requests 1) -or -not (Integer $configuration.budget.max_total_tokens 10000000) -or $null -ne $configuration.budget.max_cost_usd -or -not (Integer $configuration.limits.timeout_ms 60000) -or $configuration.limits.timeout_ms -eq 0) { throw 'stop' }
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
        [Noetome.TextInvokeNative]::UniqueJsonKeys($originalLedger)
        $ledger=Parse-JsonText $originalLedger; Check-Ledger $ledger
        [long]$reservation=$configuration.budget.max_total_tokens
        if ($reservation -lt 1 -or $reservation -gt $ledger.remaining_tokens) { $diagnostic='budget-stop'; throw 'stop' }
        $id=[Guid]::NewGuid().ToString()
        $record=[pscustomobject]@{invocation_id=$id;task_id='B15-T02';reserved_at=[DateTime]::UtcNow.ToString('o');status='reserved';endpoint=$configuration.endpoint;configured_model=$configuration.model;request_strategy='tangmao-deepseek-chat-positive-revision-v6';reservation_tokens=$reservation;reservation_policy='bounded_request_total_cap';confirmed_total_tokens=$null;network_attempts=$null;config_reference=$Config;config_sha256=(Digest-Stream $configStream);binary_reference=$Binary;binary_sha256=(Digest-Stream $binaryGuard);project_reference=$Project;source_import_id=$ImportId;output_root=$OutputRoot;actual_model=$null;response_sha256=$null;price_usd=$null;reason='One explicit text CLI request; unknown usage retains reservation.'}
        $backupPath=$UsageLedgerFile+'.text-before-'+$id+'.json'
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
    $start.Arguments=((@('extract','api',$Project,$ImportId,$Config,$OutputRoot) + $(if ($CancelFile) { @('--cancel-file',$CancelFile) } else { @() })) | ForEach-Object { Native-Argument $_ }) -join ' '
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $start.EnvironmentVariables[$environmentName]=$plaintext
    $process=New-Object Diagnostics.Process; $process.StartInfo=$start
    $diagnostic='child-containment-failed'; $job=New-Object Noetome.TextInvokeJob
    $diagnostic='launch-failed'
    if (-not $process.Start()) { throw 'stop' }
    $started=$true
    $diagnostic='child-containment-failed'; $job.Assign($process); $contained=$true
    if ($reservationApplied) { Save-Phase 'launch' }
    $stdoutTask=[Noetome.TextInvokeNative]::ReadBounded($process.StandardOutput.BaseStream,16777216)
    $stderrTask=[Noetome.TextInvokeNative]::ReadBounded($process.StandardError.BaseStream,1048576)
    $childWatch=[Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        if ($stdoutTask.IsFaulted -or $stderrTask.IsFaulted) { $diagnostic='child-output-limit'; throw 'stop' }
        if ($process.WaitForExit(50) -and $stdoutTask.IsCompleted -and $stderrTask.IsCompleted) { break }
        if ($CancelFile -and [IO.File]::Exists($CancelFile) -and $childWatch.ElapsedMilliseconds -gt 5000) { $diagnostic='child-cancelled'; throw 'stop' }
        if ($childWatch.ElapsedMilliseconds -gt ($configuration.limits.timeout_ms+10000)) { $diagnostic='child-timeout'; throw 'stop' }
    }
    $stdout=$stdoutTask.GetAwaiter().GetResult(); $stderr=$stderrTask.GetAwaiter().GetResult(); $exitCode=$process.ExitCode
    if ($null -ne $record) {
        $record.status='usage_unknown'
        try {
            [Noetome.TextInvokeNative]::UniqueJsonKeys($stdout)
            $report=Parse-JsonText $stdout
            $api=$report
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
                        $record.confirmed_total_tokens=$usage.total_tokens; $record.status='settled_valid_usage_text'; $ledger.accounted_tokens += $usage.total_tokens
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
    Emit-Utf8 (Hide-Credential $stdout $plaintext); Emit-Utf8 (Hide-Credential $stderr $plaintext) -ErrorStream
} catch {
    if ($started) { $termination=Stop-SelectedChild }
    if ($reservationApplied) {
        try { if ($null -eq $record.confirmed_total_tokens) { $record.status='usage_unknown' }; Write-Ledger $ledgerStream $ledger; Save-Phase 'failed' } catch { }
    }
    [Console]::Error.WriteLine('text-credential-invoke: '+$diagnostic); $exitCode=2
    if ($started -and (-not $termination.root_exit_confirmed -or ($contained -and -not $termination.job_empty_confirmed))) { [Console]::Error.WriteLine('text-credential-invoke: child-termination-unconfirmed') }
} finally {
    if ($started -and $null -eq $termination) { $termination=Stop-SelectedChild }
    if ($null -ne $job) { $job.Dispose() }
    if ($null -ne $start) { $start.EnvironmentVariables.Remove($environmentName) }
    $plaintext=$null; $stdout=$null; $stderr=$null
    if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
    if ($null -ne $secure) { $secure.Dispose() }
    if ($null -ne $process) { $process.Dispose() }
    for ($i=$guards.Count-1; $i -ge 0; $i--) { $guards[$i].Dispose() }
}
exit $exitCode
