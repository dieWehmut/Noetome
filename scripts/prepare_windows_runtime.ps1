[CmdletBinding()]
param(
    [string]$OutputDir,
    [switch]$PassThru
)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'The locked PDF runtime must be prepared on native Windows.' }
$taskRepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $OutputDir) { $OutputDir = Join-Path $taskRepoRoot 'target\windows-runtime' }
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
if (Test-Path -LiteralPath $OutputDir) {
    throw 'Runtime output already exists. Choose a new -OutputDir; existing content is never replaced.'
}

# Security-fix Python releases have no python.org Windows installer. This exact
# portable distribution preserves the adapter's already accepted runtime pins.
# Build-time download only; the installed application never installs packages.
$taskArchiveUrl = 'https://github.com/astral-sh/python-build-standalone/releases/download/20260929/cpython-3.12.14%2B20260929-x86_64-pc-windows-msvc-install_only_stripped.tar.gz'
$taskArchiveHash = 'f38e68f4d612ade6dd50c894fc80b14c0be0c3b5201145d6fff5f20b9323204d'
$null = New-Item -ItemType Directory -Path $OutputDir
$taskArchive = Join-Path $OutputDir 'python-runtime.tar.gz'
$taskPython = Join-Path $OutputDir 'python\python.exe'
$taskLock = Join-Path $taskRepoRoot 'sidecars\pdf_parser\runtime-lock.json'
$taskRequirements = Join-Path $taskRepoRoot 'sidecars\pdf_parser\requirements-lock.txt'

Write-Host "Downloading byte-pinned CPython 3.12.14 Windows x64 into $OutputDir"
Invoke-WebRequest -Uri $taskArchiveUrl -OutFile $taskArchive
$taskActualHash = (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($taskActualHash -ne $taskArchiveHash) { throw 'Portable Python archive SHA256 differs from the pinned release asset.' }
& tar -xzf $taskArchive -C $OutputDir
if ($LASTEXITCODE -ne 0) { throw "Portable Python extraction failed: $LASTEXITCODE" }
if (-not (Test-Path -LiteralPath $taskPython -PathType Leaf)) { throw 'Portable Python executable is missing after extraction.' }
if (-not (Test-Path -LiteralPath (Join-Path $OutputDir 'python\LICENSE.txt') -PathType Leaf)) {
    throw 'Portable Python license is missing; do not distribute this runtime.'
}

& $taskPython -I -B -X utf8 -m pip --isolated --disable-pip-version-check install --require-hashes --only-binary=:all: -r $taskRequirements | Out-Host
if ($LASTEXITCODE -ne 0) { throw "Locked PDF dependency installation failed: $LASTEXITCODE" }
& $taskPython -I -B -X utf8 -m pip --isolated --disable-pip-version-check check | Out-Host
if ($LASTEXITCODE -ne 0) { throw "PDF dependency consistency check failed: $LASTEXITCODE" }

$taskProbe = @'
import hashlib, importlib.metadata, json, platform, sys
from pathlib import Path
lock_path = Path(sys.argv[1])
lock_bytes = lock_path.read_bytes()
lock = json.loads(lock_bytes)
assert sys.version.split()[0] == lock['python_version'], 'Python version differs from runtime-lock.json'
assert sys.platform == 'win32' and platform.machine().lower() in ('amd64', 'x86_64'), 'Windows x64 required'
packages = {name: importlib.metadata.version(name) for name in lock['packages']}
assert packages == lock['packages'], 'PDF package versions differ from runtime-lock.json'
licenses = {}
for name in packages:
    distribution = importlib.metadata.distribution(name)
    notices = [str(file).replace('\\', '/') for file in distribution.files or ()
               if any(token in str(file).lower() for token in ('license', 'copying', 'notice'))]
    assert notices, f'Distribution license/notice files missing: {name}'
    assert all(distribution.locate_file(file).is_file() for file in notices), f'License file absent: {name}'
    licenses[name] = sorted(notices)
print(json.dumps({'python_version': sys.version.split()[0], 'platform': sys.platform,
                 'architecture': platform.machine(), 'packages': packages, 'licenses': licenses,
                 'runtime_lock_sha256': hashlib.sha256(lock_bytes).hexdigest()}, ensure_ascii=False))
'@
$taskProbeOutput = & $taskPython -I -B -X utf8 -c $taskProbe $taskLock
if ($LASTEXITCODE -ne 0) { throw "Locked runtime identity/license verification failed: $LASTEXITCODE" }
$taskIdentity = $taskProbeOutput | ConvertFrom-Json
$taskReceipt = [pscustomobject]@{
    schema_version = 1
    producer = 'noetome-locked-windows-pdf-runtime'
    python_relative_path = 'python/python.exe'
    archive_url = $taskArchiveUrl
    archive_sha256 = $taskActualHash
    python_executable_sha256 = (Get-FileHash -LiteralPath $taskPython -Algorithm SHA256).Hash.ToLowerInvariant()
    requirements_sha256 = (Get-FileHash -LiteralPath $taskRequirements -Algorithm SHA256).Hash.ToLowerInvariant()
    identity = $taskIdentity
    license_policy = 'Preserve python/LICENSE.txt and all installed wheel license and notice files without pruning.'
}
$taskReceipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDir 'runtime-receipt.json') -Encoding utf8
# Only the downloaded archive in the exclusively created output is discarded.
# Failed preparations remain intact so their actual failure can be investigated.
Remove-Item -LiteralPath $taskArchive
Write-Host "Locked PDF runtime verified: $taskPython"
if ($PassThru) {
    [pscustomobject]@{ Python = $taskPython; Root = $OutputDir; Receipt = $taskReceipt }
}
