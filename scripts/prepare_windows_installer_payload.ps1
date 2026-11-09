[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$CliBinary,
    [Parameter(Mandatory=$true)][string]$RuntimeDir,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit,
    [string]$OutputDir,
    [string]$Quickstart,
    [ValidateSet('release','debug')][string]$BuildProfile='release',
    [switch]$PassThru
)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Installer payload preparation requires native Windows.' }
$taskRepoRoot=Split-Path $PSScriptRoot -Parent
if (-not $OutputDir) { $OutputDir=Join-Path $taskRepoRoot 'target\windows-installer-payload' }
if (-not $Quickstart) { $Quickstart=Join-Path $taskRepoRoot 'doc\windows\QUICKSTART.zh-CN.md' }
$OutputDir=[IO.Path]::GetFullPath($OutputDir)
if (Test-Path -LiteralPath $OutputDir) { throw 'Payload output exists; choose a new directory. Existing files are never replaced.' }
$taskPython=Join-Path $RuntimeDir 'python\python.exe'
if (-not (Test-Path -LiteralPath $taskPython -PathType Leaf)) { throw 'Complete locked runtime python/python.exe is missing.' }
$taskTemp=[IO.Path]::GetTempFileName()
$taskDriver=$taskTemp+'.py'
$taskDriverCreated=$false
$taskCode=@'
import hashlib, importlib.metadata, json, os, platform, shutil, stat, sys
from pathlib import Path
repo, cli, runtime, out, quick, commit, profile = map(str,sys.argv[1:])
repo,cli,runtime,out,quick=map(Path,(repo,cli,runtime,out,quick))
def sha(data):return hashlib.sha256(data).hexdigest()
def readjson(path):return json.loads(path.read_bytes().decode('utf-8-sig'))
def plain(path, allow_hardlinks=False):
    s=path.lstat()
    if stat.S_ISLNK(s.st_mode) or getattr(s,'st_file_attributes',0)&0x400:raise ValueError('linked/reparse source or destination')
    if not (stat.S_ISREG(s.st_mode) or stat.S_ISDIR(s.st_mode)):raise ValueError('nonordinary source or destination')
    if stat.S_ISREG(s.st_mode) and s.st_nlink!=1 and not allow_hardlinks:raise ValueError('hardlinked source or destination')
    return s
for root in (repo,runtime,out.parent,cli.parent,quick.parent):
    for path in (root,*root.parents):plain(path)
plain(cli,True);plain(quick)
if out.exists():raise ValueError('output exists')
lock=readjson(repo/'sidecars/pdf_parser/runtime-lock.json');receipt=readjson(runtime/'runtime-receipt.json')
assert receipt['archive_sha256']=='f38e68f4d612ade6dd50c894fc80b14c0be0c3b5201145d6fff5f20b9323204d'
assert receipt['python_executable_sha256']==sha((runtime/'python/python.exe').read_bytes())
assert receipt['requirements_sha256']==sha((repo/'sidecars/pdf_parser/requirements-lock.txt').read_bytes())
assert sys.version.split()[0]==lock['python_version']=='3.12.14' and sys.platform=='win32' and platform.machine().lower() in ('amd64','x86_64')
assert {n:importlib.metadata.version(n) for n in lock['packages']}==lock['packages']
assert receipt['identity']['runtime_lock_sha256']==sha((repo/'sidecars/pdf_parser/runtime-lock.json').read_bytes())
out.mkdir();sources={}
def copy_file(source,relative,origin,allow_hardlinks=False):
    plain(source,allow_hardlinks);data=source.read_bytes();dest=out/relative;dest.parent.mkdir(parents=True,exist_ok=True)
    with dest.open('xb') as handle:handle.write(data);handle.flush();os.fsync(handle.fileno())
    plain(dest);assert dest.read_bytes()==data;sources[relative]=origin

def copy_tree(source,prefix,origin):
    stack=[source]
    while stack:
        parent=stack.pop();plain(parent)
        for child in parent.iterdir():
            info=plain(child)
            if stat.S_ISDIR(info.st_mode):stack.append(child)
            else:
                rel=child.relative_to(source).as_posix();copy_file(child,prefix+'/'+rel,origin+'/'+rel)
