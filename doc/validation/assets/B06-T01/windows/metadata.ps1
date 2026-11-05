$ErrorActionPreference = 'Stop'
$venvExe = 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
$directExe = 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
$obsidianExe = 'C:\Program Files\Obsidian\Obsidian.exe'
$metadata = @{}
$metadata.kind = 'B06-T01 pre-final selected environment verification'
$metadata.checked_at_utc = [DateTime]::UtcNow.ToString('o')
$metadata.windows = [Environment]::OSVersion.Version.ToString()
$metadata.powershell = $PSVersionTable.PSVersion.ToString()
$metadata.rustc = (& rustc --version 2>&1 | Out-String).Trim()
$metadata.cargo = (& cargo --version 2>&1 | Out-String).Trim()
$metadata.obsidian_exists = Test-Path -LiteralPath $obsidianExe -PathType Leaf
if ($metadata.obsidian_exists) {
  $item = Get-Item -LiteralPath $obsidianExe
  $metadata.obsidian_launcher = @{path=$item.FullName;file_version=$item.VersionInfo.FileVersion;product_version=$item.VersionInfo.ProductVersion;sha256=(Get-FileHash -LiteralPath $obsidianExe -Algorithm SHA256).Hash.ToLowerInvariant();byte_length=$item.Length}
}
$metadata.existing_obsidian_process_ids = @(Get-Process -Name Obsidian -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$metadata.existing_build_processes = @(Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Select-Object Id,ProcessName)
$venvCode = "import sys,struct,importlib.metadata as m,json; print(json.dumps({'python_version':'.'.join(map(str,sys.version_info[:3])),'platform':sys.platform,'bits':struct.calcsize('P')*8,'executable':sys.executable,'base_executable':sys._base_executable,'packages':{n:m.version(n) for n in ['pdfplumber','pdfminer.six','pypdfium2','Pillow','charset-normalizer','cryptography','cffi','pycparser']}}))"
$directCode = "import sys,struct,json; print(json.dumps({'python_version':'.'.join(map(str,sys.version_info[:3])),'platform':sys.platform,'bits':struct.calcsize('P')*8,'executable':sys.executable,'base_executable':sys._base_executable}))"
$venvRaw = & $venvExe -c $venvCode
$venvExit = $LASTEXITCODE
if ($venvExit -ne 0) { exit $venvExit }
$directRaw = & $directExe -c $directCode
$directExit = $LASTEXITCODE
if ($directExit -ne 0) { exit $directExit }
$metadata.venv = $venvRaw | ConvertFrom-Json
$metadata.direct = $directRaw | ConvertFrom-Json
$metadata.venv_exit = $venvExit
$metadata.direct_exit = $directExit
$metadata | ConvertTo-Json -Depth 8
