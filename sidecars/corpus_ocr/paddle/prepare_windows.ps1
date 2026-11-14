[CmdletBinding()]
param([string]$RuntimeDir)
$ErrorActionPreference = 'Stop'
if (-not $RuntimeDir) { $RuntimeDir = Join-Path $env:LOCALAPPDATA 'Noetome\runtimes\corpus-ocr\paddleocr-3.7.0' }
$RuntimeDir = [IO.Path]::GetFullPath($RuntimeDir)
$ownerPath = Join-Path $RuntimeDir 'setup-owner.json'
if (Test-Path -LiteralPath $RuntimeDir) {
  if (-not (Test-Path -LiteralPath $ownerPath -PathType Leaf)) { throw 'Existing directory has no Noetome setup identity.' }
  $owner = Get-Content -LiteralPath $ownerPath -Raw | ConvertFrom-Json
  if ($owner.producer -ne 'noetome-paddle-ocr-setup-v1') { throw 'Runtime owner differs.' }
} else {
  New-Item -ItemType Directory -Path $RuntimeDir | Out-Null
  @{schema_version=1;producer='noetome-paddle-ocr-setup-v1'} | ConvertTo-Json | Set-Content -LiteralPath $ownerPath -Encoding UTF8
}
$taskUv = (Get-Command uv.exe -ErrorAction Stop).Source
$taskPythonBase = Join-Path $env:LOCALAPPDATA 'Noetome\runtimes\corpus-ocr\mineru-4.2.1\environment\Scripts\python.exe'
$taskPython = Join-Path $RuntimeDir 'environment\Scripts\python.exe'
$taskLock = Join-Path $RuntimeDir 'requirements.lock'
$taskLog = Join-Path $RuntimeDir 'setup.log'
$env:UV_NO_PROGRESS='1'; $env:UV_SYSTEM_CERTS='true'; $env:UV_LINK_MODE='copy'; $env:PYTHONUTF8='1'
function Invoke-Step([string]$Label,[string[]]$Arguments) {
  Add-Content -LiteralPath $taskLog -Value $Label -Encoding UTF8
  $taskPrevious = $ErrorActionPreference
  try {
    $ErrorActionPreference='Continue'
    & $taskUv @Arguments 2>&1 | ForEach-Object { Add-Content -LiteralPath $taskLog -Value $_.ToString() -Encoding UTF8 }
    $taskExit = $LASTEXITCODE
  } finally { $ErrorActionPreference=$taskPrevious }
  if ($taskExit -ne 0) { throw "$Label failed: $taskExit; see $taskLog" }
}
if (-not (Test-Path -LiteralPath $taskPython)) {
  Invoke-Step 'Create separate native Python environment' @('--no-config','venv','--python',$taskPythonBase,(Join-Path $RuntimeDir 'environment'))
}
if (-not (Test-Path -LiteralPath $taskLock)) {
  Invoke-Step 'Resolve and hash-lock official Paddle GPU and document parser' @('--no-config','pip','compile','--python',$taskPython,'--generate-hashes','--index-strategy','unsafe-best-match','--index','https://www.paddlepaddle.org.cn/packages/stable/cu126/','--output-file',$taskLock,(Join-Path $PSScriptRoot 'requirements.in'))
}
Invoke-Step 'Install only locked dependencies' @('--no-config','pip','sync','--python',$taskPython,'--require-hashes','--index-strategy','unsafe-best-match','--index','https://www.paddlepaddle.org.cn/packages/stable/cu126/',$taskLock)
Invoke-Step 'Check dependencies' @('--no-config','pip','check','--python',$taskPython)
$taskProbe = @'
import importlib.metadata,json,sys,paddle
assert sys.version.split()[0]=='3.12.14'
assert importlib.metadata.version('paddleocr')=='3.7.0'
assert paddle.__version__=='3.2.1'
print(json.dumps({'python':sys.version.split()[0],'paddleocr':importlib.metadata.version('paddleocr'),'paddle':paddle.__version__,'compiled_cuda':paddle.is_compiled_with_cuda(),'gpu_count':paddle.device.cuda.device_count()},ensure_ascii=False))
'@
$ErrorActionPreference='Continue'
& $taskPython -I -B -X utf8 -c $taskProbe 2>&1 | Out-File -LiteralPath (Join-Path $RuntimeDir 'probe.log') -Encoding UTF8
$taskProbeExit = $LASTEXITCODE
if ($taskProbeExit -ne 0) { throw 'Native Paddle identity check failed.' }
Write-Output ('Prepared ' + $RuntimeDir)
