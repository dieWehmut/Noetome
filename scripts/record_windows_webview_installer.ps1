[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$NsisScript,
    [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'WebView installer evidence requires Windows.' }
$taskScript = Get-Item -LiteralPath $NsisScript -Force
if ($taskScript.PSIsContainer) { throw 'The generated NSIS script must be a file.' }
$taskText = [IO.File]::ReadAllText($taskScript.FullName, [Text.Encoding]::UTF8)
$taskMatches = [regex]::Matches($taskText, '(?m)^\s*!define\s+WEBVIEW2INSTALLERPATH\s+"([^"\r\n]+)"\s*$')
if ($taskMatches.Count -ne 1) { throw 'Exactly one generated WEBVIEW2INSTALLERPATH is required.' }
$taskVendorPath = $taskMatches[0].Groups[1].Value
if (-not [IO.Path]::IsPathRooted($taskVendorPath) -or $taskVendorPath.Contains('$')) {
    throw 'The embedded WebView installer path must be an explicit absolute file.'
}
$taskVendor = Get-Item -LiteralPath $taskVendorPath -Force
if ($taskVendor.PSIsContainer -or ($taskVendor.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'The embedded WebView installer must be an ordinary file.'
}
$taskPin = (Get-FileHash -LiteralPath $taskVendor.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$taskSignature = Get-AuthenticodeSignature -LiteralPath $taskVendor.FullName
if ($taskSignature.Status -ne 'Valid' -or -not $taskSignature.SignerCertificate -or
    $taskSignature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
    throw 'The embedded WebView installer requires a valid Microsoft Authenticode signature.'
}
if ((Get-FileHash -LiteralPath $taskVendor.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskPin) {
    throw 'The embedded WebView installer changed during evidence capture.'
}
$taskEvidence = [ordered]@{
    schema_version = 1
    producer = 'noetome-embedded-webview2-installer-evidence'
    source_commit = $env:GITHUB_SHA
    architecture = 'x64'
    source_url = 'https://go.microsoft.com/fwlink/?linkid=2124701'
    generated_nsis_script = $taskScript.FullName
    generated_nsis_sha256 = (Get-FileHash -LiteralPath $taskScript.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    installer_path = $taskVendor.FullName
    installer_sha256 = $taskPin
    installer_bytes = $taskVendor.Length
    product_version = $taskVendor.VersionInfo.ProductVersion
    file_version = $taskVendor.VersionInfo.FileVersion
    signature_status = $taskSignature.Status.ToString()
    signer_subject = $taskSignature.SignerCertificate.Subject
    signer_thumbprint = $taskSignature.SignerCertificate.Thumbprint
    missing_webview_install_branch = 'not_run; existing runtime can make NSIS skip this branch'
}
if (Test-Path -LiteralPath $OutputPath) { throw 'Evidence output already exists.' }
[IO.File]::WriteAllText([IO.Path]::GetFullPath($OutputPath), ($taskEvidence | ConvertTo-Json -Depth 6) + "`n", (New-Object Text.UTF8Encoding($false)))
$taskEvidence | ConvertTo-Json -Depth 6
