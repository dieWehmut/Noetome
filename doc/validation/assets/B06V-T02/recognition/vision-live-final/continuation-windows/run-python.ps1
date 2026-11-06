$ErrorActionPreference='Stop'
$env:PSExecutionPolicyPreference='Bypass'
$env:PATH='C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\native\powershell;C:\WINDOWS\System32\WindowsPowerShell\v1.0;'+$env:PATH
$env:NOETOME_PDF_PYTHON='D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
$env:NOETOME_TEST_PYTHON='C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
$env:NOETOME_TEST_BINARY='D:\Noetome\.artifacts\b06v-t02-continuation-final-20261106\repo\target\debug\noetome.exe'
Remove-Item Env:NOETOME_TANGMAO_API_KEY -ErrorAction SilentlyContinue
Set-Location 'D:\Noetome\.artifacts\b06v-t02-continuation-final-20261106\repo'
& 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe' -X utf8 -m unittest discover -s tests -p 'test_*.py' -v
exit $LASTEXITCODE
