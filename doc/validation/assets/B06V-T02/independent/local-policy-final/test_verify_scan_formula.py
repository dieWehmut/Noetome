"""Independent scan attacks using synthetic outputs and a saved real error.

These tests never invoke OCR, a model, a network service or the product CLI.
The ordinary CLI audit and real recognition receipts are separate evidence.
"""
from __future__ import annotations

import copy
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


def synthetic_response(request_id, crop, expression=r"x_{i} = -\frac{a_{i}}{b_{i}}"):
    """A schema-shaped FAKE response, not an actual pix2tex execution."""
    return dict(schema_version=1, producer="pix2tex", package_version="0.1.4", request_id=request_id,
                input=V.fingerprint(crop), model={"release": "v0.0.1", "files": copy.deepcopy(V.MODEL_PINS)},
                recognized_tex=expression, issues=[], elapsed_ms=1, status="recognized", confidence=None,
                runtime={"python": "3.12.14", "packages": copy.deepcopy(V.PACKAGES)}, policy=copy.deepcopy(V.POLICY))


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
    for value, field in [("[原始公式区域](original.png)", "case.region"), ("[选定扫描页](page.png)", "case.page"), ("[识别原响应](raw-response.json)", "recognition"), ("[识别表达](recognized.tex)", "recognition.recognized_tex"), ("[归档 PDF](original.pdf)", "case.pdf")]:
        out.append(value+"\n\n"); labels.append(("attachment_link", field))
    raw = "".join(out).encode()
    nodes = [n for n in V.markdown_nodes(raw) if n["kind"] in ("literal", "math", "image", "link")]
    assert len(nodes) == len(labels)
    maps = [dict(kind=kind, source_field=field, start_byte=node["start"], end_byte=node["end"]) for node, (kind, field) in zip(nodes, labels)]
    return raw, dict(relative_path="扫描公式预览.md", content_sha256=V.fingerprint(raw)["sha256"], byte_length=len(raw), mappings=maps)


