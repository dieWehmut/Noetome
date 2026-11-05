$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $MyInvocation.MyCommand.Path
$taskConfig=Get-Content -LiteralPath (Join-Path $taskRoot 'root.json') -Raw | ConvertFrom-Json
$taskTemp=Join-Path $taskRoot 'credential-diagnostic-temp'
$null=New-Item -ItemType Directory -Path $taskTemp
$env:TEMP=$taskTemp
$env:TMP=$taskTemp
$env:PYTHONUTF8='1'
Set-Location -LiteralPath $taskConfig.repo_native
& $taskConfig.python_native -X utf8 -m unittest discover -s tests -p test_api_credential_scripts.py -v
exit $LASTEXITCODE
