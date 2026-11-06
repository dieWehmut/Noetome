$ErrorActionPreference='Stop'
$base='D:\Noetome\.artifacts\b06v-t02-scan-helper-tests-20261106i-echo-green'
$source=[IO.File]::ReadAllText($base+'\repo\scripts\invoke_scan_api_credential.ps1')
$match=[regex]::Match($source,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@")
if (-not $match.Success) { throw 'frozen source definition missing' }
Add-Type -TypeDefinition $match.Groups[1].Value
$cases=[IO.File]::ReadAllText($base+'\special-mixed-public-inputs.json') | ConvertFrom-Json
$results=foreach ($case in $cases) { [pscustomobject]@{name=$case.name;detected=[Noetome.ScanInvokeNative]::CredentialEchoOrUncertain($case.echo,$case.key)} }
$receipt=[pscustomobject]@{schema_version=1;runtime=$PSVersionTable.PSVersion.ToString();public_test_only=$true;source_sha256=(Get-FileHash -LiteralPath ($base+'\repo\scripts\invoke_scan_api_credential.ps1')).Hash.ToLowerInvariant();http_requests=0;results=$results}
$bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($receipt | ConvertTo-Json -Depth 8))
[Console]::OpenStandardOutput().Write($bytes,0,$bytes.Length)
if (@($results | Where-Object { -not $_.detected }).Count) { exit 1 }
exit 0
