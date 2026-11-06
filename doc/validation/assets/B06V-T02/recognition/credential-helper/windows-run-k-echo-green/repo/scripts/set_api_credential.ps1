# Windows CurrentUser DPAPI. No plaintext key argument and no implicit credential lookup.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$CredentialFile,
    [Parameter(Mandatory = $true)][string]$ServiceUrl,
    [Parameter(Mandatory = $true)][string]$Model,
    [switch]$Dialog,
    [string]$FromEnvironment
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-CredentialPath {
    param([string]$Path)
    # Local, canonical drive paths only: exclude device paths, UNC, ADS and aliases.
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path.Substring(2).Contains(':')) { throw 'path-invalid' }
    foreach ($part in ($Path.Substring(3) -split '[\\/]')) {
        if ($part -eq '.' -or $part -eq '..' -or $part -match '[. ]$' -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'path-invalid' }
    }
    $full = [IO.Path]::GetFullPath($Path)
    $repository = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\', '/')
    if ($full.Equals($repository, [StringComparison]::OrdinalIgnoreCase) -or
        $full.StartsWith($repository + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'path-in-repository' }
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            if (([IO.File]::GetAttributes($cursor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'path-link' }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    if (Test-Path -LiteralPath $full) { throw 'destination-exists' }
    if (-not [IO.Directory]::Exists([IO.Path]::GetDirectoryName($full))) { throw 'parent-missing' }
    return $full
}

function Test-CredentialInput {
    param([Security.SecureString]$Secure)
    if ($null -eq $Secure -or $Secure.Length -lt 1 -or $Secure.Length -gt 8192) { return $false }
    $pointer = [IntPtr]::Zero
    try {
        $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
        for ($index = 0; $index -lt $Secure.Length; $index++) {
            $character = [Runtime.InteropServices.Marshal]::ReadInt16($pointer, $index * 2)
            if ($character -lt 33 -or $character -gt 126) { return $false }
        }
        return $true
    }
    finally {
        if ($pointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
    }
}

function New-CredentialDialog {
    param([string]$ServiceUrl, [string]$Model)
    Add-Type -AssemblyName PresentationFramework
    Add-Type -AssemblyName PresentationCore
    Add-Type -AssemblyName WindowsBase
    $window = New-Object System.Windows.Window
    $window.Title = 'Noetome API credential'
    $window.Width = 560
    $window.SizeToContent = 'Height'
    $window.ResizeMode = 'NoResize'
    $window.WindowStartupLocation = 'CenterScreen'
    $panel = New-Object System.Windows.Controls.StackPanel
    $panel.Margin = '20'
    $label = New-Object System.Windows.Controls.TextBlock
    $label.Text = "Service: $ServiceUrl`nModel: $Model`nEnter API key (stored for this Windows user only):"
    $label.TextWrapping = 'Wrap'
    $label.Margin = '0,0,0,12'
    $panel.Children.Add($label) | Out-Null
    $password = New-Object System.Windows.Controls.PasswordBox
    $password.MaxLength = 8193
    $password.Margin = '0,0,0,12'
    $panel.Children.Add($password) | Out-Null
    $status = New-Object System.Windows.Controls.TextBlock
    $status.TextWrapping = 'Wrap'
    $status.Margin = '0,0,0,12'
    $panel.Children.Add($status) | Out-Null
    $buttons = New-Object System.Windows.Controls.StackPanel
    $buttons.Orientation = 'Horizontal'
    $save = New-Object System.Windows.Controls.Button
    $save.Content = 'Save'
    $save.Width = 100
    $save.IsDefault = $false
    $save.Margin = '0,0,12,0'
    $cancel = New-Object System.Windows.Controls.Button
    $cancel.Content = 'Cancel'
    $cancel.Width = 100
    $cancel.IsCancel = $true
    $buttons.Children.Add($save) | Out-Null
    $buttons.Children.Add($cancel) | Out-Null
    $panel.Children.Add($buttons) | Out-Null
    $window.Content = $panel
    return @{ Window = $window; PasswordBox = $password; Save = $save; Cancel = $cancel; Status = $status }
}

$secure = $null
$stream = $null
$created = $false
$saved = $false
$diagnostic = 'operation-failed'
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { $diagnostic = 'windows-required'; throw 'stop' }
    $diagnostic = 'path-rejected'
    $destination = Assert-CredentialPath $CredentialFile
    $diagnostic = 'configuration-invalid'
    $origin = $null
    if (-not [Uri]::TryCreate($ServiceUrl, [UriKind]::Absolute, [ref]$origin) -or
        $origin.Scheme -ne 'https' -or $origin.UserInfo -or $origin.AbsolutePath -ne '/' -or
        $origin.Query -or $origin.Fragment -or -not $origin.Host -or
        $Model -notmatch '^[\x21-\x7e]{1,256}$') { throw 'stop' }
    if ($Dialog -and $FromEnvironment) { throw 'stop' }
    if ($PSBoundParameters.ContainsKey('FromEnvironment') -and $FromEnvironment -cnotmatch '^[A-Z][A-Z0-9_]{1,63}$') { throw 'stop' }
    $diagnostic = 'input-invalid'
    if ($Dialog) {
        $ui = New-CredentialDialog -ServiceUrl $ServiceUrl -Model $Model
        $state = @{ Secure = $null }
        $validateInput = ${function:Test-CredentialInput}
        $ui.Save.Add_Click({
            $candidate = $ui.PasswordBox.SecurePassword
            if (-not (& $validateInput $candidate)) {
                $candidate.Dispose()
                $ui.Status.Text = 'Use 1 to 8192 printable ASCII characters, without spaces.'
                return
            }
            $state.Secure = $candidate
            $ui.PasswordBox.Clear()
            $ui.Window.DialogResult = $true
        }.GetNewClosure())
        $ui.Cancel.Add_Click({ $ui.PasswordBox.Clear(); $ui.Window.DialogResult = $false }.GetNewClosure())
        $ui.Window.Add_ContentRendered({ $ui.PasswordBox.Focus() | Out-Null }.GetNewClosure())
        try {
            $accepted = $ui.Window.ShowDialog()
            $secure = $state.Secure
        }
        finally { $ui.PasswordBox.Clear() }
        if ($accepted -ne $true) { [Console]::Out.WriteLine('credential-set: cancelled'); exit 0 }
    }
    elseif ($FromEnvironment) {
        $inputValue = [Environment]::GetEnvironmentVariable($FromEnvironment, 'Process')
        try {
            if ([string]::IsNullOrEmpty($inputValue)) { throw 'stop' }
            $secure = ConvertTo-SecureString -String $inputValue -AsPlainText -Force
        }
        finally { $inputValue = $null }
    }
    else { $secure = Read-Host -Prompt 'API key' -AsSecureString }
    if (-not (Test-CredentialInput $secure)) { throw 'stop' }
    $diagnostic = 'save-failed'
    $encrypted = ConvertFrom-SecureString -SecureString $secure
    # Recheck immediately before CreateNew. FileMode.CreateNew prevents overwrite races.
    $destination = Assert-CredentialPath $CredentialFile
    $stream = [IO.File]::Open($destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $created = $true
    $bytes = [Text.Encoding]::ASCII.GetBytes($encrypted)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
    $stream.Dispose()
    $stream = $null
    $saved = $true
    [Console]::Out.WriteLine('credential-set: saved')
    [Console]::Out.WriteLine('credential-file: ' + $destination)
}
catch {
    [Console]::Error.WriteLine('credential-set: ' + $diagnostic)
    exit 2
}
finally {
    if ($null -ne $stream) { $stream.Dispose() }
    if ($created -and -not $saved) {
        try { [IO.File]::Delete($destination) } catch { }
    }
    if ($null -ne $secure) { $secure.Dispose() }
}
