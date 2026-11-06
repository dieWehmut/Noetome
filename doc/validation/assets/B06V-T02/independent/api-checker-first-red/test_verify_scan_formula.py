"""Independent scan attacks using synthetic outputs and a saved real error.

These unit tests never invoke OCR, a model, a network service or the product CLI.
The ordinary CLI audit and real recognition receipts are separate evidence.
"""
from __future__ import annotations

import copy
import base64
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import uuid
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("verify_scan_formula", ROOT / "scripts/verify_scan_formula.py")
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)
FIXTURE = ROOT / "fixtures/scan-formula"


def synthetic_note(case, semantics, decision, expression, compiled):
    """Hand-built synthetic note for attacks; independent of production renderer."""
    out, labels = ["# 扫描公式预览\n\n## 标题\n\n"], []
    def literal(value, kind, field):
        out.append("```text\n"+value+"\n```\n\n"); labels.append((kind, field))
    literal(case["title"], "title", "case.title")
    out.append("## 公式\n\n")
    if decision["representation"] == "native_math":
        out.append("$$\n"+compiled+"\n$$\n\n"); labels.append(("expression", "compiled_expression"))
    else:
        out.append("![[original.png]]\n\n"); labels.append(("image_embed", "case.region"))
    out.append("该表达由选定扫描区域重建，并经符号与前提核对；作者原始 TeX 未知。\n\n## 识别原表达\n\n")
    literal(expression, "recognized_expression", "recognition.recognized_tex")
    out.append("## 符号定义\n\n")
    for i, symbol in enumerate(semantics["symbols"]):
        out.append(f"### 符号 {i+1}\n\n符号：\n\n")
        for label, key, kind in ((None, "symbol", "symbol"), ("定义：", "definition", "definition"), ("单位：", "unit", "unit")):
            if label: out.append(label+"\n\n")
            literal(symbol[key] if symbol[key] is not None else "来源未给出单位", kind, f"semantics.symbols[{i}].{key}")
    for section, key, kind in (("适用前提", "conditions", "condition"), ("范围", "ranges", "range"), ("例外", "exceptions", "exception")):
        out.append("## "+section+"\n\n")
        for i, value in enumerate(semantics[key]): literal(value, kind, f"semantics.{key}[{i}]")
        if not semantics[key]: out.append("选定来源未列出。\n\n")
    out.append("## 原始附件\n\n")
    for value, field in [("[原始公式区域](original.png)", "case.region"), ("[选定扫描页](page.png)", "case.page"), ("[识别原响应](response.bin)", "recognition"), ("[识别表达](recognized.tex)", "recognition.recognized_tex"), ("[归档 PDF](original.pdf)", "case.pdf")]:
        out.append(value+"\n\n"); labels.append(("attachment_link", field))
    raw = "".join(out).encode()
    nodes = [n for n in V.markdown_nodes(raw) if n["kind"] in ("literal", "math", "image", "link")]
    assert len(nodes) == len(labels)
    maps = [dict(kind=kind, source_field=field, start_byte=node["start"], end_byte=node["end"]) for node, (kind, field) in zip(nodes, labels)]
    return raw, dict(relative_path="扫描公式预览.md", content_sha256=V.fingerprint(raw)["sha256"], byte_length=len(raw), mappings=maps)