def synthetic_vault(vault, state="clear", profile_name="native"):
    """A fully labeled unit-only FAKE output tree. No CLI/model invocation."""
    vault.mkdir()
    case_root, profile_path = FIXTURE/"cases"/state, FIXTURE/"profiles"/(profile_name+".json")
    case, semantics, source = V.selected_source(case_root)
    profile_raw = V.read(profile_path); profile = V.strict_json(profile_raw)
    source["profile.json"] = profile_raw
    for name, raw in source.items(): (vault/name).write_bytes(raw)
    rid = str(uuid.uuid4())
    request = dict(schema_version=1, request_id=rid, input={"path": str((case_root/case["region"]["file"]).resolve()), **V.fingerprint(source["original.png"])}, model_directory=str(vault.parent/"synthetic-unit-test-model"), limits=copy.deepcopy(V.LIMITS))
    response = synthetic_response(rid, source["original.png"])
    decision, tokens = V.classify(case, semantics, profile, response)
    V.save(vault/".owner.json", dict(schema_version=1, producer="knowledge_scan_formula", request_id=rid, purpose="unpublished_scan_formula"))
    V.save(vault/"recognition-request.json", request); V.save(vault/"raw-response.json", response)
    (vault/"recognition-stderr.bin").write_bytes(b"synthetic unit-only response; no real recognition\n")
    (vault/"recognized.tex").write_bytes(response["recognized_tex"].encode())
    ready = decision["representation"] is not None
    files, plan_pin = [], None
    if ready:
        compiled = "".join(V.math_tokens(response["recognized_tex"])) if decision["representation"] == "native_math" else None
        plan = dict(schema_version=1, producer="knowledge_scan_formula", scope="selected_scan_formula_v1", request_id=rid, case=case, semantics=semantics, decision=decision, tokens=tokens, recognized_expression=response["recognized_tex"], compiled_expression=compiled)
        V.save(vault/"plan.json", plan); plan_pin = V.fingerprint(V.read(vault/"plan.json"))
        if compiled is not None: (vault/"compiled.tex").write_bytes(compiled.encode())
        raw, entry = synthetic_note(case, semantics, decision, response["recognized_tex"], compiled)
        (vault/"扫描公式预览.md").write_bytes(raw); files.append(entry)
    known = tokens is not None
    report = {**V.COMMON, "request_id": rid, "status": "preview_ready" if ready else "needs_review", "preview_status": "saved" if ready else "blocked", "case": case, "semantics": semantics, "profile": profile,
              "checks": dict(source_preservation="passed", region_match="passed", recognition="passed", expression_match="passed" if known else "needs_review", symbol_definitions="passed" if known else "needs_review", semantic_review="passed" if ready else "needs_review", note_plan="passed" if ready else "not_run", rendering="passed" if ready else "not_run", saved_output="passed"), "decision": decision, "recognition": response,
              "recognition_invocations": 1, "recognition_request_fingerprint": V.fingerprint(V.read(vault/"recognition-request.json")), "recognition_response_fingerprint": V.fingerprint(V.read(vault/"raw-response.json")),
              "runtime_inputs": {**V.MODEL_PINS, "python": {"sha256": "a"*64, "byte_length": 12345}, "adapter.py": V.fingerprint(V.read(ROOT/"sidecars/scan_formula/adapter.py"))}, "tokens": tokens, "plan_fingerprint": plan_pin, "inputs": V.pins(source), "files": files, "artifacts": {}}
    V.save(vault/"scan.json", report); (vault/"scan.md").write_bytes(b"")
    V.resynchronize_result(vault)
    return case_root, profile_path, FIXTURE/("expected-"+state+".json")


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
            decision,_=V.classify(c,s,profile,response)
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

    def test_request_is_crop_only_and_cannot_contain_author_expression(self):
        case,_,source=V.selected_source(FIXTURE/"cases/clear")
        q=dict(schema_version=1,request_id="test",input={"path":"/synthetic/region.png",**V.fingerprint(source["original.png"])},model_directory="/synthetic/model",limits=copy.deepcopy(V.LIMITS))
        V.verify_request(q,"test",source["original.png"])
        for key,value in (("expected","x_i=-a_i/b_i"),("semantics",{"expression":"x"}),("profile",{})):
            wrong=copy.deepcopy(q);wrong[key]=value
            with self.subTest(key=key),self.assertRaises(AssertionError): V.verify_request(wrong,"test",source["original.png"])
        wrong=copy.deepcopy(q);wrong["input"]["path"]="/synthetic/semantics.json"
        with self.assertRaises(AssertionError): V.verify_request(wrong,"test",source["original.png"])

    def test_recorded_windows_verbatim_prefix_is_same_explicit_crop_without_alias_read(self):
        V.same_saved_path(r"\\?\D:\Case\region.png", "D:/case/region.png")
        V.same_saved_path("/tmp/selected/region.png", "/tmp/selected/region.png")
        for actual,selected in (("D:/different/region.png","D:/case/region.png"),("/tmp/selected/../private/region.png","/tmp/selected/region.png")):
            with self.subTest(actual=actual),self.assertRaises(AssertionError): V.same_saved_path(actual,selected)

    def test_response_runtime_model_policy_confidence_and_source_are_strict(self):
        _,_,source=V.selected_source(FIXTURE/"cases/clear"); crop=source["original.png"]; r=synthetic_response("test",crop)
        V.verify_response(r,"test",crop)
        edits=[lambda v:v.update(confidence=0.99),lambda v:v["policy"].update(network_calls=1),lambda v:v["policy"].update(automatic_downloads=True),lambda v:v["model"]["files"]["weights.pth"].update(sha256="f"*64),lambda v:v["runtime"]["packages"].update(pix2tex="0.1.3"),lambda v:v.update(request_id="other"),lambda v:v["input"].update(byte_length=1),lambda v:v.update(status="recognized",recognized_tex=None),lambda v:v.update(elapsed_ms=True)]
        for edit in edits:
            wrong=copy.deepcopy(r);edit(wrong)
            with self.subTest(edit=edit),self.assertRaises(AssertionError): V.verify_response(wrong,"test",crop)

    def test_guessing_known_answer_cannot_heal_missing_sign_or_subscript(self):
        profile=V.strict_json(V.read(FIXTURE/"profiles/native.json"))
        for state in ("unreadable_sign","unreadable_subscript"):
            c,s,source=V.selected_source(FIXTURE/"cases"/state); decision,tokens=V.classify(c,s,profile,synthetic_response("test",source["original.png"]))
            self.assertIsNone(decision["representation"]);self.assertIsNone(tokens)
            self.assertEqual(decision["issue_codes"],["SCAN_REGION_UNREADABLE","SCAN_CRITICAL_SYMBOL_UNKNOWN","SCAN_SEMANTICS_UNKNOWN"])

    def test_recognition_success_is_not_semantic_match(self):
        c,s,source=V.selected_source(FIXTURE/"cases/clear");p=V.strict_json(V.read(FIXTURE/"profiles/native.json"))
        for expr,issue in ((r"x_i=\frac{a_i}{b_i}","SCAN_RECOGNITION_SEMANTIC_MISMATCH"),(r"x_i=-\frac{\bar{a_i}}{b_i}","SCAN_UNSUPPORTED_RECOGNITION")):
            decision,_=V.classify(c,s,p,synthetic_response("test",source["original.png"],expr))
            self.assertIsNone(decision["representation"]);self.assertIn(issue,decision["issue_codes"])


