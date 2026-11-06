import importlib.util,pathlib,tempfile,shutil,json
spec=importlib.util.spec_from_file_location('scan_tests', pathlib.Path('tests/test_verify_scan_formula.py'))
t=importlib.util.module_from_spec(spec); spec.loader.exec_module(t); v=t.V
records=[]
with tempfile.TemporaryDirectory(prefix='noetome-review-directory-red-') as p:
 root=pathlib.Path(p)
 for label,name in [('ordinary','unexpected-empty-directory'),('nested','unexpected-directory/nested-empty')]:
  case=root/(label+'-case'); shutil.copytree(v.FIXTURE/'cases/clear',case); (case/name).mkdir(parents=True)
  c,s,_=v.selected_source(case); records.append(dict(scope='case',variant=label,observed='accepted-unexpected-directory',selection_id=c['selection_id']))
  vault=root/(label+'-vault'); args=t.synthetic_vault(vault); (vault/name).mkdir(parents=True)
  checked=v.verify_output(args[0],args[1],args[2],vault,args[3]); records.append(dict(scope='vault',variant=label,observed='accepted-unexpected-directory',independent=checked))
 print(json.dumps(dict(status='red',records=records,model_calls=0,network_calls=0,expected='four checker rejections'),ensure_ascii=False,indent=2))
 raise SystemExit(1)
