param([Parameter(Mandatory=$true)][string]$Work)
$ErrorActionPreference='Stop'
$ownedLaunch=Get-Content -Raw -LiteralPath (Join-Path $Work 'launch.json') | ConvertFrom-Json
$ownedProcess=@(Get-CimInstance Win32_Process -Filter "ProcessId=$($ownedLaunch.pid)" | Select-Object ProcessId,ParentProcessId,CreationDate,ExecutablePath,CommandLine)
$ownedListener=@(Get-NetTCPConnection -LocalPort $ownedLaunch.port -State Listen -ErrorAction SilentlyContinue | Select-Object LocalAddress,LocalPort,OwningProcess)
$result=@{checked_at_utc=[DateTime]::UtcNow.ToString('o');owned_launch_pid=$ownedLaunch.pid;owned_port=$ownedLaunch.port;process_rows=$ownedProcess;listen_rows=$ownedListener;owned_pid_absent=$ownedProcess.Count -eq 0;owned_listener_absent=$ownedListener.Count -eq 0;scope='Read-only probe after own-profile-asserted app.quit; no other process is stopped'}
$json=$result | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText((Join-Path $Work 'cleanup-native.json'),$json,(New-Object System.Text.UTF8Encoding($false)))
$json
if (-not ($result.owned_pid_absent -and $result.owned_listener_absent)) {exit 1}
