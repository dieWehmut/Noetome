# Native Windows regression. All DPAPI values, workers and paths are newly created synthetic test data.
# This test makes no HTTP requests and never opens a real model credential or ledger.
[CmdletBinding()]
param([string]$Root)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'WINDOWS_REQUIRED' }
if (-not $Root) { $Root=[IO.Path]::Combine([IO.Path]::GetTempPath(),'noetome-compile-credential-check-'+[Guid]::NewGuid().ToString('N')) }
if ([IO.Directory]::Exists($Root) -or [IO.File]::Exists($Root)) { throw 'CHECK_ROOT_MUST_BE_NEW' }
$null=[IO.Directory]::CreateDirectory($Root)
. (Join-Path $PSScriptRoot 'text_api_support.ps1')
$null=Selected-Path $Root
foreach ($name in @('invoke_compile_credential.ps1','text_api_support.ps1')) {
    $source=Open-Selected (Join-Path $PSScriptRoot $name)
    $null=Save-New (Join-Path $Root $name) (Bytes-Stream $source)
}
$helper=Read-Selected (Open-Selected (Join-Path $Root 'invoke_compile_credential.ps1')) 65536
$match=[regex]::Match($helper,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@")
if (-not $match.Success) { throw 'CHECK_HELPER_CSHARP_MISSING' }
[IO.File]::WriteAllText((Join-Path $Root 'helper-csharp.txt'),$match.Groups[1].Value)
$checks=New-Object 'Collections.Generic.List[object]'
$errors=$null; $tokens=$null
$null=[Management.Automation.Language.Parser]::ParseFile((Join-Path $Root 'invoke_compile_credential.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count -ne 0) { throw 'full-helper-syntax-error' }
$checks.Add([pscustomobject]@{name='full PowerShell helper syntax';passed=$true})
Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $Root 'helper-csharp.txt')))
$checks.Add([pscustomobject]@{name='actual Windows C# compilation';passed=$true})
$binary=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$child=Join-Path $Root 'child.ps1'
[IO.File]::WriteAllText($child,@'
param([string]$Marker,[int]$WaitSeconds)
[IO.File]::WriteAllText($Marker,'started')
[Console]::Out.Write('{"mode":"synthetic_zero_requests","child_environment_present":'+([Environment]::GetEnvironmentVariable('NOETOME_COMPILE_SYNTHETIC_CREDENTIAL') -eq 'synthetic-child-value').ToString().ToLowerInvariant()+'}')
[Console]::Error.Write('synthetic-error')
Start-Sleep -Seconds $WaitSeconds
exit 0
'@)
$marker=Join-Path $Root 'normal.marker'
$args=((@($binary,'-NoProfile','-File',$child,'-Marker',$marker,'-WaitSeconds','0')) | ForEach-Object {Native-Argument $_}) -join ' '
$job=New-Object Noetome.TextInvokeJob
$worker=New-Object Noetome.CompileProcess($binary,$args,$Root,'NOETOME_COMPILE_SYNTHETIC_CREDENTIAL','synthetic-child-value',$job.Handle)
try {
 if ($job.ActiveProcesses() -ne 1 -or (Test-Path -LiteralPath $marker)) { throw 'not-contained-before-resume' }
 $checks.Add([pscustomobject]@{name='native Job assignment at creation before execution';passed=$true})
 $worker.Resume()
 $out=[Noetome.TextInvokeNative]::ReadBounded($worker.Stdout,1048576)
 $err=[Noetome.TextInvokeNative]::ReadBounded($worker.Stderr,1048576)
 if (-not $worker.Process.WaitForExit(10000)) { throw 'synthetic-exit-timeout' }
 $stdout=$out.GetAwaiter().GetResult(); $stderr=$err.GetAwaiter().GetResult()
 if ($stdout -cne '{"mode":"synthetic_zero_requests","child_environment_present":true}' -or $stderr -cne 'synthetic-error' -or $worker.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $marker)) { [pscustomobject]@{stdout=$stdout;stderr=$stderr;code=$worker.ExitCode;marker=(Test-Path -LiteralPath $marker)} | ConvertTo-Json -Depth 10; throw 'output-or-private-child-environment-invalid' }
 if ($env:NOETOME_COMPILE_SYNTHETIC_CREDENTIAL) { throw 'parent-environment-contaminated' }
 $checks.Add([pscustomobject]@{name='bounded native stdout/stderr and child-only synthetic environment';passed=$true})
} finally { $job.Dispose(); $worker.Dispose() }
$marker=Join-Path $Root 'kill.marker'
$args=((@($binary,'-NoProfile','-File',$child,'-Marker',$marker,'-WaitSeconds','300')) | ForEach-Object {Native-Argument $_}) -join ' '
$job=New-Object Noetome.TextInvokeJob
$worker=New-Object Noetome.CompileProcess($binary,$args,$Root,'NOETOME_COMPILE_SYNTHETIC_CREDENTIAL','synthetic-child-value',$job.Handle)
try {
 $held=$worker.Process; $null=$held.Handle
 $worker.Resume()
 $watch=[Diagnostics.Stopwatch]::StartNew()
 while (-not (Test-Path -LiteralPath $marker)) { if ($watch.ElapsedMilliseconds -gt 10000) { throw 'start-marker-timeout' }; Start-Sleep -Milliseconds 20 }
 $job.Dispose()
 if (-not $held.WaitForExit(5000)) { throw 'job-close-left-worker' }
 $checks.Add([pscustomobject]@{name='closing launcher-owned Job confirms held worker identity exit';passed=$true})
} finally { $job.Dispose(); $worker.Dispose() }