class ScanAuthorTests(unittest.TestCase):
    def test_one_independent_author_formula_three_scan_states_thirty_three_pins(self):
        self.assertEqual(V.verify_fixture(), dict(status="passed", author_formula_count=1, scan_states=3, frozen_files=33))

    def test_modified_expected_cannot_become_new_author_truth(self):
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory)/"expected-clear.json"; e=V.strict_json(V.read(FIXTURE/"expected-clear.json")); e["expression"]=e["expression"].replace("-", "+"); V.save(p,e)
            with self.assertRaisesRegex(AssertionError,"independent author expectation"): V.expectation(p)

    def test_only_single_script_operand_braces_are_equivalent(self):
        self.assertEqual(V.math_tokens(r"x_i=-\frac{a_i}{b_i}"), V.math_tokens(r"x_{i}=-\frac{a_{i}}{b_{i}}"))
        self.assertEqual(V.math_tokens(r"x^2=\frac{a}{b}"), V.math_tokens(r"x^{2}=\dfrac{a}{b}"))
        self.assertNotEqual(V.math_tokens(r"x_{i+1}=a"), V.math_tokens(r"x_i+1=a"))
        self.assertNotEqual(V.math_tokens(r"x={a+b}c"), V.math_tokens(r"x=a+bc"))

    def test_sign_every_subscript_and_fraction_order_remain_meaning(self):
        base=r"x_i=-\frac{a_i}{b_i}"
        for changed in (base.replace("=-", "="), base.replace("=-", "=+"), base.replace("x_i", "x_j"), base.replace("a_i", "a_j"), base.replace("b_i", "b_j"), r"x_i=-\frac{b_i}{a_i}"):
            with self.subTest(changed=changed): self.assertNotEqual(V.math_tokens(base), V.math_tokens(changed))

    def test_hallucinated_overbar_and_unknown_macros_cannot_be_removed(self):
        for bad in (r"x_i=-\frac{\bar{a_i}}{b_i}", r"{\bf x_i}=-\frac{a_i}{b_i}", r"x_i=-\frac{a_i}{\hat{b_i}}", r"x_i=-\href{https://example.org}{a_i}", "x_i=−a_i/b_i"):
            with self.subTest(bad=bad),self.assertRaises(AssertionError): V.math_tokens(bad)

    def test_historical_real_overbar_response_remains_blocked_for_native_and_fallback(self):
        raw=V.read(ROOT/"doc/validation/assets/B06V-T02/recognition/linux-direct-01/clear/response.json")
        self.assertEqual(V.fingerprint(raw)["sha256"],"b23483e35728697e2d2f95595616d76add9a7a5b4be268f92a724edade5e2196")
        response=V.strict_json(raw);self.assertIn(r"\bar",response["recognized_tex"])
        c,s,_=V.selected_source(FIXTURE/"cases/clear")
        for name in ("native","unverified"):
            profile=V.strict_json(V.read(FIXTURE/"profiles"/(name+".json")))
            candidate=dict(schema_version=1,recognized_tex=response["recognized_tex"],uncertainties=[])
            decision,_=V.classify(c,s,profile,candidate)
            self.assertIsNone(decision["representation"])
            self.assertIn("SCAN_UNSUPPORTED_RECOGNITION",decision["issue_codes"])

    def test_incomplete_formula_and_unbounded_nesting_rejected(self):
        for bad in ("x_i=", r"x_i=-\frac{}{b_i}", "x_^2", "("*65+"x"+")"*65, "x "*5000):
            with self.subTest(bad=bad),self.assertRaises(AssertionError): V.math_tokens(bad)

    def test_actual_page_crop_relation_checked_after_source_pin_resynchronization(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/"case"; shutil.copytree(FIXTURE/"cases/clear",root)
            c=V.strict_json(V.read(root/"case.json")); c["region"]["bounds"][0]+=1; V.save(root/"case.json",c)
            with self.assertRaisesRegex(AssertionError,"region pixels"): V.selected_source(root)

    def test_rectangle_bounds_not_only_png_dimensions(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/"case"; shutil.copytree(FIXTURE/"cases/clear",root)
            c=V.strict_json(V.read(root/"case.json")); c["region"]["bounds"][0]=799; V.save(root/"case.json",c)
            with self.assertRaisesRegex(AssertionError,"within selected page"): V.selected_source(root)

    def test_unknown_fields_missing_nullable_and_duplicate_json_are_rejected(self):
        for edit in (lambda c:c.pop("pdf"),lambda c:c.update(expected="never a provider input")):
            with tempfile.TemporaryDirectory() as directory:
                root=Path(directory)/"case"; shutil.copytree(FIXTURE/"cases/clear",root); c=V.strict_json(V.read(root/"case.json")); edit(c); V.save(root/"case.json",c)
                with self.assertRaises(AssertionError): V.selected_source(root)
        with self.assertRaises(AssertionError): V.strict_json(b'{"expression":null,"expression":"x"}')

    def test_ancestor_link_refused_before_source_read(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory); real=base/"real"; real.mkdir(); (real/"x").write_bytes(b"source")
            link=base/"link"
            try: link.symlink_to(real,target_is_directory=True)
            except OSError: self.skipTest("directory symbolic link unavailable on this host")
            with mock.patch.object(Path,"read_bytes",side_effect=AssertionError("must not read through link")) as read:
                with self.assertRaises(AssertionError): V.read(link/"x")
                read.assert_not_called()


    def test_recorded_windows_verbatim_prefix_is_same_explicit_crop_without_alias_read(self):
        V.same_saved_path(r'\\?\D:\Case\region.png','D:/case/region.png')
        V.same_saved_path('/tmp/selected/region.png','/tmp/selected/region.png')
        for a,b in (('D:/different/region.png','D:/case/region.png'),('/tmp/selected/../private/region.png','/tmp/selected/region.png')):
            with self.subTest(path=a),self.assertRaises(AssertionError): V.same_saved_path(a,b)


def synthetic_body(config,crop):
    """FAKE fixture bytes, no recognition request is actually transmitted."""
    task=V.TASK_PREFIX+json.dumps(V.RECOGNITION_SCHEMA,sort_keys=True,separators=(',',':'))+V.TASK_SUFFIX
    value=dict(model=config['model'],messages=[dict(role='system',content=task),dict(role='user',content=[dict(type='image_url',image_url=dict(url='data:image/png;base64,'+base64.b64encode(crop).decode(),detail='original'))])],response_format=dict(type='json_object'),max_tokens=config['limits']['max_output_tokens'],stream=False,thinking=dict(type='disabled'))
    return json.dumps(value,sort_keys=True,separators=(',',':')).encode()


def synthetic_vault(vault,state='clear',profile_name='native',response_edit=None,config_edit=None):
    """Hand-built, explicitly artificial unit output tree; no CLI/network/model."""
    vault.mkdir(); case_root=FIXTURE/'cases'/state; profile_path=FIXTURE/'profiles'/(profile_name+'.json'); api_path=vault.parent/(vault.name+'-api.json')
    config=V.artificial_config('http://127.0.0.1:8765/v1/chat/completions')
    if config_edit: config_edit(config)
    V.save(api_path,config); config_raw=V.read(api_path)
    case,semantics,source=V.selected_source(case_root); profile_raw=V.read(profile_path); profile=V.strict_json(profile_raw); source.update({'profile.json':profile_raw,'source-api-config.json':config_raw})
    for name,raw in source.items(): (vault/name).write_bytes(raw)
    rid=str(uuid.uuid4()); body=synthetic_body(config,source['original.png']); (vault/'request-body.json').write_bytes(body)
    request=dict(schema_version=1,producer='vision_scan_request',request_id=rid,request_strategy=V.STRATEGY,provider=config['provider'],transport_mode=config['transport_mode'],endpoint=config['endpoint'],configured_model=config['model'],credential_env=config['credential_env'],input=dict(path=str((case_root/case['region']['file']).resolve()),**V.fingerprint(source['original.png']),dimensions=case['region']['bounds'][2:]),config=dict(path=str(api_path.resolve()),fingerprint=V.fingerprint(config_raw)),limits=config['limits'],budget=config['budget'],reservation_policy=V.RESERVATION,input_reserved_tokens=len(body)+1024,reserved_tokens=len(body)+1024+config['limits']['max_output_tokens'],model_tokenizer=None,request_body=V.fingerprint(body))
    V.save(vault/'request.json',request); response=V.artificial_response()
    if response_edit: response_edit(response)
    raw=(json.dumps(response,ensure_ascii=False)+'\n').encode(); candidate,decode_issue=V.decode_recognition(raw); pre=V.preflight_issue(config,body)
    metadata_valid=True
    try: _,normalized,raw_usage=V.raw_metadata(raw)
    except AssertionError: metadata_valid=False; normalized=raw_usage=None
    issue=pre or ('SCAN_API_USAGE' if not metadata_valid else decode_issue)
    if issue: candidate=None
    attempt=0 if pre else 1
    api=dict(provider=config['provider'],transport_mode=config['transport_mode'],endpoint=config['endpoint'],request_strategy=V.STRATEGY,configured_model=config['model'],actual_model=response['model'] if attempt and metadata_valid else None,response_id=response['id'] if attempt and metadata_valid else None,service_tier=None,http_request_id=None,http_status=200 if attempt else None,response_complete=bool(attempt),finish_reason=response['choices'][0]['finish_reason'] if attempt and metadata_valid and len(response['choices'])==1 else None,confidence=None,usage=dict(network_attempts=attempt,real_model_calls=0,paid_calls=0,price_usd=None,model_tokenizer=None,model_usage=normalized if attempt else None,raw_model_usage=raw_usage if attempt else None,model_usage_status='validated' if attempt and metadata_valid else 'not_validated',input_reserved_tokens=len(body)+1024,reserved_tokens=len(body)+1024+config['limits']['max_output_tokens'],reservation_policy=V.RESERVATION,request_utf8_bytes=len(body)),response_receipt={**V.fingerprint(raw), 'omission_reason':None} if attempt else None,errors=[dict(code=issue,message='Artificial unit failure '+issue)] if issue else [])
    if attempt: (vault/'response.bin').write_bytes(raw)
    if candidate:
        V.save(vault/'recognition.json',candidate)
        if candidate['recognized_tex'] is not None: (vault/'recognized.tex').write_bytes(candidate['recognized_tex'].encode())
    decision,tokens=V.classify(case,semantics,profile,candidate,issue); ready=decision['representation'] is not None; files=[]; plan_pin=None
    raw_tex=candidate['recognized_tex'] if candidate else None
    if ready:
        compiled=''.join(V.math_tokens(raw_tex)) if decision['representation']=='native_math' else None
        plan=dict(schema_version=1,producer='knowledge_scan_formula',scope='selected_scan_formula_v1',request_id=rid,case=case,semantics=semantics,decision=decision,tokens=tokens,recognized_expression=raw_tex,compiled_expression=compiled); V.save(vault/'plan.json',plan); plan_pin=V.fingerprint(V.read(vault/'plan.json'))
        if compiled is not None: (vault/'compiled.tex').write_bytes(compiled.encode())
        note,file=synthetic_note(case,semantics,decision,raw_tex,compiled); (vault/'扫描公式预览.md').write_bytes(note); files.append(file)
    matched=False
    try: matched=tokens is not None and tokens==V.math_tokens(raw_tex)
    except (AssertionError,TypeError): pass
    known=tokens is not None
    report={**V.COMMON,'request_id':rid,'status':'preview_ready' if ready else 'needs_review','preview_status':'saved' if ready else 'blocked','network_calls':attempt,'paid_calls':0,'case':case,'semantics':semantics,'profile':profile,'checks':dict(source_preservation='passed',region_match='passed',recognition='passed' if candidate and raw_tex is not None and not candidate['uncertainties'] else 'needs_review',expression_match='passed' if matched else 'needs_review',symbol_definitions='passed' if known else 'needs_review',semantic_review='passed' if ready else 'needs_review',note_plan='passed' if ready else 'not_run',rendering='passed' if ready else 'not_run',saved_output='passed'),'decision':decision,'recognition':candidate,'api':api,'recognition_request_fingerprint':V.fingerprint(body),'recognition_response_fingerprint':V.fingerprint(raw) if attempt else None,'tokens':tokens,'plan_fingerprint':plan_pin,'inputs':V.pins(source),'files':files,'artifacts':{}}
    V.save(vault/'.owner.json',dict(schema_version=1,producer='knowledge_scan_formula',request_id=rid,purpose='unpublished_scan_formula')); V.save(vault/'scan.json',report); (vault/'scan.md').write_bytes(b''); V.resynchronize_result(vault)
    return case_root,api_path,profile_path,FIXTURE/('expected-'+state+'.json')


class ScanApiWireTests(unittest.TestCase):
    def setUp(self):
        _,_,source=V.selected_source(FIXTURE/'cases/clear'); self.crop=source['original.png']; self.config=V.artificial_config('http://127.0.0.1:8765/v1/chat/completions'); self.body=synthetic_body(self.config,self.crop)
    def test_only_numeric_loopback_or_exact_authorized_live_route(self):
        V.api_config_schema(self.config)
        live=V.strict_json(V.read(FIXTURE/'api-profiles/authorized-stop.json')); V.api_config_schema(live)
        for url in ('http://localhost:8765/v1/chat/completions','http://127.0.0.2:8765/v1/chat/completions','http://127.0.0.1:8765/v1/chat/completions?x=1','https://127.0.0.1:8765/v1/chat/completions','http://127.0.0.1:0/v1/chat/completions'):
            c=copy.deepcopy(self.config); c['endpoint']=url
            with self.subTest(url=url),self.assertRaises(AssertionError): V.api_config_schema(c)
    def test_nullable_config_fields_and_types_cannot_be_missing_or_coerced(self):
        for edit in (lambda c:c['budget'].pop('max_cost_usd'),lambda c:c['budget'].update(max_requests=True),lambda c:c['capabilities'].update(vision=1),lambda c:c['limits'].update(max_output_tokens=8193),lambda c:c.update(extra='not allowed')):
            c=copy.deepcopy(self.config); edit(c)
            with self.subTest(edit=edit),self.assertRaises(AssertionError): V.api_config_schema(c)
    def test_actual_body_only_exact_inline_original_crop(self):
        V.verify_body(self.body,self.config,self.crop)
        edits=(lambda b:b.update(semantics={'expression':'x_i=-a_i/b_i'}),lambda b:b['messages'][1]['content'].append({'type':'text','text':'author answer'}),lambda b:b['messages'][1]['content'][0]['image_url'].update(url='https://example.org/region.png'),lambda b:b['messages'][0].update(content=V.TASK_PREFIX+'{}'+V.TASK_SUFFIX+' source answer'),lambda b:b['messages'][1]['content'][0]['image_url'].update(detail='auto'),lambda b:b.update(stream=True))
        for edit in edits:
            b=V.strict_json(self.body); edit(b)
            with self.subTest(edit=edit),self.assertRaises(AssertionError): V.verify_body(json.dumps(b).encode(),self.config,self.crop)
    def test_normalized_recognition_null_requires_unknown_and_no_confidence(self):
        good=dict(schema_version=1,recognized_tex=None,uncertainties=[dict(kind='sign',description='Obscured mark.')]); V.recognition_schema(good)
        for edit in (lambda r:r.update(confidence=0.99),lambda r:r.update(uncertainties=[]),lambda r:r.update(schema_version=True),lambda r:r['uncertainties'][0].update(kind='pixel'),lambda r:r.pop('recognized_tex')):
            r=copy.deepcopy(good); edit(r)
            with self.subTest(edit=edit),self.assertRaises(AssertionError): V.recognition_schema(r)
    def test_chat_plain_single_stopped_candidate_and_role_are_strict(self):
        good=V.artificial_response(); parsed,issue=V.decode_recognition(json.dumps(good).encode()); self.assertIsNone(issue); self.assertEqual(parsed['recognized_tex'],r'x_{i} = -\frac{a_{i}}{b_{i}}')
        edits=((lambda r:r['choices'].append(copy.deepcopy(r['choices'][0])),'SCAN_API_OUTPUT'),(lambda r:r['choices'][0]['message'].update(tool_calls=None),'SCAN_API_OUTPUT'),(lambda r:r['choices'][0].update(finish_reason='length'),'SCAN_API_INCOMPLETE'),(lambda r:r['choices'][0]['message'].update(role='system'),'SCAN_API_OUTPUT'),(lambda r:r['choices'][0]['message'].update(refusal='refused'),'SCAN_API_REFUSAL'),(lambda r:r['choices'][0]['message'].update(content='```json\n{}\n```'),'SCAN_API_RECOGNITION_SCHEMA'),(lambda r:r['choices'][0].update(index=False),'SCAN_API_OUTPUT'))
        for edit,code in edits:
            r=copy.deepcopy(good); edit(r)
            with self.subTest(code=code): self.assertEqual(V.decode_recognition(json.dumps(r).encode())[1],code)
    def test_usage_is_derived_from_original_arithmetic_cache_and_details(self):
        r=V.artificial_response(); r['usage'].update(prompt_cache_hit_tokens=80,prompt_cache_miss_tokens=20,prompt_tokens_details={'cached_tokens':80},completion_tokens_details={'reasoning_tokens':10})
        _,usage,raw=V.raw_metadata(json.dumps(r).encode()); self.assertEqual(usage['input_tokens_details'],{'cached_tokens':80}); self.assertEqual(raw['prompt_cache_hit_tokens'],80)
        for edit in (lambda v:v['usage'].update(total_tokens=131),lambda v:v['usage'].update(prompt_tokens=True),lambda v:v['usage'].update(prompt_cache_miss_tokens=100),lambda v:v['usage']['prompt_tokens_details'].update(cached_tokens=101),lambda v:v['usage']['completion_tokens_details'].update(reasoning_tokens=31)):
            bad=copy.deepcopy(r); edit(bad)
            with self.subTest(edit=edit),self.assertRaises(AssertionError): V.raw_metadata(json.dumps(bad).encode())
    def test_missing_details_stay_missing_and_duplicate_usage_is_invalid(self):
        _,u,_=V.raw_metadata(json.dumps(V.artificial_response()).encode()); self.assertNotIn('input_tokens_details',u); self.assertNotIn('output_tokens_details',u)
        with self.assertRaises(AssertionError): V.raw_metadata(b'{"usage":{"prompt_tokens":1,"prompt_tokens":2}}')
    def test_budget_reserves_actual_body_and_prevents_unproved_requests(self):
        self.assertIsNone(V.preflight_issue(self.config,self.body))
        for edit,issue in ((lambda c:c['budget'].update(max_requests=0),'SCAN_BUDGET_STOP'),(lambda c:c['budget'].update(max_total_tokens=1),'SCAN_BUDGET_STOP'),(lambda c:c['budget'].update(max_cost_usd='1.0'),'SCAN_PRICE_UNKNOWN'),(lambda c:c['limits'].update(max_input_bytes=1),'SCAN_API_INPUT_LIMIT'),(lambda c:c['capabilities'].update(structured_output=False),'SCAN_API_CAPABILITY')):
            c=copy.deepcopy(self.config); edit(c)
            with self.subTest(issue=issue): self.assertEqual(V.preflight_issue(c,self.body),issue)


class ScanOutputAttackTests(unittest.TestCase):
    def setUp(self): self.tmp=tempfile.TemporaryDirectory(); self.base=Path(self.tmp.name)
    def tearDown(self): self.tmp.cleanup()
    def sample(self,state='clear',profile='native',name='vault',**kwargs):
        vault=self.base/name; args=synthetic_vault(vault,state,profile,**kwargs); return vault,args
    def verify(self,vault,args): return V.verify_output(args[0],args[1],args[2],vault,args[3])
    def test_valid_native_and_original_fallback_are_distinct(self):
        for name,profile,wanted in (('native','native','native_math'),('image','unverified','formula_image')):
            vault,args=self.sample(profile=profile,name=name); checked=self.verify(vault,args); self.assertEqual(checked['representation'],wanted); self.assertEqual(checked['real_model_calls'],0); self.assertEqual(checked['cumulative_budget_binding'],'not_run')
    def test_unknown_sign_and_subscript_are_not_repaired_by_guessed_clear_tex(self):
        for state in ('unreadable_sign','unreadable_subscript'):
            vault,args=self.sample(state,name=state); checked=self.verify(vault,args); self.assertIsNone(checked['representation']); self.assertIn('SCAN_CRITICAL_SYMBOL_UNKNOWN',checked['issue_codes'])
    def test_full_synchronized_mutation_matrix_all_rejected(self):
        vault,args=self.sample(); attacks=V.tamper_saved_outputs(args[0],args[1],args[2],vault,args[3],self.base/'attacks'); self.assertEqual(len(attacks),18); self.assertTrue(all(a['actual_rejection'] for a in attacks))
    def test_overbar_stays_math_rejection_after_valid_api_metadata(self):
        def edit(r): r['choices'][0]['message']['content']=json.dumps(dict(schema_version=1,recognized_tex=r'x_i=-\frac{\bar{a_i}}{b_i}',uncertainties=[]))
        vault,args=self.sample(response_edit=edit); checked=self.verify(vault,args); self.assertIn('SCAN_UNSUPPORTED_RECOGNITION',checked['issue_codes']); self.assertNotIn('SCAN_API_USAGE',checked['issue_codes'])
    def test_compiled_plan_cannot_use_expected_instead_of_actual_recognition(self):
        vault,args=self.sample(); p=V.strict_json(V.read(vault/'plan.json')); p['compiled_expression']=p['compiled_expression'].replace('b_i','b_j'); V.save(vault/'plan.json',p); (vault/'compiled.tex').write_bytes(p['compiled_expression'].encode()); V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,'compiled expression from actual recognition'): self.verify(vault,args)
    def test_unknown_scan_cannot_inherit_clear_plan_and_preview(self):
        bad,args=self.sample('unreadable_sign',name='unknown'); good,_=self.sample(name='clear'); r=V.strict_json(V.read(bad/'scan.json')); g=V.strict_json(V.read(good/'scan.json'))
        for name in ('plan.json','compiled.tex','扫描公式预览.md'): shutil.copyfile(good/name,bad/name)
        for key in ('status','preview_status','decision','tokens','checks','files'): r[key]=g[key]
        V.save(bad/'scan.json',r); V.resynchronize_result(bad)
        with self.assertRaisesRegex(AssertionError,'derived semantic outcome'): self.verify(bad,args)
    def test_hidden_success_file_on_blocked_case_even_with_fresh_sha(self):
        vault,args=self.sample('unreadable_subscript'); (vault/'compiled.tex').write_bytes(b'x_i=-a_i/b_i'); V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,'complete inventory'): self.verify(vault,args)
    def test_saved_crop_path_cannot_select_semantics_after_fresh_hashes(self):
        vault,args=self.sample(); q=V.strict_json(V.read(vault/'request.json')); q['input']['path']=q['input']['path'].replace('region.png','semantics.json'); V.save(vault/'request.json',q); V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,'only the held crop'): self.verify(vault,args)
    def test_raw_chat_and_normalized_recognition_must_agree(self):
        vault,args=self.sample(); r=V.strict_json(V.read(vault/'recognition.json')); r['recognized_tex']=r'x_i=-\frac{a_i}{b_j}'; V.save(vault/'recognition.json',r); V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,'derived from actual raw Chat'): self.verify(vault,args)
    def test_full_report_payload_types_not_bool_integer_coercion(self):
        vault,args=self.sample(); r=V.strict_json(V.read(vault/'scan.json')); r['network_calls']=True; V.save(vault/'scan.json',r); V.resynchronize_result(vault)
        with self.assertRaises(AssertionError): self.verify(vault,args)
    def test_unvalidated_usage_failure_still_preserves_original(self):
        vault,args=self.sample(response_edit=lambda r:r['usage'].update(total_tokens=131)); checked=self.verify(vault,args); self.assertIn('SCAN_API_USAGE',checked['issue_codes']); self.assertTrue((vault/'response.bin').exists()); self.assertFalse((vault/'recognized.tex').exists())
    def test_model_uncertainty_blocks_correct_looking_expression(self):
        def edit(r): r['choices'][0]['message']['content']=json.dumps(dict(schema_version=1,recognized_tex=r'x_i=-\frac{a_i}{b_i}',uncertainties=[dict(kind='sign',description='Artificial uncertainty.')]))
        vault,args=self.sample(response_edit=edit); checked=self.verify(vault,args); self.assertIn('SCAN_RECOGNITION_UNCERTAIN',checked['issue_codes']); self.assertFalse((vault/'plan.json').exists())
    def test_budget_zero_preflight_has_no_invented_response_or_usage(self):
        vault,args=self.sample(config_edit=lambda c:c['budget'].update(max_requests=0)); checked=self.verify(vault,args); self.assertIn('SCAN_BUDGET_STOP',checked['issue_codes']); self.assertFalse((vault/'response.bin').exists())
    def test_live_alias_is_observed_unknown_billing_confidence_not_zero_cost(self):
        vault,args=self.sample(); config=V.strict_json(V.read(args[1])); config.update(transport_mode='live',endpoint=V.LIVE_ENDPOINT,model='deepseek-flash',credential_env='NOETOME_TANGMAO_API_KEY'); V.save(args[1],config); (vault/'source-api-config.json').write_bytes(V.read(args[1])); body=V.strict_json(V.read(vault/'request-body.json')); body['model']='deepseek-flash'; V.save(vault/'request-body.json',body)
        q=V.strict_json(V.read(vault/'request.json')); q.update(transport_mode='live',endpoint=V.LIVE_ENDPOINT,configured_model='deepseek-flash',credential_env='NOETOME_TANGMAO_API_KEY'); q['config']['fingerprint']=V.fingerprint(V.read(args[1])); raw=V.read(vault/'request-body.json'); q.update(input_reserved_tokens=len(raw)+1024,reserved_tokens=len(raw)+1024+8192); V.save(vault/'request.json',q)
        r=V.strict_json(V.read(vault/'scan.json')); r['paid_calls']=None; r['api'].update(transport_mode='live',endpoint=V.LIVE_ENDPOINT,configured_model='deepseek-flash'); r['api']['usage'].update(real_model_calls=1,paid_calls=None,request_utf8_bytes=len(raw),input_reserved_tokens=len(raw)+1024,reserved_tokens=len(raw)+1024+8192); V.save(vault/'scan.json',r); V.resynchronize_result(vault)
        checked=self.verify(vault,args); self.assertEqual(checked['real_model_calls'],1); self.assertEqual(checked['cumulative_budget_binding'],'not_run')
        r=V.strict_json(V.read(vault/'scan.json')); r['api']['confidence']=0.99; V.save(vault/'scan.json',r); V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,'uncalibrated'): self.verify(vault,args)


if __name__=='__main__': unittest.main()
