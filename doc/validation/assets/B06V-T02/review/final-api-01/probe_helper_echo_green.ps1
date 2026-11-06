param([Parameter(Mandatory=$true)][string]$Helper, [Parameter(Mandatory=$true)][string]$Cases)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$before=[Console]::OutputEncoding.CodePage
$raw=[IO.File]::ReadAllText($Helper,[Text.Encoding]::UTF8)
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseInput($raw,[ref]$tokens,[ref]$errors)
if ($errors.Count -ne 0) { throw 'helper-parse-failed' }
$addType=@($ast.FindAll({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type' },$true))
if ($addType.Count -ne 1) { throw 'unexpected-helper-types' }
Invoke-Expression $addType[0].Extent.Text
$hide=@($ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Hide-Credential' },$true))
if ($hide.Count -ne 1) { throw 'missing-helper-redaction' }
Invoke-Expression $hide[0].Extent.Text
$examples=[IO.File]::ReadAllText($Cases,[Text.Encoding]::UTF8) | ConvertFrom-Json
$records=@()
foreach ($case in $examples) {
    $actual=Hide-Credential $case.value $case.secret
    $expected=if ($case.redact) { '[redacted]' } else { $case.value }
    $records+=@([pscustomobject]@{name=$case.name;passed=($actual -ceq $expected);expected_redaction=$case.redact;actual_utf8_base64=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($actual));expected_utf8_base64=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($expected))})
}
$after=[Console]::OutputEncoding.CodePage
$result=[pscustomobject]@{scope='Only Add-Type and Hide-Credential definitions from selected source; public literal fake keys; no CLI/HTTP/DPAPI/ledger';powershell=$PSVersionTable.PSVersion.ToString();console_before=$before;console_after=$after;console_unchanged=($before -eq $after);records=$records}
$encoded=[Text.Encoding]::UTF8.GetBytes(($result | ConvertTo-Json -Depth 12)+"`n")
$stream=[Console]::OpenStandardOutput();$stream.Write($encoded,0,$encoded.Length);$stream.Flush()
if ($before -ne $after -or @($records | Where-Object { -not $_.passed }).Count -ne 0) { exit 1 }
exit 0