# Entire bridge with a new synthetic CurrentUser DPAPI value and a fake CLI.
$credential=Join-Path $Root 'synthetic.dpapi'
$synthetic=ConvertTo-SecureString 'synthetic-child-value' -AsPlainText -Force
try { [IO.File]::WriteAllText($credential,($synthetic | ConvertFrom-SecureString)) } finally { $synthetic.Dispose() }
$fake=Join-Path $Root 'fake-noetome.exe'
Add-Type -OutputAssembly $fake -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.IO;
using System.Diagnostics;
public static class FakeCompileCli {
 public static int Main(string[] args) {
  if (args.Length!=4 || args[0]!="compile" || args[1]!="run" || !Directory.Exists(args[2])) return 4;
  if (Environment.GetEnvironmentVariable("NOETOME_TANGMAO_API_KEY")!="synthetic-child-value") return 5;
  File.WriteAllText(Path.Combine(args[2],"fake-invoked-"+args[3]),"invoked");
  if (args[3]=="synthetic-kill") {
   File.WriteAllText(Path.Combine(args[2],"worker.pid"),Process.GetCurrentProcess().Id.ToString());
   System.Threading.Thread.Sleep(300000);
  }
  if (args[3]=="synthetic-descendant") {
   var start=new ProcessStartInfo(Path.Combine(Environment.GetEnvironmentVariable("SystemRoot"),"System32","WindowsPowerShell","v1.0","powershell.exe"),"-NoProfile -Command \"Start-Sleep -Seconds 300\"");
   start.UseShellExecute=false; start.CreateNoWindow=true;
   var child=Process.Start(start);
   File.WriteAllText(Path.Combine(args[2],"descendant.pid"),child.Id.ToString());
   System.Threading.Thread.Sleep(800);
  }
  Console.Write("{\"run_id\":\""+args[3]+"\",\"mode\":\"fake_CLI_zero_model_requests\",\"status\":\"completed\"}");
  return 0;
 }
}
'@
function Full-Helper([string]$Id) {
 $arguments=((@($binary,'-NoProfile','-ExecutionPolicy','Bypass','-File',(Join-Path $Root 'invoke_compile_credential.ps1'),'-Binary',$fake,'-Project',$Root,'-RunId',$Id,'-CredentialFile',$credential)) | ForEach-Object { Native-Argument $_ }) -join ' '
 $parentJob=New-Object Noetome.TextInvokeJob
 try { $worker=New-Object Noetome.CompileProcess($binary,$arguments,$Root,'NOETOME_COMPILE_HELPER_TEST','synthetic-helper',$parentJob.Handle); $worker.Resume() }
 catch { $parentJob.Dispose(); throw }
 $out=[Noetome.TextInvokeNative]::ReadBounded($worker.Stdout,1048576)
 $err=[Noetome.TextInvokeNative]::ReadBounded($worker.Stderr,1048576)
 return [pscustomobject]@{process=$worker.Process;worker=$worker;job=$parentJob;stdout=$out;stderr=$err}
}
$full=Full-Helper 'synthetic-run'
try {
 if (-not $full.process.WaitForExit(15000)) { $full.process.Kill(); throw 'fake-full-helper-timeout' }
 $stdout=$full.stdout.GetAwaiter().GetResult(); $stderr=$full.stderr.GetAwaiter().GetResult()
 if ($full.worker.ExitCode -ne 0 -or $stderr.Length -ne 0 -or $stdout -cne '{"run_id":"synthetic-run","mode":"fake_CLI_zero_model_requests","status":"completed"}') { [pscustomobject]@{code=$full.worker.ExitCode;stdout=$stdout;stderr=$stderr} | ConvertTo-Json -Depth 10; throw 'fake-full-helper-output' }
 $checks.Add([pscustomobject]@{name='full native helper reads newly created synthetic DPAPI and invokes only fake compile CLI';passed=$true})
} finally { $full.job.Dispose(); $full.worker.Dispose() }
$full=Full-Helper 'synthetic-descendant'; $held=$null
try {
 $pidFile=Join-Path $Root 'descendant.pid'; $watch=[Diagnostics.Stopwatch]::StartNew()
 while (-not (Test-Path -LiteralPath $pidFile)) { if ($watch.ElapsedMilliseconds -gt 15000) { throw 'descendant-pid-timeout' }; Start-Sleep -Milliseconds 10 }
 $held=[Diagnostics.Process]::GetProcessById([int][IO.File]::ReadAllText($pidFile)); $null=$held.Handle
 if (-not $full.process.WaitForExit(15000)) { $full.process.Kill(); throw 'helper-left-pipe-holding-descendant' }
 if (-not $held.WaitForExit(5000)) { throw 'owned-descendant-identity-not-exited' }
 $stdout=$full.stdout.GetAwaiter().GetResult(); $stderr=$full.stderr.GetAwaiter().GetResult()
 if ($full.worker.ExitCode -ne 0 -or $stderr.Length -ne 0 -or $stdout -cne '{"run_id":"synthetic-descendant","mode":"fake_CLI_zero_model_requests","status":"completed"}') { [pscustomobject]@{code=$full.worker.ExitCode;stdout=$stdout;stderr=$stderr} | ConvertTo-Json -Depth 10; throw 'descendant-helper-output' }
 $checks.Add([pscustomobject]@{name='normal fake CLI root exit terminates actual held descendant before bounded pipe drain';passed=$true})
} finally { if ($null -ne $held) { $held.Dispose() }; $full.job.Dispose(); $full.worker.Dispose() }

