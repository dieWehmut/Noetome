[CmdletBinding()]
param(
    [string]$InstallerPath,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\Noetome'),
    [string]$OutputDir
)

function Get-NoetomeShortcutPaths([string]$Programs, [string]$Desktop) {
    @((Join-Path $Programs 'Noetome.lnk'),
      (Join-Path $Programs 'Noetome\Noetome.lnk'),
      (Join-Path $Desktop 'Noetome.lnk')) | Where-Object {
        Test-Path -LiteralPath $_ -PathType Leaf
    }
}

$ErrorActionPreference = 'Stop'
$taskRepoRoot = Split-Path $PSScriptRoot -Parent
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$taskExePath = Join-Path $InstallDir 'noetome-desktop.exe'
if (Test-Path -LiteralPath $InstallDir -PathType Leaf) { throw 'Installation target is an existing file. Choose a new -InstallDir.' }
if ((Test-Path -LiteralPath $InstallDir) -and @(Get-ChildItem -LiteralPath $InstallDir -Force).Count -gt 0) {
    $taskExistingRegistration = @(Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object {
        $_.DisplayName -eq 'Noetome' -and $_.InstallLocation -and
        [string]::Equals([IO.Path]::GetFullPath($_.InstallLocation.Trim('"')), $InstallDir, [StringComparison]::OrdinalIgnoreCase)
    })
    if (-not (Test-Path -LiteralPath $taskExePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $InstallDir 'uninstall.exe') -PathType Leaf) -or
        $taskExistingRegistration.Count -eq 0) {
        throw 'Installation target has unknown existing content. Choose a new -InstallDir.'
    }
}

$taskPreparation = @{}
if ($InstallerPath) { $taskPreparation.InstallerPath = $InstallerPath }
if ($OutputDir) { $taskPreparation.OutputDir = $OutputDir }
$taskPrepared = & (Join-Path $PSScriptRoot 'prepare_desktop_installer.ps1') @taskPreparation -PassThru
$InstallerPath = $taskPrepared.Path
$taskBuiltExe = Join-Path $taskRepoRoot 'target\release\noetome-desktop.exe'
if (-not (Test-Path -LiteralPath $taskBuiltExe -PathType Leaf)) { throw 'The current release executable is missing. Build the desktop app first.' }
$taskBuiltHash = (Get-FileHash -LiteralPath $taskBuiltExe -Algorithm SHA256).Hash.ToLowerInvariant()

# Tauri CLI 2.12.4 temporarily changes this unique marker for NSIS, then restores
# the source executable. Compare the entire payload against that exact variant,
# or the unchanged source when --no-binary-patching was used. No other bytes vary.
# https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.4/crates/tauri-bundler/src/bundle.rs
$taskBuildBytes = [IO.File]::ReadAllBytes($taskBuiltExe)
$taskSourceHasher = [Security.Cryptography.SHA256]::Create()
try { $taskSnapshotHash = ([BitConverter]::ToString($taskSourceHasher.ComputeHash($taskBuildBytes))).Replace('-', '').ToLowerInvariant() }
finally { $taskSourceHasher.Dispose() }
if ($taskSnapshotHash -ne $taskBuiltHash) { throw 'The current build changed while reading it. Retry after the build finishes.' }
$taskBuildText = [Text.Encoding]::ASCII.GetString($taskBuildBytes)
$taskUnknownMarker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
$taskNsisMarker = '__TAURI_BUNDLE_TYPE_VAR_NSS'
$taskMarkerOffset = $taskBuildText.IndexOf($taskUnknownMarker, [StringComparison]::Ordinal)
$taskExpectedNsisHash = $null
if ($taskMarkerOffset -ge 0) {
    if ($taskBuildText.IndexOf($taskUnknownMarker, $taskMarkerOffset + $taskUnknownMarker.Length, [StringComparison]::Ordinal) -ge 0) {
        throw 'Current executable has ambiguous Tauri bundle markers. Cannot verify the installed payload.'
    }
    $taskMarkerBytes = [Text.Encoding]::ASCII.GetBytes($taskNsisMarker)
    [Array]::Copy($taskMarkerBytes, 0, $taskBuildBytes, $taskMarkerOffset, $taskMarkerBytes.Length)
    $taskHasher = [Security.Cryptography.SHA256]::Create()
    try { $taskExpectedNsisHash = ([BitConverter]::ToString($taskHasher.ComputeHash($taskBuildBytes))).Replace('-', '').ToLowerInvariant() }
    finally { $taskHasher.Dispose() }
}