copy_file(cli,'noetome.exe','build/noetome.exe',True)
copy_tree(runtime/'python','pdf-python','runtime/python')
copy_file(runtime/'python/vcruntime140.dll','vcruntime140.dll','runtime/python/vcruntime140.dll')
for name in ('invoke_text_api_credential.ps1','text_api_support.ps1','text_model_profile.ps1','set_text_api_credential.ps1'):copy_file(repo/'scripts'/name,'helpers/'+name,'repository/scripts/'+name)
for tree in ('run-checkpoint-v1','text-compare-v1','pdf-text-v1'):copy_tree(repo/'fixtures'/tree,'fixtures/'+tree,'repository/fixtures/'+tree)
copy_file(repo/'doc/validation/assets/B06V-T02/recognition/vision-live-final/actual-final-ledger.json','fixtures/model-ledger/actual-final-ledger.json','repository/frozen-full-ledger')
copy_file(repo/'doc/validation/assets/B06V-T02/review/continuation-current-ledger.json','fixtures/model-ledger/continuation-current-ledger.json','repository/frozen-ledger-proof')
for name in ('single-page.pdf','scanned-page.pdf'):copy_file(repo/'fixtures/pdf-text-v1'/name,'samples/'+name,'repository/fixtures/pdf-text-v1/'+name)
copy_file(quick,'QUICKSTART.zh-CN.md','repository/doc/windows/QUICKSTART.zh-CN.md')
for name in ('runtime-lock.json','requirements-lock.txt'):copy_file(repo/'sidecars/pdf_parser'/name,name,'repository/sidecars/pdf_parser/'+name)
copy_file(runtime/'runtime-receipt.json','runtime-receipt.json','runtime/runtime-receipt.json')
readme='Noetome Windows 11 x64\n\nSee QUICKSTART.zh-CN.md for Chinese setup and hands-on steps.\nThe packaged fixtures and author PDF are engineering samples, not reviewed scientific knowledge.\nThe full locked PDF runtime and licenses are preserved in pdf-python. Model credentials and cumulative usage stay external to this payload.\n'
(out/'README.md').write_bytes(readme.encode('utf-8'));sources['README.md']='generated/installer-entry'
rows=[]
for name in sorted(sources):
    data=(out/name).read_bytes();rows.append({'path':name,'bytes':len(data),'sha256':sha(data),'source':sources[name]})
manifest={'schema_version':1,'producer':'noetome-windows-installer-payload','source_commit':commit,'target':'x86_64-pc-windows-msvc','build_profile':profile,'version':readjson(repo/'crates/noetome-desktop/tauri.conf.json')['version'],'source_inputs':{'cli':{'bytes':cli.stat().st_size,'sha256':sha(cli.read_bytes()),'reference':'build/noetome.exe'},'runtime_receipt':{'bytes':(runtime/'runtime-receipt.json').stat().st_size,'sha256':sha((runtime/'runtime-receipt.json').read_bytes()),'reference':'runtime/runtime-receipt.json'}},'totals':{'files':len(rows),'bytes':sum(r['bytes'] for r in rows)},'files':rows}
raw=(json.dumps(manifest,ensure_ascii=False,sort_keys=True,indent=2)+'\n').encode('utf-8');(out/'payload-manifest.json').write_bytes(raw);(out/'payload-manifest.sha256').write_bytes((sha(raw)+'\n').encode('ascii'))
print(json.dumps({'root':str(out),'manifest':str(out/'payload-manifest.json'),'manifest_sha256':sha(raw),'files':len(rows),'bytes':manifest['totals']['bytes'],'source_commit':commit,'build_profile':profile}))
'@
try {
    $taskDriverStream=[IO.File]::Open($taskDriver,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    $taskDriverCreated=$true
    try {
        $taskDriverBytes=(New-Object Text.UTF8Encoding($false)).GetBytes($taskCode)
        $taskDriverStream.Write($taskDriverBytes,0,$taskDriverBytes.Length)
        $taskDriverStream.Flush($true)
    } finally { $taskDriverStream.Dispose() }
    $taskOutput=& $taskPython -I -B -X utf8 $taskDriver $taskRepoRoot $CliBinary $RuntimeDir $OutputDir $Quickstart $SourceCommit $BuildProfile
    if ($LASTEXITCODE -ne 0) { throw "Installer payload preparation failed with actual exit $LASTEXITCODE; partial output is retained." }
    $taskReceipt=$taskOutput | ConvertFrom-Json
    Write-Host "Installer payload preserved $($taskReceipt.files) files / $($taskReceipt.bytes) bytes; manifest SHA256 $($taskReceipt.manifest_sha256)"
    if ($PassThru) { $taskReceipt }
} finally {
    if ($taskDriverCreated) { Remove-Item -LiteralPath $taskDriver -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $taskTemp -ErrorAction SilentlyContinue
}