$full=Full-Helper 'synthetic-kill'; $held=$null
try {
 $pidFile=Join-Path $Root 'worker.pid'; $watch=[Diagnostics.Stopwatch]::StartNew()
 while (-not (Test-Path -LiteralPath $pidFile)) { if ($watch.ElapsedMilliseconds -gt 15000) { throw 'worker-pid-timeout' }; Start-Sleep -Milliseconds 10 }
 $held=[Diagnostics.Process]::GetProcessById([int][IO.File]::ReadAllText($pidFile)); $null=$held.Handle
 $full.process.Kill()
 if (-not $full.process.WaitForExit(5000) -or -not $held.WaitForExit(5000)) { throw 'killed-launcher-left-worker' }
 $checks.Add([pscustomobject]@{name='actual strong kill of PowerShell launcher closes Job and confirms held fake worker identity exit';passed=$true})
} finally { if ($null -ne $held) { $held.Dispose() }; $full.job.Dispose(); $full.worker.Dispose() }
[IO.File]::WriteAllText($credential,'invalid-synthetic-ciphertext')
$full=Full-Helper 'synthetic-invalid'
try {
 if (-not $full.process.WaitForExit(15000)) { $full.process.Kill(); throw 'invalid-credential-helper-timeout' }
 $stdout=$full.stdout.GetAwaiter().GetResult(); $stderr=$full.stderr.GetAwaiter().GetResult()
 if ($full.worker.ExitCode -eq 0 -or $stdout.Length -ne 0 -or $stderr.Trim() -cne 'compile-credential-invoke: credential-unavailable' -or (Test-Path -LiteralPath (Join-Path $Root 'fake-invoked-synthetic-invalid'))) { throw 'invalid-ciphertext-launched-cli-or-leaked-detail' }
 $checks.Add([pscustomobject]@{name='invalid synthetic DPAPI ciphertext is rejected before fake CLI launch with stable public diagnostics';passed=$true})
} finally { $full.job.Dispose(); $full.worker.Dispose() }
$result=[pscustomobject]@{schema_version=1;mode='native_synthetic_zero_model_requests';passed=$checks.Count;failed=0;checks=$checks}
[IO.File]::WriteAllText((Join-Path $Root 'result.json'),($result | ConvertTo-Json -Depth 20))
$result | ConvertTo-Json -Depth 20
