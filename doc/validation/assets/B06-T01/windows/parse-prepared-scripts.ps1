param([Parameter(Mandatory=$true)][string]$Root)
$ErrorActionPreference='Stop'
$parseResults=@()
foreach ($fileName in @('metadata.ps1','launch-validation.ps1','launch-actual-obsidian.ps1')) {
    $scriptTokens=$null
    $scriptErrors=$null
    $null=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $Root $fileName),[ref]$scriptTokens,[ref]$scriptErrors)
    $parseResults+=@{file=$fileName;errors=@($scriptErrors | ForEach-Object {$_.ToString()});passed=@($scriptErrors).Count -eq 0}
}
$currentObs=@(Get-Process -Name Obsidian -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$currentCargo=@(Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$result=@{kind='B06 native runner preparation; no product execution';checked_at_utc=[DateTime]::UtcNow.ToString('o');parse_results=$parseResults;all_parse_passed=@($parseResults | Where-Object {-not $_.passed}).Count -eq 0;currently_running_obsidian_ids=$currentObs;currently_running_cargo_rustc_ids=$currentCargo}
$json=$result | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText((Join-Path $Root 'prepared-script-parse.json'),$json,(New-Object System.Text.UTF8Encoding($false)))
$json
if (-not $result.all_parse_passed) {exit 1}
