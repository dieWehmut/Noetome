"""Read-only independent audit of actual Windows helper artificial-child receipts.

No private file, credential, HTTP service, CLI or model is opened or invoked.
This validates the helper receipt contract, not a full Core or live budget run.
"""
import importlib.util
import json
from pathlib import Path, PureWindowsPath

ROOT = next(p for p in Path(__file__).resolve().parents if (p/'scripts/verify_scan_formula.py').exists())
spec = importlib.util.spec_from_file_location('scan', ROOT/'scripts/verify_scan_formula.py')
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
OUT = Path(__file__).resolve().parent
EVIDENCE = ROOT/'doc/validation/assets/B06V-T02/recognition/credential-helper/windows-run-g/evidence'
records = []
for edition in ('powershell','pwsh'):
    directory=EVIDENCE/('ScanCredentials_'+edition)/'test_actual_four_phase_receipts_bind_selected_bytes_and_usage'
    source_before=v.pins(v.inventory(directory))
    relocation=v.strict_json(v.read(directory/'artifact-relocation.json'))
    v.exact(relocation['original_fixture_retained'],False,'original temporary fixture was cleaned, not a current private path')
    original=PureWindowsPath(relocation['original_fixture_directory'])
    def copied(ref):
        v.fields(ref,'path sha256 byte_length')
        path=PureWindowsPath(ref['path'])
        v.exact(path.parent,original,'only explicitly archived fixture basenames can resolve')
        local=directory/path.name
        v.exact(v.file_pin(local),{k:ref[k] for k in ('sha256','byte_length')},'actual mirrored helper receipt bytes')
        return local
    candidates=list(directory.glob('*.before.json'))
    v.exact(len(candidates),1,'one actual selected helper invocation')
    before_receipt=v.strict_json(v.read(candidates[0])); uid=before_receipt['invocation_id']; phases={}; ledgers={}
    fields='schema_version kind invocation_id phase recorded_at usage_ledger_reference ledger_snapshot selected_binary selected_config case_directory profile output_root reservation launch result settlement'
    for phase in ('before','reserved','launch','settled'):
        file=directory/('usage.json.scan-'+uid+'.'+phase+'.json')
        receipt=v.strict_json(v.read(file));v.fields(receipt,fields)
        for key,value in (('schema_version',1),('kind','scan_api_credential_invocation'),('invocation_id',uid),('phase',phase)):v.exact(receipt[key],value,'actual phase identity '+key)
        ledger=v.strict_json(v.read(copied(receipt['ledger_snapshot'])));v.ledger_schema(ledger)
        for key in ('selected_binary','selected_config','usage_ledger_reference','case_directory','profile','output_root','reservation'):v.exact(receipt[key],before_receipt[key],'same selected invocation across phases '+key)
        phases[phase]=receipt;ledgers[phase]=ledger
    before,reserved,launch,settled=(ledgers[p] for p in ('before','reserved','launch','settled'))
    config=v.strict_json(v.read(copied(before_receipt['selected_config'])))
    cap=before_receipt['reservation']['max_total_tokens'];v.exact(cap,config['budget']['max_total_tokens'],'helper reserves selected total cap');v.exact(before_receipt['reservation']['policy'],'bounded_request_total_cap','helper policy distinct from Core body reserve')
    v.exact(reserved['records'][:-1],before['records'],'reserve preserves prior history');v.exact(launch,reserved,'actual launch retains reserve')
    v.exact(reserved['pending_reserved_tokens'],before['pending_reserved_tokens']+cap,'actual reserved ledger arithmetic');v.exact(reserved['accounted_tokens'],before['accounted_tokens'],'actual prior confirmed usage preserved')
    for phase in ('before','reserved'):
        for key in ('launch','result','settlement'):v.exact(phases[phase][key],None,'no invented prelaunch output')
    marker=v.strict_json(v.read(directory/'child-started.json'))
    v.exact(phases['launch']['launch']['pid'],marker['parent_pid'],'actual selected venv launcher PID, independently recorded child parent')
    v.exact(phases['settled']['launch'],phases['launch']['launch'],'same observed actual child launch')
    result=phases['settled']['result'];v.exact(result['cli_exit_code'],3,'actual artificial child exit')
    output={}
    for stream in ('stdout','stderr'):
        entry=result[stream];local=copied(entry['saved_copy']);data=v.read(local)
        v.exact(entry['omission_reason'],None,'no secret echo in this explicitly artificial case')
        v.exact(v.fingerprint(data),dict(sha256=entry['original_sha256'],byte_length=entry['original_byte_length']),'actual original child UTF8 bytes')
        v.exact(data,v.read(directory/('capture-1.'+stream+'.txt')),'parent console retains exact selected child UTF8 output')
        output[stream]=data
    stdout=v.strict_json(output['stdout']);v.exact(stdout['unicode_probe'],'公式 xᵢ = −aᵢ/bᵢ','actual Unicode stdout survives PS encoding')
    v.exact(output['stderr'].decode(),'诊断：原始UTF-8字节 ✓\n','actual Unicode stderr survives PS encoding')
    usage=stdout['api']['usage']['model_usage'];v.exact(usage,dict(input_tokens=10,output_tokens=5,total_tokens=15),'artificial child actual usage receipt')
    record=settled['records'][-1];v.exact(record['invocation_id'],uid,'same original settled invocation');v.exact(record['confirmed_total_tokens'],usage['total_tokens'],'actual known synthetic usage settled');v.exact(record['actual_model'],stdout['api']['actual_model'],'actual artificial returned alias');v.exact(record['response_sha256'],stdout['api']['response_receipt']['sha256'],'artificial response fingerprint preserved')
    v.exact(settled['records'][:-1],before['records'],'settlement preserves earlier history');v.exact(settled['accounted_tokens'],before['accounted_tokens']+15,'settled arithmetic');v.exact(settled['pending_reserved_tokens'],before['pending_reserved_tokens'],'unused total-cap reserve released');v.exact(settled['model_attempts'],before['model_attempts']+1,'actual artificial child declared request count')
    v.exact(v.pins(v.inventory(directory)),source_before,'all helper mirror originals unchanged')
    records.append(dict(edition=edition,invocation_id=uid,helper_total_cap=cap,actual_artificial_tokens=15,original_phase_ledgers='passed',raw_unicode_stdio='passed',selected_venv_launcher_pid='passed',actual_binary_reread='not_run',full_core_report='not_run',live_budget='not_run',original_mirror_pins=source_before))
summary=dict(schema_version=1,status='passed',mode='read-only actual Windows helper with explicitly artificial child; not a live or Core semantic audit',phase_receipts=8,ledger_snapshots=8,editions=2,additional_cli_calls=0,additional_http_calls=0,real_model_calls=0,paid_calls=0,current_private_state_restored=False,records=records)
v.save(OUT/'receipt.json',summary)
print(json.dumps({k:value for k,value in summary.items() if k!='records'},ensure_ascii=False,indent=2))
