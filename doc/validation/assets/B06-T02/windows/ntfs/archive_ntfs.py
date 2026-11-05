from pathlib import Path, PureWindowsPath
import hashlib, json, shutil, stat, subprocess, zipfile, zlib
ROOT=Path('/mnt/d/Noetome/target/compiler-temp/native-b06-t02-94dae560292e')
REPO=Path('/home/lyinn/project/Noetome')
OUT=REPO/'doc/validation/assets/B06-T02/windows/ntfs'
OUT.mkdir(exist_ok=False)
def pin_bytes(b): return {'sha256':hashlib.sha256(b).hexdigest(),'byte_length':len(b)}
def pin(p): return pin_bytes(p.read_bytes())
def save(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def windows(p):
    q=PureWindowsPath(p); assert q.drive.upper()=='D:'
    return Path('/mnt/d').joinpath(*q.parts[1:])
pointer=json.loads((ROOT/'native-ntfs-pointer.json').read_text())
WORK=windows(pointer['work']);final=json.loads((WORK/'final.json').read_text())
assert pointer['status']==final['status']=='passed'
assert final['cli_calls']==29 and final['executed_admission_scenarios']==25 and len(final['commands'])==35
assert final['not_run_scenarios'][0]['winerror']==1314
assert all(c['protected_before_after_match'] and not c['timed_out'] for c in final['commands'])
assert all(c['removed'] and c['target_preserved'] for c in final['cleanup'])
assert final['protected_before_after_match']
assert (WORK/'protected-before.json').read_bytes()==(WORK/'protected-after.json').read_bytes()
cli=WORK/'probe-cli.exe';built=ROOT/'repo/target/debug/noetome.exe'
assert pin(cli)==pin(built)==final['cli']
copy=json.loads((WORK/'cli-copy.json').read_text());assert copy['copied_link_count']==1 and copy['built_link_count']==2
sources={}
for p in sorted(WORK.rglob('*')):
    assert not p.is_symlink()
    if p.is_file() and p!=cli: sources['probe/'+p.relative_to(WORK).as_posix()]=p
for attempt in (1,2):
    for suffix in ('.stdout.bin','.stderr.bin','.receipt.json'):
        p=ROOT/('ntfs-driver-attempt-'+str(attempt)+suffix);sources['drivers/'+p.name]=p
for name in ('probe-ntfs-final-c27e740eb38c.py','probe-ntfs-final-294e8c37c3e1.py'):
    sources['drivers/'+name]=ROOT/name
sources['native-ntfs-pointer.json']=ROOT/'native-ntfs-pointer.json'
external_roots=json.loads((WORK/'protected-before.json').read_text())['trees']
for root in external_roots:
    p=windows(root)
    if p!=WORK/'authored-linear-case':
        for f in sorted(p.rglob('*')):
            assert not f.is_symlink()
            if f.is_file():sources['external/'+p.name+'/'+f.relative_to(p).as_posix()]=f
assert (ROOT/'probe-ntfs-final-294e8c37c3e1.py').read_bytes()==(REPO/'doc/validation/assets/B06-T02/windows/probe_ntfs.py').read_bytes()
index={}
archive=OUT/'originals.zip'
with zipfile.ZipFile(archive,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for name,path in sources.items():
        body=path.read_bytes();index[name]={'source_path':str(path),**pin_bytes(body),'crc32':f'{zlib.crc32(body)&0xffffffff:08x}'}
        z.writestr(name,body)
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    assert set(z.namelist())==set(index) and len(z.namelist())==len(index)
    for name,value in index.items():
        body=z.read(name);assert pin_bytes(body)=={k:value[k] for k in ('sha256','byte_length')}
        assert f'{zlib.crc32(body)&0xffffffff:08x}'==value['crc32']==f'{z.getinfo(name).CRC:08x}'
        assert body==sources[name].read_bytes(), 'source changed during archive'
save(OUT/'archive-manifest.json',{'schema_version':1,'source_root':str(WORK),'files':index,'omitted_blobs':[{'path':str(cli),'reason':'executable retained only as fingerprint; fixed built CLI unchanged',**pin(cli)}]})
for source,name in [('final.json','ntfs-final.json'),('commands.json','commands.json'),('runtime.json','runtime.json'),('symlink-unavailable.json','symlink-unavailable.json'),('cli-copy.json','cli-copy.json')]:
    shutil.copyfile(WORK/source,OUT/name);assert (OUT/name).read_bytes()==(WORK/source).read_bytes()
shutil.copyfile(Path(__file__),OUT/'archive_ntfs.py')
(OUT/'README.md').write_text('''NTFS 专项实际通过，绑定源码 `3977ac70cbfc6d5caeb8ed5ce20e9b5fce5907e2` 和最终 CLI `4117afb19b915b56bef501826b1666d7c2e685e9544e6804835970a44c87c169`。

实际 35 条子命令：29 次普通 CLI、4 次独立 Python 核对、2 次 junction 创建；25 个路径接纳场景按预期拒绝，原输入和外部哨兵保持逐字不变。两处自身 junction 已按身份验证移除，四类硬链接别名逐项移除，原始目标保留。

目录 symlink 创建返回 WinError 1314，8 个计划场景未执行，未计入通过数。见 [实际终态](ntfs-final.json)、[命令清单](commands.json) 和 [不可用原件](symlink-unavailable.json)。这不代替 Rust 持有期测试或真实 Obsidian 验证。

首次驱动在 preflight 因 Cargo build 可执行文件有两个硬链接而退出，尚未创建 probe 根或调用 CLI；第二次改用字节相同、单链接的独立副本后通过。两个版本的驱动与两轮原流都在归档内。native 快照 repo、原 build 可执行文件均未改变。

[originals.zip](originals.zip) 保存本专项完整输入/输出、每条 argv/stdout/stderr/receipt、前后指纹、路径与清理原件以及独立外部哨兵。可执行文件不入库，其真实指纹、执行路径和链接计数见 [复制记录](cli-copy.json)。[归档清单](archive-manifest.json) 给出每项原路径、SHA-256、长度和 CRC32；[实际读回验证](final-readback.json) 记录逐条解压读回与原件比较结果。
''',encoding='utf-8')
snapshot=json.loads((ROOT/'snapshot.json').read_text())
assert snapshot['head']==final['snapshot_head']
for name,value in snapshot['files'].items():assert pin(ROOT/'repo'/name)==value, 'native frozen source changed: '+name
comparison_names=[name for name in snapshot['files'] if name.startswith('crates/noetome-cli/src/') or name in ('scripts/verify_process_flow.py','tests/test_verify_process_flow.py')]
for name in comparison_names: assert pin(REPO/name)==snapshot['files'][name], 'current workspace source mismatch: '+name
artifacts={p.name:pin(p) for p in OUT.iterdir() if p.is_file()}
readback={'schema_version':1,'status':'passed','snapshot_head':snapshot['head'],'native_snapshot_files_rehashed':len(snapshot['files']),'native_snapshot_all_sha_match':True,'workspace_source_files_compared':len(comparison_names),'workspace_source_all_sha_match':True,'workspace_git_head_at_readback':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'archive_file_count':len(index),'archive_uncompressed_bytes':sum(v['byte_length'] for v in index.values()),'all_zip_entries_crc32_and_sha256_read_back_match':True,'all_archive_bytes_equal_actual_originals':True,'all_original_protected_before_after_pins_equal':True,'final_cli':pin(built),'copied_cli':pin(cli),'current_driver':pin(REPO/'doc/validation/assets/B06-T02/windows/probe_ntfs.py'),'driver_attempt_exits':[json.loads((ROOT/f'ntfs-driver-attempt-{i}.receipt.json').read_text())['exit_code'] for i in (1,2)],'commands':35,'ordinary_cli_calls':29,'executed_admission_scenarios':25,'symlink_not_run':{'winerror':1314,'planned_not_executed_scenarios':8},'artifacts':artifacts,'omission':'CLI executable blobs excluded; fingerprints retained. No target build or other agents native archives copied.'}
save(OUT/'final-readback.json',readback);assert json.loads((OUT/'final-readback.json').read_text())==readback
print(json.dumps({'output':str(OUT),'archive':pin(archive),'archive_files':len(index),'native_source_files':len(snapshot['files']),'workspace_source_files':len(comparison_names),'readback':pin(OUT/'final-readback.json')},ensure_ascii=False,indent=2))
