$ErrorActionPreference='Stop'
$env:PSExecutionPolicyPreference='Bypass'
$env:NOETOME_PDF_PYTHON='D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
$env:NOETOME_TEST_PYTHON='C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
Set-Location 'D:\Noetome\.artifacts\b06v-t02-python-live-final-20261106\repo'
& 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe' -X utf8 -m unittest discover -s tests -p 'test_*.py'
exit $LASTEXITCODE
