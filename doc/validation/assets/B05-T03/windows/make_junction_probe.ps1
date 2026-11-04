param([Parameter(Mandatory=$true)][string]$Alias,[Parameter(Mandatory=$true)][string]$Target)
$ErrorActionPreference = "Stop"
if (Test-Path -LiteralPath $Alias) { throw "alias already exists" }
& $env:ComSpec /d /c mklink /J $Alias $Target
exit $LASTEXITCODE
