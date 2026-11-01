# Run Cargo with a project-local Windows compiler temporary directory. This is
# useful when build scripts inherit Low integrity from a development checkout.
# Arguments are forwarded directly; no command string or permissions are changed.
# Quote '--' when forwarding Cargo's application/test argument separator in PowerShell.
$ErrorActionPreference = 'Stop'
$taskCargoArguments = $args
$taskRepoRoot = Split-Path $PSScriptRoot -Parent
$taskCompilerTemp = Join-Path $taskRepoRoot 'target\compiler-temp'
if (Test-Path -LiteralPath $taskCompilerTemp) {
    $taskTemporaryItem = Get-Item -LiteralPath $taskCompilerTemp -Force
    if (-not $taskTemporaryItem.PSIsContainer -or
        ($taskTemporaryItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Compiler temporary path must be an ordinary build directory.'
    }
}
$null = [IO.Directory]::CreateDirectory($taskCompilerTemp)
$taskPreviousTemp = $env:TEMP
$taskPreviousTmp = $env:TMP
$taskCargoExit = 1
Push-Location $taskRepoRoot
try {
    if ($env:OS -eq 'Windows_NT') {
        $env:TEMP = $taskCompilerTemp
        $env:TMP = $taskCompilerTemp
    }
    & cargo @taskCargoArguments
    $taskCargoExit = $LASTEXITCODE
} finally {
    $env:TEMP = $taskPreviousTemp
    $env:TMP = $taskPreviousTmp
    Pop-Location
}
exit $taskCargoExit