class ScanOutputAttackTests(unittest.TestCase):
    """All generated outputs are synthetic unit fixtures, never real recognition."""
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.base=Path(self.temp.name)

    def sample(self,state="clear",profile="native",name="vault"):
        vault=self.base/name;args=synthetic_vault(vault,state,profile);return vault,args

    def verify(self,vault,args):
        return V.verify_output(args[0],args[1],vault,args[2])

    def test_synthetic_complete_native_fallback_and_blocked_trees_are_checked(self):
        for i,(state,profile) in enumerate((("clear","native"),("clear","unverified"),("unreadable_sign","native"),("unreadable_subscript","native"))):
            vault,args=self.sample(state,profile,str(i));result=self.verify(vault,args);self.assertEqual(result["status"],"passed");self.assertEqual(result["runtime_byte_binding"],"not_run")

    def test_full_mutation_runner_rejects_all_sixteen_attacks_on_synthetic_baseline(self):
        vault,args=self.sample()
        attacks=V.tamper_saved_outputs(args[0],args[1],vault,args[2],self.base/"mutations",source_mode="synthetic unit-only fixture; not real OCR")
        self.assertEqual(len(attacks),16)
        self.assertTrue(all(a["actual_rejection"] for a in attacks))

    def test_synchronized_response_and_all_math_outputs_still_reject_missing_sign(self):
        vault,args=self.sample();r=V.strict_json(V.read(vault/"raw-response.json"));r["recognized_tex"]=r["recognized_tex"].replace("= -","= ");V.save(vault/"raw-response.json",r)
        (vault/"recognized.tex").write_bytes(r["recognized_tex"].encode());(vault/"compiled.tex").write_bytes("".join(V.math_tokens(r["recognized_tex"])).encode())
        p=V.strict_json(V.read(vault/"plan.json"));p.update(recognized_expression=r["recognized_tex"],compiled_expression=V.read(vault/"compiled.tex").decode());V.save(vault/"plan.json",p)
        V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"derived semantic outcome"):self.verify(vault,args)

    def test_synchronized_compiled_plan_cannot_come_from_expected_instead_of_response(self):
        vault,args=self.sample();p=V.strict_json(V.read(vault/"plan.json"));p["compiled_expression"]=p["compiled_expression"].replace("b_i","b_j");V.save(vault/"plan.json",p);(vault/"compiled.tex").write_bytes(p["compiled_expression"].encode());V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"compiled expression from actual recognition"):self.verify(vault,args)

    def test_synchronized_actual_display_math_cannot_drop_minus(self):
        vault,args=self.sample();p=vault/"扫描公式预览.md";p.write_bytes(V.read(p).replace(b"x_i=-",b"x_i="));V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"actual compiled text bytes"):self.verify(vault,args)

    def test_labels_cannot_reassign_same_flattened_definition_and_unit_values(self):
        vault,args=self.sample();p=vault/"扫描公式预览.md";raw=V.read(p).decode();raw=raw.replace("定义：","交换：",1).replace("单位：","定义：",1).replace("交换：","单位：",1);p.write_bytes(raw.encode());V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"symbol-specific definition and unit"):self.verify(vault,args)

    def test_removed_condition_rejects_after_all_mappings_and_hashes_updated(self):
        vault,args=self.sample();p=vault/"扫描公式预览.md";p.write_bytes(V.read(p).replace(b"```text\na_i >= 0.\n```\n\n",b"",1));r=V.strict_json(V.read(vault/"scan.json"));r["files"][0]["mappings"]=[m for m in r["files"][0]["mappings"] if m["source_field"]!="semantics.conditions[0]"];V.save(vault/"scan.json",r);V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"all actual literal conditions"):self.verify(vault,args)

    def test_sync_source_plan_report_and_pins_cannot_change_author_unit_association(self):
        vault,args=self.sample();case_root=self.base/"changed-case";shutil.copytree(args[0],case_root)
        s=V.strict_json(V.read(case_root/"semantics.json"));s["symbols"][0]["unit"]="dimensionless";V.save(case_root/"semantics.json",s)
        c=V.strict_json(V.read(case_root/"case.json"));c["semantics"]["sha256"]=V.fingerprint(V.read(case_root/"semantics.json"))["sha256"];V.save(case_root/"case.json",c)
        (vault/"source-semantics.json").write_bytes(V.read(case_root/"semantics.json"));(vault/"source-case.json").write_bytes(V.read(case_root/"case.json"));V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"independent author semantics symbols"):V.verify_output(case_root,args[1],vault,args[2])

    def test_entire_visible_report_shell_refuses_extra_accuracy_claim(self):
        vault,args=self.sample();p=vault/"scan.md";p.write_bytes(V.read(p)+"科学准确率100%。\n".encode())
        with self.assertRaisesRegex(AssertionError,"complete visible report scope"):self.verify(vault,args)

    def test_unknown_scan_cannot_inherit_clear_success_plan_and_preview(self):
        bad,args=self.sample("unreadable_sign",name="unknown");good,_=self.sample(name="clear");r=V.strict_json(V.read(bad/"scan.json"));g=V.strict_json(V.read(good/"scan.json"))
        for name in ("plan.json","compiled.tex","扫描公式预览.md"):shutil.copyfile(good/name,bad/name)
        for key in ("status","preview_status","decision","tokens","checks","files"):r[key]=g[key]
        V.save(bad/"scan.json",r);V.resynchronize_result(bad)
        with self.assertRaisesRegex(AssertionError,"derived semantic outcome"):self.verify(bad,args)

    def test_hidden_success_file_on_blocked_case_refused_even_with_fresh_artifact_sha(self):
        vault,args=self.sample("unreadable_subscript");(vault/"compiled.tex").write_bytes(b"x_i=-a_i/b_i");V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"complete inventory"):self.verify(vault,args)

    def test_crop_only_request_cannot_be_replaced_by_semantics_after_all_hashes_refreshed(self):
        vault,args=self.sample();r=V.strict_json(V.read(vault/"recognition-request.json"));r["input"]["path"]=r["input"]["path"].replace("region.png","semantics.json");V.save(vault/"recognition-request.json",r);V.resynchronize_result(vault)
        with self.assertRaisesRegex(AssertionError,"only the held crop"):self.verify(vault,args)


if __name__ == "__main__":
    unittest.main()
