function Hide-Credential([string]$Value,[string]$Secret) {
    if ([string]::IsNullOrEmpty($Secret)) { return $Value }
    $parts=foreach ($character in $Secret.ToCharArray()) {
        $literal=[regex]::Escape([string]$character)
        $json=(([string]$character | ConvertTo-Json -Compress).Trim('"'))
        '(?:'+$literal+'|'+[regex]::Escape($json)+'|\\u'+([int]$character).ToString('x4')+')'
    }
    return [regex]::Replace($Value,($parts -join ''),'[redacted]',[Text.RegularExpressions.RegexOptions]::IgnoreCase)
}

$key='noetome-test-scan-public-only-20261106'
$single=-join ($key.ToCharArray() | ForEach-Object { '\u'+([int]$_).ToString('x4') })
$double=$single.Replace('\','\\')
$records=foreach ($entry in @(@('plain',$key),@('single_unicode',$single),@('nested_json_unicode',$double))) {
 $result=Hide-Credential $entry[1] $key
 [pscustomobject]@{name=$entry[0];input_length=$entry[1].Length;output_length=$result.Length;removed=($result -ceq '[redacted]');unchanged=($result -ceq $entry[1])}
}
[pscustomobject]@{runtime=$PSVersionTable.PSVersion.ToString();results=@($records)} | ConvertTo-Json -Depth 5 -Compress
if (@($records | Where-Object { -not $_.removed }).Count -gt 0) { exit 1 }
