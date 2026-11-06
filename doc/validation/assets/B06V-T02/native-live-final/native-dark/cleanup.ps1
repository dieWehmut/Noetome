$ErrorActionPreference='Stop'
$profilePath='D:\Noetome\.artifacts\b06v-t02-live-scan-20261106\obsidian-clear-dark-prepared\captures\clear-native_math\profile'
$receiptPid=35688
$receiptPort=63151
$candidates=@(Get-CimInstance Win32_Process -Filter "Name='Obsidian.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($profilePath) })
$terminated=@()
foreach($candidate in $candidates) {
    if ($receiptPid -eq 0 -or $candidate.ProcessId -eq $receiptPid) {
        $terminated += $candidate.ProcessId
        taskkill.exe /PID $candidate.ProcessId /T /F | Out-Null
    }
}
Start-Sleep -Milliseconds 300
$remaining=@(Get-CimInstance Win32_Process -Filter "Name='Obsidian.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($profilePath) })
$pidState=if($receiptPid -ne 0){Get-Process -Id $receiptPid -ErrorAction SilentlyContinue}else{$null}
$listener=if($receiptPort -ne 0){Get-NetTCPConnection -LocalPort $receiptPort -State Listen -ErrorAction SilentlyContinue}else{$null}
@{ownedPid=$receiptPid;ownedPort=$receiptPort;ownedPidAbsent=($null -eq $pidState);ownedListenerAbsent=($null -eq $listener);profileProcessCount=$remaining.Count;emergencyTerminated=$terminated;checkedUtc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json -Compress
