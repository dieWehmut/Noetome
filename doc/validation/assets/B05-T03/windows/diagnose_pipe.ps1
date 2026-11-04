$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath 'D:\Noetome\target\compiler-temp\native-b05-t03-6af0813f3f\repo'
$env:NOETOME_PDF_PYTHON = 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
$env:NOETOME_TEST_PYTHON = 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
& .\scripts\cargo_local.ps1 test -p noetome-cli --lib --locked --offline review_broken_pipe_notifies_controller_and_reaps_live_tree '--' --nocapture
$venvExit = $LASTEXITCODE
$env:NOETOME_TEST_PYTHON = 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
& .\scripts\cargo_local.ps1 test -p noetome-cli --lib --locked --offline review_broken_pipe_notifies_controller_and_reaps_live_tree '--' --nocapture
$directExit = $LASTEXITCODE
Write-Output "VENv_EXIT=$venvExit DIRECT_EXIT=$directExit"
exit $directExit
