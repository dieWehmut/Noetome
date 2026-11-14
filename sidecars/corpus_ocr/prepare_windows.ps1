[CmdletBinding()]
param([string]$RuntimeDir)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows is required.' }
if (-not $RuntimeDir) {
    $RuntimeDir = Join-Path $env:LOCALAPPDATA 'Noetome\runtimes\corpus-ocr\mineru-4.2.1'
}
$RuntimeDir = [IO.Path]::GetFullPath($RuntimeDir)
$taskOwnerPath = Join-Path $RuntimeDir 'setup-owner.json'
if (Test-Path -LiteralPath $RuntimeDir) {
    if (-not (Test-Path -LiteralPath $taskOwnerPath -PathType Leaf)) {
        throw 'Existing runtime has no Noetome OCR setup owner. Choose a new directory.'
    }
    $taskOwner = Get-Content -Raw -LiteralPath $taskOwnerPath | ConvertFrom-Json
    if ($taskOwner.producer -ne 'noetome-corpus-ocr-setup-v1') { throw 'Runtime owner differs.' }
} else {
    $null = New-Item -ItemType Directory -Path $RuntimeDir
    [ordered]@{ producer='noetome-corpus-ocr-setup-v1'; engine='mineru'; version='4.2.1' } |
        ConvertTo-Json | Set-Content -LiteralPath $taskOwnerPath -Encoding UTF8
}
$taskUv = (Get-Command uv.exe -ErrorAction Stop).Source
$taskLog = Join-Path $RuntimeDir 'setup.log'
$taskRequirements = Join-Path $PSScriptRoot 'requirements.in'
$taskLock = Join-Path $RuntimeDir 'requirements-windows.lock'
$taskEnvironment = Join-Path $RuntimeDir 'environment'
$taskPython = Join-Path $taskEnvironment 'Scripts\python.exe'
$env:UV_CACHE_DIR = Join-Path $RuntimeDir 'package-cache'
$env:UV_PYTHON_INSTALL_DIR = Join-Path $RuntimeDir 'python-managed'
$env:UV_NO_PROGRESS = '1'
$env:UV_SYSTEM_CERTS = 'true'
$env:UV_LINK_MODE = 'copy'
$env:PYTHONUTF8 = '1'

function Invoke-SetupStep([string]$Label, [string[]]$Arguments) {
    Write-Host $Label
    Add-Content -LiteralPath $taskLog -Value ("`nSTEP " + $Label) -Encoding UTF8
    # Installation output contains package URLs and failures, never model credentials.
    # PowerShell 5.1 wraps ordinary native stderr progress as NativeCommandError.
    # Capture it without treating a progress line as a failed command; the native
    # exit code remains the success criterion and is checked immediately below.
    $taskPreviousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $taskUv @Arguments 2>&1 | ForEach-Object {
            Add-Content -LiteralPath $taskLog -Value $_.ToString() -Encoding UTF8
        }
        $taskExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $taskPreviousPreference
    }
    if ($taskExitCode -ne 0) { throw "$Label failed with exit $taskExitCode; see $taskLog" }
}

Invoke-SetupStep 'Install isolated CPython 3.12.14 (no PATH/registry registration)' @(
    '--no-config', 'python', 'install', '3.12.14', '--no-bin', '--no-registry',
    '--install-dir', $env:UV_PYTHON_INSTALL_DIR
)
if (-not (Test-Path -LiteralPath $taskPython -PathType Leaf)) {
    Invoke-SetupStep 'Create isolated OCR environment' @(
        '--no-config', 'venv', '--python', '3.12.14', '--managed-python', $taskEnvironment
    )
}
if (-not (Test-Path -LiteralPath $taskLock -PathType Leaf)) {
    Invoke-SetupStep 'Resolve pinned MinerU and hash-lock native Windows dependencies' @(
        '--no-config', 'pip', 'compile', '--python', $taskPython,
        '--generate-hashes', '--output-file', $taskLock, $taskRequirements
    )
}
Invoke-SetupStep 'Install only the resolved hash-locked dependencies' @(
    '--no-config', 'pip', 'sync', '--python', $taskPython, '--require-hashes', $taskLock
)
Invoke-SetupStep 'Check installed dependency consistency' @(
    '--no-config', 'pip', 'check', '--python', $taskPython
)
$taskUvVersion = & $taskUv --version
$taskProbe = @'
import hashlib, importlib.metadata, json, platform, sys
from pathlib import Path
assert sys.platform == 'win32' and platform.machine().lower() in ('amd64','x86_64')
assert sys.version.split()[0] == '3.12.14'
assert importlib.metadata.version('mineru') == '4.2.1'
items=[]
for dist in sorted(importlib.metadata.distributions(), key=lambda d:d.metadata['Name'].lower()):
    notices=[]
    for file in dist.files or ():
        if any(word in str(file).lower() for word in ('license','copying','notice')):
            path=dist.locate_file(file)
            if path.is_file():
                notices.append({'path':str(file).replace('\\','/'),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
    items.append({'name':dist.metadata['Name'],'version':dist.version,'license':dist.metadata.get('License-Expression') or dist.metadata.get('License'),'notices':notices})
print(json.dumps({'python_version':sys.version.split()[0],'platform':sys.platform,'architecture':platform.machine(),'packages':items},ensure_ascii=False))
'@
$taskProbeResult = & $taskPython -I -B -X utf8 -c $taskProbe
if ($LASTEXITCODE -ne 0) { throw 'Installed runtime identity check failed.' }
[ordered]@{
    schema_version=1; producer='noetome-corpus-ocr-runtime-setup';
    runtime_root=$RuntimeDir; python=$taskPython; uv_version=$taskUvVersion;
    python_sha256=(Get-FileHash -LiteralPath $taskPython -Algorithm SHA256).Hash.ToLowerInvariant();
    requirements_sha256=(Get-FileHash -LiteralPath $taskLock -Algorithm SHA256).Hash.ToLowerInvariant();
    identity=($taskProbeResult | ConvertFrom-Json);
    models_ready=$false; inference_verified=$false;
    license_policy='Preserve original package and model notices; model revisions/hashes recorded separately.'
} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $RuntimeDir 'setup-receipt.json') -Encoding UTF8
Write-Host "OCR packages installed: $taskPython"
