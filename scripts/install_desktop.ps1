param(
    [string]$InstallerPath,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\Noetome')
)

$ErrorActionPreference = 'Stop'
$taskRepoRoot = Split-Path $PSScriptRoot -Parent
if (-not $InstallerPath) {
    $taskInstallers = @(Get-ChildItem -LiteralPath (Join-Path $taskRepoRoot 'target\release\bundle\nsis') -Filter '*-setup.exe')
    if ($taskInstallers.Count -ne 1) { throw 'Expected exactly one built NSIS installer. Pass -InstallerPath explicitly.' }
    $InstallerPath = $taskInstallers[0].FullName
}
$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$taskExePath = Join-Path $InstallDir 'noetome-desktop.exe'
if ((Test-Path -LiteralPath $InstallDir) -and (-not (Test-Path -LiteralPath $taskExePath))) {
    if (@(Get-ChildItem -LiteralPath $InstallDir -Force).Count -gt 0) {
        throw 'Installation target has unknown existing content. Choose a new -InstallDir.'
    }
}

# NSIS /D must be the final argument and is deliberately not quoted, even with spaces.
$taskInstaller = Start-Process -FilePath $InstallerPath -ArgumentList "/S /D=$InstallDir" -WindowStyle Hidden -PassThru -Wait
if ($taskInstaller.ExitCode -ne 0) { throw "Installer failed with exit code $($taskInstaller.ExitCode)" }
if (-not (Test-Path -LiteralPath $taskExePath)) { throw 'Installer exited successfully but the app executable is missing.' }
$taskBuiltExe = Join-Path $taskRepoRoot 'target\release\noetome-desktop.exe'
$taskInstalledHash = (Get-FileHash -LiteralPath $taskExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$taskBuiltHash = (Get-FileHash -LiteralPath $taskBuiltExe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($taskInstalledHash -ne $taskBuiltHash) { throw 'Installed executable differs from the bundled build.' }

$taskPrograms = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$taskShortcuts = @(Get-ChildItem -LiteralPath $taskPrograms -Filter 'Noetome.lnk' -Recurse | ForEach-Object { $_.FullName })
$taskUninstall = @(Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq 'Noetome' } | Select-Object DisplayName, DisplayVersion, InstallLocation)
$taskEvidenceDir = Join-Path $taskRepoRoot '.artifacts'
New-Item -ItemType Directory -Path $taskEvidenceDir -Force | Out-Null
@{
    installer = $InstallerPath
    installer_sha256 = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    installer_exit_code = $taskInstaller.ExitCode
    installed_executable = $taskExePath
    executable_sha256 = $taskInstalledHash
    matches_built_executable = $true
    shortcuts = $taskShortcuts
    uninstall_records = $taskUninstall
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $taskEvidenceDir 'desktop-install.json') -Encoding utf8
Write-Output "Installed: $taskExePath"
Write-Output "Executable SHA256: $taskInstalledHash"
Write-Output 'Installation evidence: .artifacts/desktop-install.json'
