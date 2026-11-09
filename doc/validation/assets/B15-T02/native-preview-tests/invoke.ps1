$ErrorActionPreference='Stop'
$source='C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap\source'
$work='C:\Users\Lyinn\AppData\Local\Temp\noetome-b15-native-fituqqap'
Set-Location -LiteralPath $source
$env:CARGO_TARGET_DIR=Join-Path $work 'target'
$start=New-Object Diagnostics.ProcessStartInfo
$start.FileName='C:\Users\Lyinn\.cargo\bin\cargo.exe'
$start.Arguments='test -p noetome-cli --test extraction_preview --locked -- --nocapture'
$start.WorkingDirectory=$source
$start.UseShellExecute=$false
$start.CreateNoWindow=$true
$start.RedirectStandardOutput=$true
$start.RedirectStandardError=$true
$process=New-Object Diagnostics.Process
$process.StartInfo=$start
$out=[IO.File]::Open((Join-Path $work 'preview-test.stdout.log'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
$err=[IO.File]::Open((Join-Path $work 'preview-test.stderr.log'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
$watch=[Diagnostics.Stopwatch]::StartNew()
try {
 if (-not $process.Start()) {throw 'cargo did not start'}
 $outTask=$process.StandardOutput.BaseStream.CopyToAsync($out)
 $errTask=$process.StandardError.BaseStream.CopyToAsync($err)
 $process.WaitForExit()
 $outTask.GetAwaiter().GetResult()
 $errTask.GetAwaiter().GetResult()
 $code=$process.ExitCode
 $value=[ordered]@{exit_code=$code;seconds=$watch.Elapsed.TotalSeconds;command=($start.FileName+' '+$start.Arguments);cwd=$source;target_dir=$env:CARGO_TARGET_DIR;powershell=$PSVersionTable.PSVersion.ToString();os=[Environment]::OSVersion.Version.ToString()}
 [IO.File]::WriteAllText((Join-Path $work 'preview-test.result.json'),($value|ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
} finally {$out.Dispose();$err.Dispose();$process.Dispose()}
exit $code
