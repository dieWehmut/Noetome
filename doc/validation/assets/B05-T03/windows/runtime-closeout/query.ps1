$ErrorActionPreference = 'Stop'
$venvExe = 'D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe'
$directExe = 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
$venvCode = "import sys,struct,importlib.metadata as m,json; print(json.dumps({'python':sys.version,'python_version':'.'.join(map(str,sys.version_info[:3])),'platform':sys.platform,'bits':struct.calcsize('P')*8,'executable':sys.executable,'base_executable':sys._base_executable,'prefix':sys.prefix,'base_prefix':sys.base_prefix,'packages':{n:m.version(n) for n in ['pdfplumber','pdfminer.six','pypdfium2','Pillow','charset-normalizer','cryptography','cffi','pycparser']}}))"
$directCode = "import sys,struct,json; print(json.dumps({'python':sys.version,'python_version':'.'.join(map(str,sys.version_info[:3])),'platform':sys.platform,'bits':struct.calcsize('P')*8,'executable':sys.executable,'base_executable':sys._base_executable,'prefix':sys.prefix,'base_prefix':sys.base_prefix}))"
$venvRaw = & $venvExe -c $venvCode
$venvExit = $LASTEXITCODE
if ($venvExit -ne 0) { exit $venvExit }
$directRaw = & $directExe -c $directCode
$directExit = $LASTEXITCODE
if ($directExit -ne 0) { exit $directExit }
$fingerprints = @{}
foreach ($exe in @($venvExe,$directExe)) {
  $item = Get-Item -LiteralPath $exe
  $fingerprints[$exe] = @{sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant(); byte_length=$item.Length}
}
@{kind='selected_native_runtime_metadata';checked_at_utc=[DateTime]::UtcNow.ToString('o');venv=($venvRaw|ConvertFrom-Json);direct=($directRaw|ConvertFrom-Json);venv_exit=$venvExit;direct_exit=$directExit;interpreter_fingerprints=$fingerprints} | ConvertTo-Json -Depth 8 -Compress
