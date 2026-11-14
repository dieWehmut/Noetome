param(
    [Parameter(Mandatory=$true)][ValidateSet('mineru','paddleocr')][string]$EngineKind,
    [Parameter(Mandatory=$true)][string]$RuntimeRoot,
    [Parameter(Mandatory=$true)][string]$InputPages,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [ValidateRange(60,1800)][int]$PageTimeoutSeconds=600,
    [string]$ComparisonScript,
    [ValidateSet('vl16','ppstructurev3')][string]$PaddlePipeline='vl16'
)
$ErrorActionPreference='Stop'
if([string]::IsNullOrEmpty($ComparisonScript)){
    $ComparisonScript=Join-Path $PSScriptRoot 'compare.py'
}
$taskPython=Join-Path $RuntimeRoot 'environment\Scripts\python.exe'
$taskReportPath=Join-Path $OutputRoot 'report.json'
$taskProgressPath=Join-Path $OutputRoot 'in-progress.json'
$taskReceipt=Get-Content -LiteralPath (Join-Path $InputPages 'render-receipt.json') -Raw | ConvertFrom-Json
$taskExpected=$taskReceipt.items.Count
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

function Write-TaskJson($path,$value){
    $temporary=$path+'.external.tmp'
    [IO.File]::WriteAllText($temporary,($value | ConvertTo-Json -Depth 40)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $path -Force
}
function Quote-TaskArgument([string]$value){
    # Start-Process passes a native command line, never a shell expression.
    $escaped=[Regex]::Replace($value,'(\\*)"','$1$1\"')
    $escaped=[Regex]::Replace($escaped,'(\\+)$','$1$1')
    return '"'+$escaped+'"'
}
function Stop-TaskInterpreter($taskProcess,$workerPidValue){
    $child=Get-CimInstance Win32_Process -Filter ('ProcessId='+$workerPidValue)
    if($child){
        if(-not $child.CommandLine.Contains($ComparisonScript) -or -not $child.CommandLine.Contains($OutputRoot)){
            throw 'Reported interpreter does not belong to this comparison'
        }
        if($workerPidValue -ne $taskProcess.Id -and $child.ParentProcessId -ne $taskProcess.Id){
            throw 'Reported interpreter is outside this venv launcher'
        }
        Stop-Process -Id $workerPidValue -Force -ErrorAction SilentlyContinue
    }
    Stop-Process -Id $taskProcess.Id -Force -ErrorAction SilentlyContinue
    $taskProcess.WaitForExit()
}

for($attempt=1;$attempt -le $taskExpected+2;$attempt++){
    $report=$null
    if(Test-Path -LiteralPath $taskReportPath){$report=Get-Content -LiteralPath $taskReportPath -Raw | ConvertFrom-Json}
    $beforeCount=if($report){$report.items.Count}else{0}
    if($beforeCount -ge $taskExpected){break}
    $stamp=[Guid]::NewGuid().ToString('N')
    $taskStdout=Join-Path $OutputRoot ('host-'+$stamp+'.stdout.log')
    $taskStderr=Join-Path $OutputRoot ('host-'+$stamp+'.stderr.log')
    $arguments=@('-I','-B','-X','utf8',$ComparisonScript,'--engine',$EngineKind,'--runtime',$RuntimeRoot,'--input',$InputPages,'--output',$OutputRoot,'--resume','--page-timeout',[string]$PageTimeoutSeconds)
    if($PaddlePipeline -ne 'vl16'){$arguments+=@('--paddle-pipeline',$PaddlePipeline)}
    $argumentLine=($arguments | ForEach-Object {Quote-TaskArgument $_}) -join ' '
    $taskProcess=Start-Process -FilePath $taskPython -ArgumentList $argumentLine -RedirectStandardOutput $taskStdout -RedirectStandardError $taskStderr -PassThru
    # Keep the native handle before exit; PowerShell 5 otherwise can expose a
    # null ExitCode after a short-lived redirected process has disappeared.
    $taskNativeHandle=$taskProcess.Handle
    $hostWatch=[Diagnostics.Stopwatch]::StartNew()
    $externalTimeout=$false
    while(-not $taskProcess.WaitForExit(1000)){
        if(-not (Test-Path -LiteralPath $taskProgressPath)){continue}
        $progress=Get-Content -LiteralPath $taskProgressPath -Raw | ConvertFrom-Json
        $elapsed=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()/1000.0-$progress.started_unix_seconds
        if($elapsed -le $PageTimeoutSeconds){continue}
        $report=Get-Content -LiteralPath $taskReportPath -Raw | ConvertFrom-Json
        if(@($report.items | Where-Object {$_.case_id -eq $progress.case_id}).Count -gt 0){continue}
        $deadlineObservedElapsed=$elapsed
        Stop-TaskInterpreter $taskProcess $progress.pid
        $elapsed=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()/1000.0-$progress.started_unix_seconds
        # Stop first, then re-read: a result confirmed at the boundary wins.
        $report=Get-Content -LiteralPath $taskReportPath -Raw | ConvertFrom-Json
        if(@($report.items | Where-Object {$_.case_id -eq $progress.case_id}).Count -eq 0){
            $original=@($taskReceipt.items | Where-Object {$_.case_id -eq $progress.case_id})
            if($original.Count -ne 1){throw 'Timed out page does not belong to frozen inputs'}
            $failed=$original[0] | ConvertTo-Json -Depth 20 | ConvertFrom-Json
            $failed | Add-Member status 'failed'
            $failed | Add-Member text ''
            $failed | Add-Member regions @()
            $failed | Add-Member elapsed_seconds $elapsed
            $failed | Add-Member issues @(@{code='OCR_COMPARE_HOST_TIMEOUT';message='External owned-process deadline stopped unconfirmed native inference.'})
            $resultPath=Join-Path (Join-Path $OutputRoot $progress.case_id) 'normalized.json'
            Write-TaskJson $resultPath $failed
            $item=[pscustomobject]@{case_id=$progress.case_id;status='failed';elapsed_seconds=$elapsed;characters=0;regions=0;issue_codes=@('OCR_COMPARE_HOST_TIMEOUT');result_sha256=(Get-FileHash -LiteralPath $resultPath -Algorithm SHA256).Hash.ToLowerInvariant()}
            $report.items=@($report.items)+$item
            $construction=@($report.construction_attempts_seconds | Measure-Object -Sum).Sum
            $report.total_seconds=$construction+@($report.items | Measure-Object -Property elapsed_seconds -Sum).Sum
            Write-TaskJson $taskReportPath $report
        }
        Write-TaskJson (Join-Path $OutputRoot ('external-timeout-'+$stamp+'.json')) @{schema_version=1;case_id=$progress.case_id;deadline_observed_elapsed_seconds=$deadlineObservedElapsed;elapsed_seconds=$elapsed;page_timeout_seconds=$PageTimeoutSeconds;owned_interpreter_pid=$progress.pid;owned_launcher_pid=$taskProcess.Id}
        Remove-Item -LiteralPath $taskProgressPath -ErrorAction SilentlyContinue
        $externalTimeout=$true
        break
    }
    $report=Get-Content -LiteralPath $taskReportPath -Raw | ConvertFrom-Json
    $hostWatch.Stop()
    Write-TaskJson (Join-Path $OutputRoot ('host-attempt-'+$stamp+'.json')) @{schema_version=1;launcher_pid=$taskProcess.Id;exit_code=$taskProcess.ExitCode;external_timeout=$externalTimeout;wall_seconds=$hostWatch.Elapsed.TotalSeconds;confirmed_before=$beforeCount;confirmed_after=$report.items.Count;stdout=(Split-Path $taskStdout -Leaf);stderr=(Split-Path $taskStderr -Leaf)}
    if($report.items.Count -le $beforeCount){
        throw ('Comparison exited before confirming another page; inspect '+$taskStderr)
    }
    if($report.items.Count -ge $taskExpected){break}
    if(-not $externalTimeout -and $taskProcess.ExitCode -ne 124){
        throw ('Comparison exited '+$taskProcess.ExitCode+' before covering frozen pages')
    }
}
$report=Get-Content -LiteralPath $taskReportPath -Raw | ConvertFrom-Json
Write-TaskJson (Join-Path $OutputRoot 'external-host-complete.json') @{schema_version=1;captured_pages=$report.items.Count;expected_pages=$taskExpected;failed_pages=@($report.items | Where-Object {$_.status -eq 'failed'}).Count}
if($report.items.Count -ne $taskExpected){exit 2}
if(@($report.items | Where-Object {$_.status -eq 'failed'}).Count -gt 0){exit 1}
exit 0
