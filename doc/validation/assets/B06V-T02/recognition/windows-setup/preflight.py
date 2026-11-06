import json,hashlib,importlib.metadata as m,sys,os,time
from pathlib import Path
r=Path('D:/Noetome/.artifacts/b06v-t02-scan-ocr-20261106a')
start=time.monotonic()
packages={d.metadata['Name']:d.version for d in m.distributions()}
models={f.name:{'sha256':hashlib.sha256(f.read_bytes()).hexdigest(),'byte_length':f.stat().st_size,'st_nlink':f.stat().st_nlink,'st_file_attributes':getattr(f.stat(),'st_file_attributes',0)} for f in (r/'model').iterdir()}
from pix2tex.cli import LatexOCR
import torch
report={'schema_version':1,'python_executable':sys.executable,'python_version':'.'.join(map(str,sys.version_info[:3])),'platform':sys.platform,'packages':packages,'torch_version':torch.__version__,'cuda_available':torch.cuda.is_available(),'model_files':models,'install_log_sha256':hashlib.sha256((r/'install.log').read_bytes()).hexdigest(),'install_lock_sha256':hashlib.sha256((r/'requirements-windows-lock.txt').read_bytes()).hexdigest(),'wall_ms':int((time.monotonic()-start)*1000),'model_instantiated':False,'recognition_calls':0,'paid_calls':0,'network_calls':0}
(r/'runtime-preflight.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
print(json.dumps({'status':'imported','python':sys.executable,'version':report['python_version'],'torch':report['torch_version'],'packages':len(packages),'models':len(models),'wall_ms':report['wall_ms']}))
