param([Parameter(Mandatory=$true)][string]$Root,[Parameter(Mandatory=$true)][string]$CaseName)
$ErrorActionPreference='Stop'
& 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe' -X utf8 (Join-Path $Root 'launch_actual_obsidian.py') $Root $CaseName
exit $LASTEXITCODE
