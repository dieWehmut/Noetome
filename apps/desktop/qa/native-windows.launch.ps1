param([Parameter(Mandatory=$true)][string]$Manifest)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$config = Get-Content -LiteralPath $Manifest -Raw -Encoding UTF8 | ConvertFrom-Json
if ($config.schema_version -ne 1 -or $config.mode -notin @('local_staged_payload','ci_clean_installed','local_ci_installed_payload')) { throw 'Unknown QA manifest identity.' }
$tempParent = [IO.Path]::GetTempPath()
$runRoot = Join-Path $tempParent ('noetome-native-ui-run-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($runRoot) | Out-Null
$evidence = [IO.Path]::GetFullPath($config.evidence_root)
[IO.Directory]::CreateDirectory($evidence) | Out-Null
$utf8 = New-Object Text.UTF8Encoding($false)
function Save-Json([string]$Name, $Value) { [IO.File]::WriteAllText((Join-Path $evidence $Name), ($Value | ConvertTo-Json -Depth 100), $utf8) }
function Plain-Path([string]$Selected) {
    if (-not [IO.Path]::IsPathRooted($Selected) -or $Selected -match '(^|[\\/])\.\.?([\\/]|$)') { throw 'QA input must be an absolute plain path.' }
    $cursor = [IO.Path]::GetFullPath($Selected)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'QA paths cannot contain links or reparse points.' }
        }
        $parent = [IO.Directory]::GetParent($cursor)
        if (-not $parent) { break }; $cursor = $parent.FullName
    }
}
foreach ($selected in @($config.payload_root,$config.desktop_exe,$config.cli_exe,$config.text_pdf,$config.scan_pdf,$config.playwright_module)) { Plain-Path $selected }
$payload = [IO.Path]::GetFullPath($config.payload_root).TrimEnd('\') + '\'
foreach ($exe in @($config.desktop_exe,$config.cli_exe)) {
    if (-not [IO.Path]::GetFullPath($exe).StartsWith($payload,[StringComparison]::OrdinalIgnoreCase)) { throw 'Executables must belong to the selected payload.' }
}
if ((Get-FileHash -LiteralPath $config.desktop_exe -Algorithm SHA256).Hash.ToLowerInvariant() -ne $config.desktop_sha256) { throw 'Desktop payload fingerprint changed.' }
if ((Get-FileHash -LiteralPath $config.cli_exe -Algorithm SHA256).Hash.ToLowerInvariant() -ne $config.cli_sha256) { throw 'CLI payload fingerprint changed.' }
$selectedRoot = Join-Path $runRoot '作者 选取资料'
$projectParent = Join-Path $runRoot '中文 空格项目父目录'
$outputParent = Join-Path $runRoot '中文 空格输出父目录'
$localData = Join-Path $runRoot 'local-app-data'
foreach ($directory in @($selectedRoot,$projectParent,$outputParent,$localData)) { [IO.Directory]::CreateDirectory($directory) | Out-Null }
$textPdf = Join-Path $selectedRoot '作者 文字资料.pdf'; $scanPdf = Join-Path $selectedRoot '作者 扫描资料.pdf'
[IO.File]::Copy($config.text_pdf,$textPdf,$false); [IO.File]::Copy($config.scan_pdf,$scanPdf,$false)
$dataRoot = Join-Path $runRoot 'isolated-profile'
if (Test-Path -LiteralPath $dataRoot) { throw 'Explicit application data root must start absent.' }
$listener = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0)
$listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
$runtime = [ordered]@{}
foreach ($property in $config.PSObject.Properties) { $runtime[$property.Name] = $property.Value }
$runtime['run_root']=$runRoot; $runtime['data_root']=$dataRoot; $runtime['local_app_data']=$localData
$runtime['project_parent']=$projectParent; $runtime['output_parent']=$outputParent
$runtime['selected_text_pdf']=$textPdf; $runtime['selected_scan_pdf']=$scanPdf; $runtime['port']=$port
$runtimePath=Join-Path $runRoot 'native-ui-runtime-manifest.json'; [IO.File]::WriteAllText($runtimePath,($runtime|ConvertTo-Json -Depth 100),$utf8)
Save-Json 'runtime-manifest.json' $runtime
$desktop=$null; $node=$null; $exitCode=1
function Child-StartInfo([string]$Executable) {
    $info=New-Object Diagnostics.ProcessStartInfo
    $info.FileName=$Executable; $info.UseShellExecute=$false; $info.WorkingDirectory=$runRoot
    foreach ($key in @($info.EnvironmentVariables.Keys)) { if ($key -like 'NOETOME_*' -or $key -like 'WEBVIEW2_*') { $info.EnvironmentVariables.Remove($key) } }
    $info.EnvironmentVariables['LOCALAPPDATA']=$localData
    $info.EnvironmentVariables['NOETOME_DESKTOP_DATA_ROOT']=$dataRoot
    $info.EnvironmentVariables['WEBVIEW2_USER_DATA_FOLDER']=(Join-Path $dataRoot 'webview')
    $info.EnvironmentVariables['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS']=('--remote-debugging-address=127.0.0.1 --remote-debugging-port='+$port)
    return $info
}
try {
    $desktopInfo=Child-StartInfo $config.desktop_exe
    $desktop=New-Object Diagnostics.Process; $desktop.StartInfo=$desktopInfo
    if (-not $desktop.Start()) { throw 'Actual desktop launch failed.' }
    Save-Json 'desktop-launch.json' @{pid=$desktop.Id;executable=$config.desktop_exe;port=$port;data_root=$dataRoot;webview_data_root=(Join-Path $dataRoot 'webview');mode=$config.mode;windows_version=[Environment]::OSVersion.VersionString;node_version=(& $config.windows_node --version);source_commit=$config.source_commit;started_utc=[DateTime]::UtcNow.ToString('o')}
    $ready=$false
    for($iteration=0;$iteration -lt 120;$iteration++) {
        if ($desktop.HasExited) { throw ('Actual desktop exited before CDP with code '+$desktop.ExitCode) }
        try { $version=Invoke-RestMethod -Uri ('http://127.0.0.1:'+ $port +'/json/version') -TimeoutSec 1; if ($version.webSocketDebuggerUrl) { $ready=$true; break } } catch {}
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) { throw 'Actual WebView2 CDP did not become ready.' }
    Save-Json 'actual-cdp-version.json' $version
    $nodeInfo=Child-StartInfo $config.windows_node
    $nodeInfo.Arguments=('"'+$config.harness+'" "'+$runtimePath+'"')
    $nodeInfo.RedirectStandardOutput=$true; $nodeInfo.RedirectStandardError=$true; $nodeInfo.CreateNoWindow=$true
    $node=New-Object Diagnostics.Process; $node.StartInfo=$nodeInfo
    if (-not $node.Start()) { throw 'Native CDP harness failed to start.' }
    $out=$node.StandardOutput.ReadToEndAsync(); $err=$node.StandardError.ReadToEndAsync()
    if (-not $node.WaitForExit(300000)) { throw 'Native CDP harness exceeded five minutes.' }
    [IO.File]::WriteAllText((Join-Path $evidence 'harness.stdout.log'),$out.Result,$utf8)
    [IO.File]::WriteAllText((Join-Path $evidence 'harness.stderr.log'),$err.Result,$utf8)
    $exitCode=$node.ExitCode
    $receiptPath=Join-Path $evidence 'native-ui-result.json'
    if (-not (Test-Path -LiteralPath $receiptPath)) { throw 'Native harness returned no result receipt.' }
    $receipt=Get-Content -LiteralPath $receiptPath -Raw -Encoding UTF8|ConvertFrom-Json
    if ($exitCode -ne 0 -or $receipt.passed -ne $true -or @($receipt.checks).Count -lt 12 -or @($receipt.checks|Where-Object {$_.passed -ne $true}).Count -ne 0) { throw 'Actual native UI validation failed; inspect retained raw evidence.' }
    $credential=Join-Path $dataRoot 'model-profile\credentials\tangmao-deepseek.dpapi'
    $ledger=Join-Path $dataRoot 'model-profile\model-services\tangmao-deepseek.usage.json'
    if ((Test-Path -LiteralPath $credential) -or (Test-Path -LiteralPath $ledger)) { throw 'UI QA created unexpected credential or usage ledger.' }
    Save-Json 'profile-isolation.json' @{root=$dataRoot;marker=(Test-Path -LiteralPath (Join-Path $dataRoot '.noetome-desktop-data.json'));credential_created=$false;ledger_created=$false;real_user_paths_accessed_by_harness=$false;local_install_actions=0}
    $exitCode=0
} catch {
    Save-Json 'launcher-error.json' @{message=$_.Exception.Message;details=$_.ToString();exit_code=1}; $exitCode=1
} finally {
    foreach ($owned in @($node,$desktop)) {
        if ($owned -and -not $owned.HasExited) { & "$env:SystemRoot\System32\taskkill.exe" /PID $owned.Id /T /F | Out-Null }
    }
    Save-Json 'launcher-result.json' @{exit_code=$exitCode;mode=$config.mode;run_root=$runRoot;finished_utc=[DateTime]::UtcNow.ToString('o');local_install_actions=0}
}
exit $exitCode
