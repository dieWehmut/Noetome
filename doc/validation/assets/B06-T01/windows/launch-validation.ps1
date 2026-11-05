param([Parameter(Mandatory=$true)][string]$Root)
$ErrorActionPreference = 'Stop'
& 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe' -X utf8 (Join-Path $Root 'validate_native.py') $Root
exit $LASTEXITCODE