# NSIS /D must be the final argument and is deliberately not quoted, even with spaces.
$taskInstaller = Start-Process -FilePath $InstallerPath -ArgumentList "/S /D=$InstallDir" -WindowStyle Hidden -PassThru -Wait
$taskInstalledHash = $null
if (Test-Path -LiteralPath $taskExePath -PathType Leaf) {
    $taskInstalledHash = (Get-FileHash -LiteralPath $taskExePath -Algorithm SHA256).Hash.ToLowerInvariant()
}
$taskCurrentBuiltHash = (Get-FileHash -LiteralPath $taskBuiltExe -Algorithm SHA256).Hash.ToLowerInvariant()
$taskCurrentInstallerHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$taskMatchMode = $null
if ($taskInstalledHash -and $taskInstalledHash -eq $taskBuiltHash) { $taskMatchMode = 'exact_current_build' }
elseif ($taskInstalledHash -and $taskExpectedNsisHash -and $taskInstalledHash -eq $taskExpectedNsisHash) { $taskMatchMode = 'current_build_with_tauri_nsis_marker' }

$taskPrograms = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$taskShortcuts = @(Get-NoetomeShortcutPaths -Programs $taskPrograms -Desktop ([Environment]::GetFolderPath('Desktop')))
$taskShortcutEvidence = @()
$taskShell = New-Object -ComObject WScript.Shell
try {
    foreach ($taskShortcutPath in $taskShortcuts) {
        $taskShortcut = $taskShell.CreateShortcut($taskShortcutPath)
        $taskShortcutEvidence += [pscustomobject]@{ path = $taskShortcutPath; target = $taskShortcut.TargetPath; icon = $taskShortcut.IconLocation }
    }
} finally { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($taskShell) }
$taskUninstall = @(Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq 'Noetome' } | Select-Object DisplayName, DisplayVersion, InstallLocation)
$taskHasRegistration = @($taskUninstall | Where-Object {
    $_.InstallLocation -and [string]::Equals([IO.Path]::GetFullPath($_.InstallLocation.Trim('"')), $InstallDir, [StringComparison]::OrdinalIgnoreCase)
}).Count -gt 0
$taskHasShortcut = @($taskShortcutEvidence | Where-Object {
    [string]::Equals($_.target, $taskExePath, [StringComparison]::OrdinalIgnoreCase)
}).Count -gt 0
$taskEvidenceDir = Join-Path $taskRepoRoot '.artifacts'
New-Item -ItemType Directory -Path $taskEvidenceDir -Force | Out-Null
@{
    source_installer = $taskPrepared.SourcePath
    source_installer_sha256 = $taskPrepared.SourceSHA256
    installer = $InstallerPath
    installer_sha256 = $taskPrepared.SHA256
    installer_integrity_label = $taskPrepared.IntegrityLabel
    installer_integrity_rid = $taskPrepared.IntegrityRid
    installer_unchanged = ($taskCurrentInstallerHash -eq $taskPrepared.SHA256)
    installer_exit_code = $taskInstaller.ExitCode
    installed_executable = $taskExePath
    executable_sha256 = $taskInstalledHash
    current_build_executable = $taskBuiltExe
    current_build_sha256 = $taskBuiltHash
    expected_nsis_executable_sha256 = $taskExpectedNsisHash
    build_unchanged = ($taskCurrentBuiltHash -eq $taskBuiltHash)
    matches_current_build = [bool]$taskMatchMode
    executable_comparison = $taskMatchMode
    shortcuts = $taskShortcutEvidence
    uninstall_records = $taskUninstall
    has_matching_shortcut = $taskHasShortcut
    has_matching_uninstall_record = $taskHasRegistration
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $taskEvidenceDir 'desktop-install.json') -Encoding utf8
if ($taskInstaller.ExitCode -ne 0) { throw "Installer failed with exit code $($taskInstaller.ExitCode). See .artifacts/desktop-install.json." }
if (-not $taskInstalledHash) { throw 'Installer exited successfully but the app executable is missing. See .artifacts/desktop-install.json.' }
if ($taskCurrentBuiltHash -ne $taskBuiltHash -or $taskCurrentInstallerHash -ne $taskPrepared.SHA256) { throw 'Build or installer changed during installation. See .artifacts/desktop-install.json.' }
if (-not $taskMatchMode) { throw 'Installed executable differs from the current build and its exact Tauri NSIS marker variant. Rebuild the package. See .artifacts/desktop-install.json.' }
if (-not $taskHasRegistration -or -not $taskHasShortcut -or -not (Test-Path -LiteralPath (Join-Path $InstallDir 'uninstall.exe') -PathType Leaf)) {
    throw 'Installation is missing its matching shortcut or uninstall registration. See .artifacts/desktop-install.json.'
}
Write-Output "Installed: $taskExePath"
Write-Output "Executable SHA256: $taskInstalledHash"
Write-Output "Executable comparison: $taskMatchMode"
Write-Output 'Installation evidence: .artifacts/desktop-install.json'
